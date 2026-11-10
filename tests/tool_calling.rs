//! Requires DATABASE_URL pointing to a disposable PostgreSQL instance with
//! CREATE DATABASE permission. sqlx creates and removes a database per test.
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use ai_gateway::{
    app::{AppState, build_app},
    domain::{SaveModelRouteRequestDto, SaveProviderRequestDto},
    providers::{OpenAiCompatibleProvider, ProviderRegistry},
    repositories::{PostgresAuthRepository, PostgresRequestsRepository, PostgresRoutesRepository},
    security::{SecretCrypto, token_hash},
    usecases::generate::GenerateService,
};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use base64::Engine;
use reqwest::Client;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::{net::TcpListener, task::JoinHandle};
use uuid::Uuid;

struct Server {
    url: String,
    task: JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn serve(app: Router) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Server { url, task }
}

struct Exchange {
    request: Value,
    response: Value,
    status: StatusCode,
    key: &'static str,
}
type Exchanges = Arc<Mutex<VecDeque<Exchange>>>;
async fn upstream(
    State(exchanges): State<Exchanges>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let exchange = exchanges
        .lock()
        .unwrap()
        .pop_front()
        .expect("unexpected upstream request");
    assert_eq!(headers["authorization"], format!("Bearer {}", exchange.key));
    assert_eq!(body, exchange.request, "actual upstream HTTP payload");
    (exchange.status, Json(exchange.response))
}
struct Harness {
    gateway: Server,
    _upstream: Server,
    exchanges: Exchanges,
    pool: PgPool,
    tenants: [Uuid; 2],
    clients: [Uuid; 2],
}
impl Harness {
    async fn new(pool: PgPool) -> Self {
        let exchanges = Exchanges::default();
        let upstream = serve(
            Router::new()
                .route("/v1/chat/completions", post(upstream))
                .with_state(exchanges.clone()),
        )
        .await;
        let owner: Uuid = sqlx::query_scalar("INSERT INTO users (email, password_hash, global_role) VALUES ('owner@test', 'unused', 'owner') RETURNING id")
            .fetch_one(&pool).await.unwrap();
        let crypto =
            SecretCrypto::from_base64(&base64::engine::general_purpose::STANDARD.encode([7u8; 32]))
                .unwrap();
        let routes = Arc::new(PostgresRoutesRepository::new(pool.clone()));
        let auth = Arc::new(PostgresAuthRepository::new(pool.clone()));
        let requests = Arc::new(PostgresRequestsRepository::new(pool.clone()));
        let mut tenants = [Uuid::nil(); 2];
        let mut clients = [Uuid::nil(); 2];
        for (i, suffix) in ["a", "b"].into_iter().enumerate() {
            tenants[i] =
                sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $1) RETURNING id")
                    .bind(format!("tenant-{suffix}"))
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            let provider = routes
                .save_provider(
                    tenants[i],
                    &SaveProviderRequestDto {
                        code: "mock".into(),
                        kind: "openai-compatible".into(),
                        base_url: format!("{}/v1/", upstream.url),
                        is_enabled: true,
                        timeout_ms: 1000,
                        api_key: None,
                    },
                )
                .await
                .unwrap();
            routes
                .save_provider_secret(
                    tenants[i],
                    provider.id,
                    &crypto.encrypt(&format!("secret-{suffix}")).unwrap(),
                    owner,
                )
                .await
                .unwrap();
            routes
                .save_model_route(
                    tenants[i],
                    &SaveModelRouteRequestDto {
                        alias: "smart-default".into(),
                        provider_code: "mock".into(),
                        external_model: format!("tenant-{suffix}-model"),
                        is_enabled: true,
                    },
                )
                .await
                .unwrap();
            clients[i] = auth
                .create_gateway_client(
                    tenants[i],
                    "factum",
                    &token_hash(&format!("client-{suffix}")),
                    "test",
                    owner,
                )
                .await
                .unwrap()
                .id;
        }
        auth.create_session(
            owner,
            &token_hash("admin-session"),
            chrono::Utc::now() + chrono::Duration::hours(1),
        )
        .await
        .unwrap();
        let generate = GenerateService::new(
            routes.clone(),
            requests.clone(),
            ProviderRegistry::new(vec![Arc::new(OpenAiCompatibleProvider::new().unwrap())]),
            crypto.clone(),
            1500,
            2000,
        );
        let gateway = serve(build_app(AppState {
            generate_service: Arc::new(generate),
            auth_repo: auth,
            routes_repo: routes,
            requests_repo: requests,
            secret_crypto: crypto,
            admin_dist_dir: "/nonexistent-ai-gateway-test-ui".into(),
        }))
        .await;
        Self {
            gateway,
            _upstream: upstream,
            exchanges,
            pool,
            tenants,
            clients,
        }
    }
    fn expect(&self, request: Value, response: Value, status: StatusCode, key: &'static str) {
        self.exchanges.lock().unwrap().push_back(Exchange {
            request,
            response,
            status,
            key,
        });
    }
    async fn generate(&self, request: &Value, token: &str) -> (StatusCode, Value) {
        let response = Client::new()
            .post(format!("{}/api/v1/generate", self.gateway.url))
            .bearer_auth(token)
            .json(request)
            .send()
            .await
            .unwrap();
        (response.status(), response.json().await.unwrap())
    }
    async fn history(&self, tenant: Uuid) -> Value {
        Client::new()
            .get(format!(
                "{}/api/admin/tenants/{tenant}/requests",
                self.gateway.url
            ))
            .bearer_auth("admin-session")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap()
    }
    fn assert_consumed(&self) {
        assert!(self.exchanges.lock().unwrap().is_empty());
    }
}

