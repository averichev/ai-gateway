//! Requires DATABASE_URL pointing to a disposable PostgreSQL instance with
//! CREATE DATABASE permission. sqlx creates and removes a database per test.
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use ai_gateway::{
    app::{AppState, build_app},
    domain::{SaveModelRouteRequestDto, SaveProviderRequestDto},
    providers::{OpenAiCompatibleProvider, ProviderRegistry},
    repositories::{PostgresAuthRepository, PostgresRequestsRepository, PostgresRoutesRepository},
    security::{SecretCrypto, token_hash},
    usecases::{
        generate::{ContentPreviewPolicy, GenerateService},
        readiness::ReadinessService,
    },
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
    delay_ms: u64,
}
type Exchanges = Arc<Mutex<VecDeque<Exchange>>>;
async fn upstream(
    State((exchanges, hits)): State<(Exchanges, Arc<AtomicUsize>)>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    hits.fetch_add(1, Ordering::SeqCst);
    let exchange = exchanges
        .lock()
        .unwrap()
        .pop_front()
        .expect("unexpected upstream request");
    assert_eq!(headers["authorization"], format!("Bearer {}", exchange.key));
    assert_eq!(body, exchange.request, "actual upstream HTTP payload");
    if exchange.delay_ms > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(exchange.delay_ms)).await;
    }
    (exchange.status, Json(exchange.response))
}
struct Harness {
    gateway: Server,
    _upstream: Server,
    exchanges: Exchanges,
    upstream_hits: Arc<AtomicUsize>,
    pool: PgPool,
    tenants: [Uuid; 2],
    clients: [Uuid; 2],
}
impl Harness {
    async fn new(pool: PgPool) -> Self {
        Self::with_previews(pool, ContentPreviewPolicy::default()).await
    }
    async fn with_previews(pool: PgPool, previews: ContentPreviewPolicy) -> Self {
        let exchanges = Exchanges::default();
        let upstream_hits = Arc::new(AtomicUsize::new(0));
        let upstream = serve(
            Router::new()
                .route("/v1/chat/completions", post(upstream))
                .with_state((exchanges.clone(), upstream_hits.clone())),
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
        let registry =
            ProviderRegistry::new(vec![Arc::new(OpenAiCompatibleProvider::new().unwrap())]);
        let readiness = ReadinessService::new(routes.clone(), registry.clone(), crypto.clone());
        let generate = GenerateService::new(
            routes.clone(),
            requests.clone(),
            registry,
            crypto.clone(),
            previews,
        );
        let gateway = serve(build_app(AppState {
            generate_service: Arc::new(generate),
            readiness_service: Arc::new(readiness),
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
            upstream_hits,
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
            delay_ms: 0,
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
    let h = Harness::with_previews(
        pool,
        ContentPreviewPolicy {
            enabled: true,
            ..ContentPreviewPolicy::default()
        },
    )
    .await;
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

// G02 privacy and configuration readiness regressions. All upstreams are mocks.
#[sqlx::test]
async fn previews_are_opt_in_and_technical_metadata_is_retained(pool: PgPool) {
    let h = Harness::new(pool).await;
    let request = json!({"model":"smart-default","messages":[
        {"role":"system","content":"private-system"},{"role":"user","content":"private-document"}]});
    let mut upstream = request.clone();
    upstream["model"] = json!("tenant-a-model");
    h.expect(
        upstream,
        completion(json!("private-answer"), vec![], "stop"),
        StatusCode::OK,
        "secret-a",
    );
    let (status, body) = h.generate(&request, "client-a").await;
    assert_eq!(status, StatusCode::OK);
    let repo = PostgresRequestsRepository::new(h.pool.clone());
    let record = repo
        .get_request(h.tenants[0], body["id"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(record.prompt_preview.is_none());
    assert!(record.response_preview.is_none());
    assert_eq!(record.tenant_id, h.tenants[0]);
    assert_eq!(record.gateway_client_id, Some(h.clients[0]));
    assert_eq!(record.model_alias, "smart-default");
    assert_eq!(record.provider_code.as_deref(), Some("mock"));
    assert_eq!(record.external_model.as_deref(), Some("tenant-a-model"));
    assert_eq!(record.status, "success");
    assert!(record.latency_ms.unwrap() >= 0);
    assert_eq!(record.input_tokens, Some(17));
    assert_eq!(record.output_tokens, Some(9));
    assert!(record.created_at <= chrono::Utc::now());
    h.assert_consumed();
}

#[sqlx::test]
async fn enabled_previews_are_limited_but_all_tool_conversations_stay_private(pool: PgPool) {
    let h = Harness::with_previews(
        pool,
        ContentPreviewPolicy {
            enabled: true,
            request_chars: 10,
            response_chars: 5,
        },
    )
    .await;
    let text =
        json!({"model":"smart-default","messages":[{"role":"user","content":"Конфиденциально"}]});
    let mut expected = text.clone();
    expected["model"] = json!("tenant-a-model");
    h.expect(
        expected,
        completion(json!("Здравствуйте"), vec![], "stop"),
        StatusCode::OK,
        "secret-a",
    );
    let (status, response) = h.generate(&text, "client-a").await;
    assert_eq!(status, StatusCode::OK);
    let repo = PostgresRequestsRepository::new(h.pool.clone());
    let record = repo
        .get_request(h.tenants[0], response["id"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.prompt_preview.as_deref(), Some("user: Конф..."));
    assert_eq!(record.response_preview.as_deref(), Some("Здрав..."));
    let tool_cases = [
        request(),
        json!({"model":"smart-default","messages":[{"role":"user","content":"private"}],"tool_choice":"none"}),
        json!({"model":"smart-default","messages":[
            {"role":"user","content":"private"},
            {"role":"assistant","tool_calls":[{"id":"c1","name":"weather","arguments":{"city":"private"}}]},
            {"role":"tool","tool_call_id":"c1","content":"private-result"}]}),
    ];
    for request in tool_cases {
        // Build the exact vendor request to preserve the existing mapping assertions.
        let mut expected = request.clone();
        expected["model"] = json!("tenant-a-model");
        if let Some(tools) = expected.get_mut("tools").and_then(Value::as_array_mut) {
            for tool in tools {
                *tool = json!({"type":"function", "function":tool.clone()});
            }
        }
        for message in expected["messages"].as_array_mut().unwrap() {
            if message["role"] == "assistant" {
                message["content"] = Value::Null;
                message["tool_calls"] = json!([call("c1", "private")]);
            }
        }
        h.expect(
            expected,
            completion(json!("private-answer"), vec![], "stop"),
            StatusCode::OK,
            "secret-a",
        );
        let (status, body) = h.generate(&request, "client-a").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let record = repo
            .get_request(h.tenants[0], body["id"].as_str().unwrap())
            .await
            .unwrap()
            .unwrap();
        assert!(record.prompt_preview.is_none());
        assert!(record.response_preview.is_none());
    }
    h.assert_consumed();
}

#[sqlx::test]
async fn clearing_previews_preserves_history_and_supports_tenant_scope(pool: PgPool) {
    let h = Harness::new(pool).await;
    sqlx::query("INSERT INTO requests (id, tenant_id, gateway_client_id, model_alias, provider_code, external_model, status, latency_ms, input_tokens, output_tokens, prompt_preview, response_preview) VALUES ('old-a', $1, $2, 'smart-default', 'mock', 'tenant-a-model', 'success', 42, 17, 9, 'private-prompt', 'private-response'), ('old-b', $3, $4, 'smart-default', 'mock', 'tenant-b-model', 'provider_error', 52, 10, 1, 'private-prompt', NULL)")
        .bind(h.tenants[0]).bind(h.clients[0]).bind(h.tenants[1]).bind(h.clients[1]).execute(&h.pool).await.unwrap();
    let repo = PostgresRequestsRepository::new(h.pool.clone());
    let before = repo
        .get_request(h.tenants[0], "old-a")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(repo.count_content_previews(None).await.unwrap(), 2);
    assert!(
        repo.get_request(h.tenants[0], "old-a")
            .await
            .unwrap()
            .unwrap()
            .prompt_preview
            .is_some()
    );
    assert_eq!(
        repo.clear_content_previews(Some(h.tenants[0]))
            .await
            .unwrap(),
        1
    );
    let after = repo
        .get_request(h.tenants[0], "old-a")
        .await
        .unwrap()
        .unwrap();
    let mut expected = serde_json::to_value(before).unwrap();
    expected["prompt_preview"] = Value::Null;
    expected["response_preview"] = Value::Null;
    assert_eq!(serde_json::to_value(after).unwrap(), expected);
    assert_eq!(
        repo.count_content_previews(Some(h.tenants[1]))
            .await
            .unwrap(),
        1
    );
    assert_eq!(repo.clear_content_previews(None).await.unwrap(), 1);
    assert_eq!(repo.clear_content_previews(None).await.unwrap(), 0);
    assert_eq!(h.history(h.tenants[0]).await.as_array().unwrap().len(), 1);
    assert_eq!(h.history(h.tenants[1]).await.as_array().unwrap().len(), 1);
    h.assert_consumed();
}

async fn readiness(h: &Harness, tenant: Uuid, alias: &str, token: &str) -> (StatusCode, Value) {
    let response = Client::new()
        .get(format!(
            "{}/api/admin/tenants/{tenant}/model-routes/{alias}/readiness",
            h.gateway.url
        ))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    (response.status(), response.json().await.unwrap())
}
fn check_status<'a>(body: &'a Value, code: &str) -> &'a str {
    body["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["code"] == code)
        .unwrap()["status"]
        .as_str()
        .unwrap()
}

#[sqlx::test]
async fn readiness_checks_configuration_without_upstream_or_history_writes(pool: PgPool) {
    let h = Harness::new(pool).await;
    let (status, ready) = readiness(&h, h.tenants[0], "smart-default", "admin-session").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ready["status"], "ready");
    assert_eq!(ready["provider_code"], "mock");
    assert!(
        ready["checks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|check| check["status"] == "ok")
    );
    for forbidden in ["secret-a", "ciphertext", "nonce", "base_url", "api_key"] {
        assert!(!ready.to_string().contains(forbidden));
    }
    let (status, missing) = readiness(&h, h.tenants[0], "absent", "admin-session").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing["status"], "incomplete");
    assert_eq!(check_status(&missing, "model_route"), "error");
    sqlx::query("UPDATE providers SET is_enabled = FALSE WHERE tenant_id = $1")
        .bind(h.tenants[0])
        .execute(&h.pool)
        .await
        .unwrap();
    let (_, disabled) = readiness(&h, h.tenants[0], "smart-default", "admin-session").await;
    assert_eq!(disabled["status"], "unavailable");
    assert_eq!(check_status(&disabled, "provider_enabled"), "error");
    sqlx::query("UPDATE providers SET is_enabled = TRUE WHERE tenant_id = $1")
        .bind(h.tenants[0])
        .execute(&h.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM provider_secrets WHERE tenant_id = $1")
        .bind(h.tenants[0])
        .execute(&h.pool)
        .await
        .unwrap();
    let (_, missing_key) = readiness(&h, h.tenants[0], "smart-default", "admin-session").await;
    assert_eq!(missing_key["status"], "incomplete");
    assert_eq!(check_status(&missing_key, "provider_credentials"), "error");
    assert_eq!(h.upstream_hits.load(Ordering::SeqCst), 0);
    assert!(h.history(h.tenants[0]).await.as_array().unwrap().is_empty());
    h.assert_consumed();
}

#[sqlx::test]
async fn readiness_rejects_invalid_parameters_adapter_timeout_and_unreadable_keys(pool: PgPool) {
    let h = Harness::new(pool).await;
    for (sql, code, overall) in [
        (
            "UPDATE model_routes SET is_enabled = FALSE WHERE tenant_id = $1",
            "model_route_enabled",
            "unavailable",
        ),
        (
            "UPDATE model_routes SET is_enabled = TRUE, external_model = '' WHERE tenant_id = $1",
            "external_model",
            "incomplete",
        ),
        (
            "UPDATE providers SET base_url = 'https://user:private-key@example.invalid/v1?token=private-query' WHERE tenant_id = $1",
            "provider_parameters",
            "incomplete",
        ),
        (
            "UPDATE providers SET timeout_ms = 0 WHERE tenant_id = $1",
            "provider_timeout",
            "incomplete",
        ),
        (
            "UPDATE providers SET kind = 'unregistered' WHERE tenant_id = $1",
            "provider_adapter",
            "unavailable",
        ),
        (
            "UPDATE provider_secrets SET nonce = decode('00', 'hex') WHERE tenant_id = $1",
            "provider_credentials",
            "unavailable",
        ),
    ] {
        sqlx::query(sql)
            .bind(h.tenants[0])
            .execute(&h.pool)
            .await
            .unwrap();
        let (status, body) = readiness(&h, h.tenants[0], "smart-default", "admin-session").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(check_status(&body, code), "error");
        assert_eq!(body["status"], overall);
        assert!(!body.to_string().contains("private-key"));
        assert!(!body.to_string().contains("private-query"));
    }
    assert_eq!(h.upstream_hits.load(Ordering::SeqCst), 0);
}

#[sqlx::test]
async fn readiness_authorizes_before_revealing_route_or_tenant(pool: PgPool) {
    let h = Harness::new(pool).await;
    let auth = PostgresAuthRepository::new(h.pool.clone());
    let user = auth
        .create_user("viewer@test", "unused", Some(h.tenants[0]), Some("viewer"))
        .await
        .unwrap();
    auth.create_session(
        user.id,
        &token_hash("viewer-session"),
        chrono::Utc::now() + chrono::Duration::hours(1),
    )
    .await
    .unwrap();
    assert_eq!(
        readiness(&h, h.tenants[0], "smart-default", "viewer-session")
            .await
            .0,
        StatusCode::OK
    );
    for tenant in [h.tenants[1], Uuid::new_v4()] {
        for alias in ["smart-default", "absent"] {
            let (status, body) = readiness(&h, tenant, alias, "viewer-session").await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert!(body.get("checks").is_none());
            assert!(body.get("provider_code").is_none());
        }
    }
    for token in ["", "unknown", "client-a"] {
        assert_eq!(
            readiness(&h, h.tenants[0], "smart-default", token).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    let response = Client::new()
        .get(format!("{}/health", h.gateway.url))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"status":"ok"})
    );
    assert_eq!(h.upstream_hits.load(Ordering::SeqCst), 0);
}

#[derive(Clone)]
struct LogWriter(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for LogWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn captured_logs() -> Arc<Mutex<Vec<u8>>> {
    use tracing_subscriber::layer::SubscriberExt;
    static LOGS: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();
    LOGS.get_or_init(|| {
        let bytes = Arc::new(Mutex::new(vec![]));
        let writer = bytes.clone();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_subscriber::filter::filter_fn(
                ai_gateway::server::safe_diagnostic_target,
            ))
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_span_events(tracing_subscriber::fmt::format::FmtSpan::NEW)
                    .with_writer(move || LogWriter(writer.clone())),
            );
        tracing::subscriber::set_global_default(subscriber).unwrap();
        bytes
    })
    .clone()
}

#[sqlx::test]
async fn errors_and_diagnostic_logs_never_echo_content_credentials_or_database_details(
    pool: PgPool,
) {
    let logs = captured_logs();
    let h = Harness::new(pool).await;
    let private = "g02-private-document-and-arguments";
    let text = json!({"model":"smart-default","messages":[{"role":"system","content":private},{"role":"user","content":private}]});
    let mut expected = text.clone();
    expected["model"] = json!("tenant-a-model");
    for (response, upstream_status, category) in [
        (
            json!({"error":{"message":format!("{private}; Bearer g02-private-bearer; secret-a; https://host.invalid?token=g02-url-secret")}}),
            StatusCode::INTERNAL_SERVER_ERROR,
            "provider_error",
        ),
        (
            json!({"choices":[{"message":{"content":{"private":private}},"finish_reason":"stop"}]}),
            StatusCode::OK,
            "provider_invalid_response",
        ),
    ] {
        h.expect(expected.clone(), response, upstream_status, "secret-a");
        let (status, body) = h.generate(&text, "client-a").await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(body["error"]["code"], category);
        let record = PostgresRequestsRepository::new(h.pool.clone())
            .get_request(h.tenants[0], body["request_id"].as_str().unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record.status, category);
        assert!(record.prompt_preview.is_none());
        assert!(!serde_json::to_string(&record).unwrap().contains(private));
        assert!(!body.to_string().contains(private));
    }
    sqlx::query("UPDATE providers SET timeout_ms = 10 WHERE tenant_id = $1")
        .bind(h.tenants[0])
        .execute(&h.pool)
        .await
        .unwrap();
    h.expect(
        expected.clone(),
        completion(json!(private), vec![], "stop"),
        StatusCode::OK,
        "secret-a",
    );
    h.exchanges.lock().unwrap().back_mut().unwrap().delay_ms = 100;
    let (status, body) = h.generate(&text, "client-a").await;
    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(body["error"]["code"], "provider_timeout");
    h.assert_consumed();
    sqlx::query("UPDATE providers SET timeout_ms = 1000 WHERE tenant_id = $1")
        .bind(h.tenants[0])
        .execute(&h.pool)
        .await
        .unwrap();

    // DB errors may include failing rows even with previews enabled. Never format them.
    sqlx::raw_sql("CREATE FUNCTION fail_history() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'g02-db-error-private-document secret-a'; END $$; CREATE TRIGGER fail_history BEFORE INSERT ON requests FOR EACH ROW EXECUTE FUNCTION fail_history();")
        .execute(&h.pool).await.unwrap();
    h.expect(
        expected,
        completion(json!(private), vec![], "stop"),
        StatusCode::OK,
        "secret-a",
    );
    assert_eq!(h.generate(&text, "client-a").await.0, StatusCode::OK);
    sqlx::raw_sql("DROP TRIGGER fail_history ON requests; DROP FUNCTION fail_history();")
        .execute(&h.pool)
        .await
        .unwrap();
    h._upstream.task.abort();
    let (status, body) = h.generate(&text, "client-a").await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(body["error"]["code"], "provider_transport_error");

    for path in [
        "/api/v1/generate".to_owned(),
        format!("/api/admin/tenants/{}/providers", h.tenants[0]),
        "/api/auth/login".to_owned(),
    ] {
        let response = Client::new().post(format!("{}{path}?token=g02-url-secret", h.gateway.url))
            .bearer_auth("g02-private-bearer").header("content-type","application/json")
            .body(r#"{"api_key":{"private":"g02-json-secret"},"model":"g02-json-secret","messages":[{"role":"g02-json-secret"}],"email":"x@y","password":{"secret":"g02-json-secret"}}"#)
            .send().await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response.text().await.unwrap();
        assert!(!body.contains("g02-json-secret"));
    }
    let output = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
    for secret in [
        private,
        "secret-a",
        "client-a",
        "g02-private-bearer",
        "g02-url-secret",
        "g02-json-secret",
        "g02-db-error-private-document",
    ] {
        assert!(
            !output.contains(secret),
            "diagnostic output leaked a private marker"
        );
    }
    for expected in [
        "provider_error",
        "provider_invalid_response",
        "provider_timeout",
        "provider_transport_error",
        "failed to persist request log",
        "upstream_status=Some(500)",
        "req_",
        "smart-default",
        "http_status=502",
    ] {
        assert!(
            output.contains(expected),
            "missing useful diagnostic: {expected}"
        );
    }
    assert_eq!(h.upstream_hits.load(Ordering::SeqCst), 4);
}

#[sqlx::test]
async fn python_smoke_runs_text_and_tool_round_trip_through_gateway_with_mock_upstream(
    pool: PgPool,
) {
    let h = Harness::new(pool).await;
    h.expect(json!({"model":"tenant-a-model","messages":[{"role":"user","content":"Reply with a short greeting."}]}),
        completion(json!("Hello, g02-hidden-answer"),vec![],"stop"),StatusCode::OK,"secret-a");
    let messages = json!([{"role":"user","content":"Use demo_add_numbers with a=12500 and b=7500. After the tool result, reply with only the sum."}]);
    let tool = json!({"name":"demo_add_numbers","description":"Add exactly two integers for the smoke test.","parameters":{
        "type":"object","properties":{"a":{"type":"integer"},"b":{"type":"integer"}},"required":["a","b"],"additionalProperties":false}});
    let vendor_call = json!({"id":"call_demo","type":"function","function":{"name":"demo_add_numbers","arguments":"{\"a\":12500,\"b\":7500}"}});
    h.expect(json!({"model":"tenant-a-model","messages":messages,"tools":[{"type":"function","function":tool}],"tool_choice":{"type":"function","function":{"name":"demo_add_numbers"}}}),
        completion(Value::Null,vec![vendor_call.clone()],"tool_calls"),StatusCode::OK,"secret-a");
    let mut final_messages = messages.as_array().unwrap().clone();
    final_messages.push(json!({"role":"assistant","content":null,"tool_calls":[vendor_call]}));
    final_messages
        .push(json!({"role":"tool","tool_call_id":"call_demo","content":"{\"sum\": 20000}"}));
    h.expect(
        json!({"model":"tenant-a-model","messages":final_messages,"tool_choice":"none"}),
        completion(json!("20000"), vec![], "stop"),
        StatusCode::OK,
        "secret-a",
    );
    let output = tokio::process::Command::new("python3")
        .arg("scripts/smoke_llm.py")
        .args([
            "--url",
            &h.gateway.url,
            "--model",
            "smart-default",
            "--timeout",
            "5",
        ])
        .env("GATEWAY_MACHINE_TOKEN", "client-a")
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let reports: Vec<Value> = String::from_utf8(output.stdout.clone())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(reports.len(), 3);
    for report in reports {
        assert_eq!(report["success"], true);
        assert_eq!(report["http_status"], 200);
        assert!(report["request_id"].as_str().unwrap().starts_with("req_"));
        assert_eq!(report["usage"]["input_tokens"], 17);
        assert_eq!(report["model_alias"], "smart-default");
    }
    let stdout = String::from_utf8(output.stdout).unwrap();
    for forbidden in [
        "client-a",
        "secret-a",
        "hidden-answer",
        "12500",
        "7500",
        "20000",
        "demo_add_numbers",
        "call_demo",
    ] {
        assert!(!stdout.contains(forbidden));
    }
    assert_eq!(h.upstream_hits.load(Ordering::SeqCst), 3);
    h.assert_consumed();
}

#[sqlx::test]
async fn existing_admin_smoke_test_keeps_text_contract_and_private_history(pool: PgPool) {
    let h = Harness::new(pool).await;
    let request = json!({"model":"smart-default","messages":[{"role":"user","content":"Admin smoke"}],"options":{"temperature":0.2,"max_tokens":32}});
    let mut expected = request.clone();
    expected["model"] = json!("tenant-a-model");
    expected["temperature"] = json!(0.2);
    expected["max_tokens"] = json!(32);
    expected.as_object_mut().unwrap().remove("options");
    h.expect(
        expected,
        completion(json!("Admin test OK"), vec![], "stop"),
        StatusCode::OK,
        "secret-a",
    );
    let url = format!(
        "{}/api/admin/tenants/{}/generate",
        h.gateway.url, h.tenants[0]
    );
    let response = Client::new()
        .post(&url)
        .bearer_auth("admin-session")
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["output_text"], "Admin test OK");
    let record = PostgresRequestsRepository::new(h.pool.clone())
        .get_request(h.tenants[0], body["id"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.gateway_client_id, None);
    assert_eq!(record.status, "success");
    assert!(record.prompt_preview.is_none());
    assert!(record.response_preview.is_none());
    let response = Client::new()
        .post(&url)
        .bearer_auth("client-a")
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    h.assert_consumed();
}

#[sqlx::test]
async fn preview_cleanup_cli_only_changes_previews_after_explicit_apply(pool: PgPool) {
    let h = Harness::new(pool).await;
    sqlx::query("INSERT INTO requests (id, tenant_id, model_alias, status, latency_ms, input_tokens, prompt_preview, response_preview) VALUES ('legacy', $1, 'smart-default', 'success', 12, 34, 'g02-cli-private-prompt', 'g02-cli-private-answer')")
        .bind(h.tenants[0]).execute(&h.pool).await.unwrap();
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&h.pool)
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&std::env::var("DATABASE_URL").unwrap()).unwrap();
    url.set_path(&database);
    let repo = PostgresRequestsRepository::new(h.pool.clone());
    for apply in [false, true] {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_purge_previews"));
        command
            .env("DATABASE_URL", url.as_str())
            .args(["--tenant-id", &h.tenants[0].to_string()]);
        if apply {
            command.arg("--apply");
        }
        let output = command.output().await.unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("records_with_previews: 1"));
        assert!(!stdout.contains("g02-cli-private"));
        let record = repo
            .get_request(h.tenants[0], "legacy")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record.prompt_preview.is_none(), apply);
        assert_eq!(record.response_preview.is_none(), apply);
        assert_eq!(record.status, "success");
        assert_eq!(record.latency_ms, Some(12));
        assert_eq!(record.input_tokens, Some(34));
        if apply {
            assert!(stdout.contains("records_cleared: 1"));
        } else {
            assert!(stdout.contains("dry_run: true"));
        }
    }
}

#[sqlx::test]
async fn python_smoke_explicitly_fails_for_unsupported_or_ignored_tool_calling(pool: PgPool) {
    let h = Harness::new(pool).await;
    let messages = json!([{"role":"user","content":"Use demo_add_numbers with a=12500 and b=7500. After the tool result, reply with only the sum."}]);
    let tool = json!({"name":"demo_add_numbers","description":"Add exactly two integers for the smoke test.","parameters":{
        "type":"object","properties":{"a":{"type":"integer"},"b":{"type":"integer"}},"required":["a","b"],"additionalProperties":false}});
    for (reply, status, expected_category) in [
        (
            json!({"error":{"message":"tools are not supported; g02-private-error","code":"unsupported_parameter","param":"tools"}}),
            StatusCode::BAD_REQUEST,
            "tool_calling_not_supported",
        ),
        (
            completion(json!("g02-private-answer-without-tool"), vec![], "stop"),
            StatusCode::OK,
            "provider_invalid_response",
        ),
    ] {
        h.expect(json!({"model":"tenant-a-model","messages":[{"role":"user","content":"Reply with a short greeting."}]}),
            completion(json!("Hello"),vec![],"stop"),StatusCode::OK,"secret-a");
        h.expect(json!({"model":"tenant-a-model","messages":messages,"tools":[{"type":"function","function":tool}],"tool_choice":{"type":"function","function":{"name":"demo_add_numbers"}}}),
            reply,status,"secret-a");
        let output = tokio::process::Command::new("python3")
            .arg("scripts/smoke_llm.py")
            .args(["--url", &h.gateway.url, "--timeout", "5"])
            .env("GATEWAY_MACHINE_TOKEN", "client-a")
            .env_remove("GATEWAY_MODEL_ALIAS")
            .output()
            .await
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        let stdout = String::from_utf8(output.stdout).unwrap();
        let reports: Vec<Value> = stdout
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0]["success"], true);
        assert_eq!(reports[1]["success"], false);
        assert_eq!(reports[1]["error_category"], expected_category);
        assert!(!stdout.contains("g02-private"));
        h.assert_consumed();
    }
    assert_eq!(h.upstream_hits.load(Ordering::SeqCst), 4);
}

#[sqlx::test]
async fn configuration_storage_failures_still_record_safe_technical_history(pool: PgPool) {
    let h = Harness::new(pool).await;
    let request = json!({"model":"smart-default","messages":[{"role":"user","content":"g02-private-storage-document"}]});
    for (table, has_route) in [("model_routes", false), ("provider_secrets", true)] {
        sqlx::raw_sql(&format!("ALTER TABLE {table} RENAME TO hidden_config"))
            .execute(&h.pool)
            .await
            .unwrap();
        let (status, body) = h.generate(&request, "client-a").await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["error"]["code"], "storage_error");
        let record = PostgresRequestsRepository::new(h.pool.clone())
            .get_request(h.tenants[0], body["request_id"].as_str().unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record.status, "storage_error");
        assert_eq!(record.gateway_client_id, Some(h.clients[0]));
        assert_eq!(record.provider_code.is_some(), has_route);
        assert!(record.latency_ms.is_some());
        assert!(record.prompt_preview.is_none());
        assert!(
            !serde_json::to_string(&record)
                .unwrap()
                .contains("g02-private-storage-document")
        );
        sqlx::raw_sql(&format!("ALTER TABLE hidden_config RENAME TO {table}"))
            .execute(&h.pool)
            .await
            .unwrap();
    }
    assert_eq!(h.upstream_hits.load(Ordering::SeqCst), 0);
}