fn definition() -> Value {
    json!({"name":"weather", "description":"Weather lookup", "parameters":{
        "type":"object", "properties":{"city":{"type":"string"}}, "required":["city"], "additionalProperties":false
    }})
}
fn request() -> Value {
    json!({"model":"smart-default", "messages":[{"role":"user","content":"Weather?"}], "tools":[definition()], "tool_choice":"auto"})
}
fn upstream_request() -> Value {
    json!({"model":"tenant-a-model", "messages":[{"role":"user","content":"Weather?"}], "tools":[{"type":"function","function":definition()}], "tool_choice":"auto"})
}
fn call(id: &str, city: &str) -> Value {
    json!({"id":id,"type":"function","function":{"name":"weather","arguments":json!({"city":city}).to_string()}})
}
fn completion(content: Value, calls: Vec<Value>, reason: &str) -> Value {
    json!({"choices":[{"message":{"content":content,"tool_calls":calls},"finish_reason":reason}],"usage":{"prompt_tokens":17,"completion_tokens":9}})
}

#[sqlx::test]
async fn full_cycle_preserves_multiple_calls_usage_routing_and_tenant_isolation(pool: PgPool) {
    let h = Harness::new(pool).await;
    // A single invocation and multiple invocations, both without output text.
    for count in [1, 2] {
        let cities = &[("call_1", "Moscow"), ("call_2", "Paris")][..count];
        h.expect(
            upstream_request(),
            completion(
                Value::Null,
                cities.iter().map(|(id, city)| call(id, city)).collect(),
                "tool_calls",
            ),
            StatusCode::OK,
            "secret-a",
        );
        let (status, first) = h.generate(&request(), "client-a").await;
        assert_eq!(status, StatusCode::OK, "{first}");
        assert_eq!(first["output_text"], "");
        assert_eq!(first["finish_reason"], "tool_calls");
        assert_eq!(first["model"], "smart-default");
        assert_eq!(first["provider"], "mock");
        assert_eq!(first["usage"], json!({"input_tokens":17,"output_tokens":9}));
        let expected_calls: Vec<_> = cities
            .iter()
            .map(|(id, city)| json!({"id":id,"name":"weather","arguments":{"city":city}}))
            .collect();
        assert_eq!(first["tool_calls"], json!(expected_calls));
        let mut next = request();
        next["tool_choice"] = json!("none");
        let messages = next["messages"].as_array_mut().unwrap();
        messages.push(json!({"role":"assistant","tool_calls":first["tool_calls"]}));
        // Tool results may arrive in a different order; IDs must be preserved.
        for (id, _) in cities.iter().rev() {
            messages.push(json!({"role":"tool","tool_call_id":id,"content":"tool-result-secret"}));
        }
        let mut expected = upstream_request();
        expected["tool_choice"] = json!("none");
        let messages = expected["messages"].as_array_mut().unwrap();
        messages.push(json!({"role":"assistant","content":null,"tool_calls":cities.iter().map(|(id, city)|call(id, city)).collect::<Vec<_>>()}));
        for (id, _) in cities.iter().rev() {
            messages.push(json!({"role":"tool","tool_call_id":id,"content":"tool-result-secret"}));
        }
        h.expect(
            expected,
            completion(json!("Sunny"), vec![], "stop"),
            StatusCode::OK,
            "secret-a",
        );
        let (status, final_answer) = h.generate(&next, "client-a").await;
        assert_eq!(status, StatusCode::OK, "{final_answer}");
        assert_eq!(final_answer["output_text"], "Sunny");
        assert_eq!(final_answer["finish_reason"], "stop");
        assert!(final_answer.get("tool_calls").is_none());
    }
    // Same alias/provider code, different tenant key and upstream model.
    let mut expected = upstream_request();
    expected["model"] = json!("tenant-b-model");
    h.expect(
        expected,
        completion(json!("Tenant B"), vec![], "stop"),
        StatusCode::OK,
        "secret-b",
    );
    let mut other_tenant = request();
    other_tenant["tenant_id"] = json!(h.tenants[0]);
    assert_eq!(
        h.generate(&other_tenant, "client-b").await.0,
        StatusCode::OK
    );
    let a = h.history(h.tenants[0]).await;
    let b = h.history(h.tenants[1]).await;
    assert_eq!(a.as_array().unwrap().len(), 4);
    assert_eq!(b.as_array().unwrap().len(), 1);
    for entry in a.as_array().unwrap() {
        assert_eq!(entry["tenant_id"], json!(h.tenants[0]));
        assert_eq!(entry["gateway_client_id"], json!(h.clients[0]));
        assert_eq!(entry["external_model"], "tenant-a-model");
        assert_eq!(entry["input_tokens"], 17);
        assert_eq!(entry["output_tokens"], 9);
        assert!(entry["latency_ms"].as_i64().unwrap() >= 0);
        let record = PostgresRequestsRepository::new(h.pool.clone())
            .get_request(h.tenants[0], entry["id"].as_str().unwrap())
            .await
            .unwrap()
            .unwrap();
        assert!(record.prompt_preview.is_none());
        assert!(record.response_preview.is_none());
        assert!(
            PostgresRequestsRepository::new(h.pool.clone())
                .get_request(h.tenants[1], &record.id)
                .await
                .unwrap()
                .is_none()
        );
    }
    h.assert_consumed();
}

#[sqlx::test]
async fn old_text_clients_and_machine_auth_still_work(pool: PgPool) {
    let h = Harness::new(pool).await;
    let text_request = json!({"model":"smart-default","messages":[{"role":"system","content":"Help"},{"role":"user","content":"Hi"},{"role":"assistant","content":"Hello"}]});
    let mut expected = text_request.clone();
    expected["model"] = json!("tenant-a-model");
    h.expect(
        expected,
        completion(json!("Welcome"), vec![], "stop"),
        StatusCode::OK,
        "secret-a",
    );
    let (status, body) = h.generate(&text_request, "client-a").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["output_text"], "Welcome");
    assert!(body.get("tool_calls").is_none());
    assert_eq!(
        h.generate(&request(), "unknown").await.0,
        StatusCode::UNAUTHORIZED
    );
    let response = Client::new()
        .post(format!("{}/api/v1/generate", h.gateway.url))
        .json(&request())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    sqlx::query("UPDATE gateway_clients SET is_enabled = FALSE WHERE id = $1")
        .bind(h.clients[0])
        .execute(&h.pool)
        .await
        .unwrap();
    assert_eq!(
        h.generate(&request(), "client-a").await.0,
        StatusCode::FORBIDDEN
    );
    h.assert_consumed();
}

#[sqlx::test]
async fn invalid_upstream_tool_calls_are_errors_without_argument_leaks(pool: PgPool) {
    let h = Harness::new(pool).await;
    let mut malformed = call("call_1", "secret-value");
    malformed["function"]["arguments"] = json!("{secret-value");
    let mut non_object = call("call_1", "x");
    non_object["function"]["arguments"] = json!("[]");
    let mut wrong_schema = call("call_1", "x");
    wrong_schema["function"]["arguments"] = json!("{\"city\":5}");
    let mut unknown = call("call_1", "x");
    unknown["function"]["name"] = json!("unknown");
    let mut invalid_id = call("", "x");
    invalid_id["id"] = json!("");
    let cases = [
        completion(Value::Null, vec![malformed], "tool_calls"),
        completion(Value::Null, vec![non_object], "tool_calls"),
        completion(Value::Null, vec![wrong_schema], "tool_calls"),
        completion(Value::Null, vec![unknown], "tool_calls"),
        completion(Value::Null, vec![invalid_id], "tool_calls"),
        completion(
            Value::Null,
            vec![call("same", "x"), call("same", "y")],
            "tool_calls",
        ),
        completion(Value::Null, vec![], "tool_calls"),
        completion(json!("fake fallback text"), vec![call("c", "x")], "length"),
        completion(Value::Null, vec![], "stop"),
        json!({"choices":[]}),
        json!({"choices":[{"message":{"tool_calls":[{"id":"c","function":{"arguments":{}}}]}}]}),
    ];
    for response in cases {
        h.expect(
            upstream_request(),
            response.clone(),
            StatusCode::OK,
            "secret-a",
        );
        let (status, body) = h.generate(&request(), "client-a").await;
        assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
        assert_eq!(body["error"]["code"], "provider_invalid_response");
        assert!(!body.to_string().contains("secret-value"));
        assert!(body.get("output_text").is_none());
        let log = PostgresRequestsRepository::new(h.pool.clone())
            .get_request(h.tenants[0], body["request_id"].as_str().unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(log.status, "provider_invalid_response");
        assert!(log.latency_ms.is_some());
        assert!(!log.error_message.unwrap().contains("secret-value"));
        if response.get("usage").is_some() {
            assert_eq!(log.input_tokens, Some(17));
        }
    }
    h.assert_consumed();
}

#[sqlx::test]
async fn unsupported_models_are_explicit_and_upstream_errors_are_sanitized(pool: PgPool) {
    let h = Harness::new(pool).await;
    for error in [
        json!({"error":{"message":"This model does not support tools; secret-value", "param":"tools", "code":"unsupported_parameter"}}),
        json!({"error":{"message":"Tool calling is not supported by this model"}}),
        json!({"error":{"message":"Unsupported value for tool_choice", "param":"tool_choice", "code":"unsupported_value"}}),
    ] {
        h.expect(
            upstream_request(),
            error,
            StatusCode::BAD_REQUEST,
            "secret-a",
        );
        let (status, body) = h.generate(&request(), "client-a").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "tool_calling_not_supported");
        let log = PostgresRequestsRepository::new(h.pool.clone())
            .get_request(h.tenants[0], body["request_id"].as_str().unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(log.status, "tool_calling_not_supported");
        assert!(!log.error_message.unwrap().contains("secret-value"));
    }
    h.expect(
        upstream_request(),
        json!({"error":{"message":"Bad schema includes secret-value","param":"tools"}}),
        StatusCode::BAD_REQUEST,
        "secret-a",
    );
    let (status, body) = h.generate(&request(), "client-a").await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(body["error"]["code"], "provider_error");
    assert!(!body.to_string().contains("secret-value"));
    h.assert_consumed();
}

#[sqlx::test]
async fn validates_tools_history_schema_and_http_body_limits(pool: PgPool) {
    let h = Harness::new(pool).await;
    let mut cases = vec![];
    for name in ["", "space name", "инструмент", &"a".repeat(65)] {
        let mut invalid = request();
        invalid["tools"][0]["name"] = json!(name);
        cases.push(invalid);
    }
    for schema in [
        json!({"type":"array"}),
        json!({"type":"object","properties":[]}),
        json!({"type":"object","required":"city"}),
        json!({"type":"object","$ref":"https://example.invalid/schema"}),
        json!({"type":"object","$ref":"file:///etc/passwd"}),
        json!({"type":"object","properties":{"city":{"type":"invalid"}}}),
    ] {
        let mut invalid = request();
        invalid["tools"][0]["parameters"] = schema;
        cases.push(invalid);
    }
    let mut duplicate = request();
    duplicate["tools"] = json!([definition(), definition()]);
    cases.push(duplicate);
    let mut too_many = request();
    too_many["tools"] = json!(
        (0..129)
            .map(|i| {
                let mut t = definition();
                t["name"] = json!(format!("tool_{i}"));
                t
            })
            .collect::<Vec<_>>()
    );
    cases.push(too_many);
    let mut unknown = request();
    unknown["tool_choice"] = json!({"name":"unknown"});
    cases.push(unknown);
    for choice in ["auto", "required"] {
        let mut no_tools = request();
        no_tools["tools"] = json!([]);
        no_tools["tool_choice"] = json!(choice);
        cases.push(no_tools);
    }
    for messages in [
        json!([{"role":"tool","tool_call_id":"orphan","content":"secret-value"}]),
        json!([{"role":"assistant","tool_calls":[{"id":"c","name":"weather","arguments":{}}]}]),
        json!([{"role":"assistant"}]),
        json!([{"role":"user","content":"x","tool_call_id":"secret-value"}]),
        json!([{"role":"assistant","tool_calls":[{"id":"c","name":"weather","arguments":{}}]},{"role":"tool","tool_call_id":"other","content":"secret-value"}]),
    ] {
        let mut invalid = request();
        invalid["messages"] = messages;
        cases.push(invalid);
    }
    for invalid in cases {
        let (status, body) = h.generate(&invalid, "client-a").await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{invalid}: {body}");
        assert_eq!(body["error"]["code"], "invalid_request");
        assert!(!body.to_string().contains("secret-value"));
    }
    let mut too_big = request();
    too_big["messages"][0]["content"] = json!("x".repeat(2 * 1024 * 1024));
    let (status, body) = h.generate(&too_big, "client-a").await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body["error"]["code"], "invalid_request");
    h.assert_consumed();
}

#[sqlx::test]
async fn maps_every_tool_choice_and_preserves_argument_values(pool: PgPool) {
    let h = Harness::new(pool).await;
    for (choice, upstream_choice) in [
        (json!("required"), json!("required")),
        (
            json!({"name":"weather"}),
            json!({"type":"function","function":{"name":"weather"}}),
        ),
        (Value::Null, Value::Null),
    ] {
        let mut input = request();
        let mut expected = upstream_request();
        if choice.is_null() {
            input.as_object_mut().unwrap().remove("tool_choice");
            expected.as_object_mut().unwrap().remove("tool_choice");
        } else {
            input["tool_choice"] = choice;
            expected["tool_choice"] = upstream_choice;
        }
        h.expect(
            expected,
            completion(
                json!("Checking"),
                vec![call("call_1", "Москва")],
                "tool_calls",
            ),
            StatusCode::OK,
            "secret-a",
        );
        let (status, result) = h.generate(&input, "client-a").await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result["output_text"], "Checking");
        assert_eq!(result["tool_calls"][0]["arguments"]["city"], "Москва");
    }
    let mut input = request();
    let mut expected = upstream_request();
    input["tools"][0]["parameters"] = json!({"type":"object"});
    expected["tools"][0]["function"]["parameters"] = input["tools"][0]["parameters"].clone();
    let raw = r#"{"integer":123456789012345678901234567890,"decimal":0.12345678901234567890123456789,"nested":{"s":"Москва","v":[true,null,3]}}"#;
    let mut precise = call("precise", "x");
    precise["function"]["arguments"] = json!(raw);
    let mut response = completion(Value::Null, vec![precise.clone()], "tool_calls");
    response["choices"][0]
        .as_object_mut()
        .unwrap()
        .remove("finish_reason");
    h.expect(expected, response, StatusCode::OK, "secret-a");
    let (status, result) = h.generate(&input, "client-a").await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["finish_reason"], "tool_calls");
    let arguments: Value = serde_json::from_str(raw).unwrap();
    assert_eq!(result["tool_calls"][0]["arguments"], arguments);
    let next = json!({"model":"smart-default","messages":[
        {"role":"assistant","tool_calls":result["tool_calls"]},
        {"role":"tool","tool_call_id":"precise","content":"ok"}
    ],"tool_choice":"none"});
    precise["function"]["arguments"] = json!(arguments.to_string());
    h.expect(
        json!({"model":"tenant-a-model","messages":[
        {"role":"assistant","content":null,"tool_calls":[precise]},
        {"role":"tool","tool_call_id":"precise","content":"ok"}
    ],"tool_choice":"none"}),
        completion(json!("done"), vec![], "stop"),
        StatusCode::OK,
        "secret-a",
    );
    assert_eq!(h.generate(&next, "client-a").await.0, StatusCode::OK);
    h.assert_consumed();
}
