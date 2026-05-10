use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    domain::{GenerateRequestDto, ProviderGenerateResult, ResolvedRouteRecord, TokenUsageDto},
    providers::{ProviderAdapter, ProviderError},
};

pub struct OpenAiCompatibleProvider {
    client: Client,
}

impl OpenAiCompatibleProvider {
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = Client::builder().user_agent("ai-gateway-mvp/0.1").build()?;

        Ok(Self { client })
    }
}

#[async_trait]
impl ProviderAdapter for OpenAiCompatibleProvider {
    fn kind(&self) -> &'static str {
        "openai-compatible"
    }

    async fn generate(
        &self,
        route: &ResolvedRouteRecord,
        request: &GenerateRequestDto,
        api_key: &str,
    ) -> Result<ProviderGenerateResult, ProviderError> {
        let payload = OpenAiRequestPayload {
            model: route.external_model.clone(),
            messages: request
                .messages
                .iter()
                .map(|message| OpenAiMessagePayload {
                    role: message.role.as_str().to_owned(),
                    content: message.content.clone(),
                })
                .collect(),
            temperature: request.options.temperature,
            max_tokens: request.options.max_tokens,
        };

        let url = format!(
            "{}/chat/completions",
            route.provider_base_url.trim_end_matches('/')
        );

        let response = self
            .client
            .post(url)
            .bearer_auth(api_key)
            .timeout(Duration::from_millis(route.provider_timeout_ms as u64))
            .json(&payload)
            .send()
            .await
            .map_err(map_transport_error)?;

        let status = response.status();
        let body = response.text().await.map_err(map_transport_error)?;

        if !status.is_success() {
            let message = extract_error_message(&body)
                .unwrap_or_else(|| format!("HTTP {} from upstream", status.as_u16()));

            return Err(ProviderError::Upstream {
                status: Some(status.as_u16()),
                message,
            });
        }

        let parsed: OpenAiResponsePayload = serde_json::from_str(&body)
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;

        let choice = parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::InvalidResponse("missing choices[0]".to_owned()))?;

        let output_text = extract_content_text(choice.message.content)
            .ok_or_else(|| ProviderError::InvalidResponse("missing message.content".to_owned()))?;

        Ok(ProviderGenerateResult {
            output_text,
            finish_reason: choice.finish_reason.unwrap_or_else(|| "stop".to_owned()),
            usage: parsed.usage.map(|usage| TokenUsageDto {
                input_tokens: usage.prompt_tokens,
                output_tokens: usage.completion_tokens,
            }),
        })
    }
}

fn map_transport_error(error: reqwest::Error) -> ProviderError {
    if error.is_timeout() {
        ProviderError::Timeout(error.to_string())
    } else {
        ProviderError::Transport(error.to_string())
    }
}

fn extract_error_message(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;

    value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| {
            value
                .get("message")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
}

fn extract_content_text(content: Option<Value>) -> Option<String> {
    let content = content?;

    match content {
        Value::String(value) => Some(value),
        Value::Array(items) => {
            let joined = items
                .into_iter()
                .filter_map(|item| {
                    item.get("text")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                })
                .collect::<Vec<_>>()
                .join("");

            if joined.is_empty() {
                None
            } else {
                Some(joined)
            }
        }
        _ => None,
    }
}

#[derive(Debug, Serialize)]
struct OpenAiRequestPayload {
    model: String,
    messages: Vec<OpenAiMessagePayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<i32>,
}

#[derive(Debug, Serialize)]
struct OpenAiMessagePayload {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponsePayload {
    choices: Vec<OpenAiChoicePayload>,
    #[serde(default)]
    usage: Option<OpenAiUsagePayload>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoicePayload {
    message: OpenAiChoiceMessagePayload,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoiceMessagePayload {
    #[serde(default)]
    content: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsagePayload {
    #[serde(default)]
    prompt_tokens: Option<i32>,
    #[serde(default)]
    completion_tokens: Option<i32>,
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use axum::{
        Json, Router,
        extract::State,
        http::{HeaderMap, StatusCode, header},
        routing::post,
    };
    use serde_json::{Value, json};
    use tokio::net::TcpListener;

    use super::*;
    use crate::domain::{GenerateMessageDto, GenerateOptionsDto, MessageRole};

    #[derive(Clone)]
    struct MockApiState {
        request_count: Arc<AtomicUsize>,
    }

    #[tokio::test]
    async fn generate_sends_request_to_api_and_returns_response() {
        let request_count = Arc::new(AtomicUsize::new(0));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/chat/completions", post(handle_chat_completions))
            .with_state(MockApiState {
                request_count: request_count.clone(),
            });
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let provider = OpenAiCompatibleProvider::new().unwrap();
        let route = ResolvedRouteRecord {
            external_model: "external-test-model".to_owned(),
            provider_code: "mock-provider".to_owned(),
            provider_kind: "openai-compatible".to_owned(),
            provider_base_url: format!("http://{address}"),
            provider_api_key_env: "TEST_API_KEY".to_owned(),
            provider_timeout_ms: 1_000,
        };
        let request = GenerateRequestDto {
            model: "test-model".to_owned(),
            messages: vec![GenerateMessageDto {
                role: MessageRole::User,
                content: "ping".to_owned(),
            }],
            options: GenerateOptionsDto {
                temperature: Some(0.2),
                max_tokens: Some(32),
            },
        };

        let response = provider
            .generate(&route, &request, "test-token")
            .await
            .unwrap();

        server.abort();
        tokio::time::timeout(Duration::from_secs(1), server)
            .await
            .expect("mock API server did not stop")
            .expect_err("mock API server stopped without abort");

        assert_eq!(request_count.load(Ordering::SeqCst), 1);
        assert_eq!(response.output_text, "pong");
        assert_eq!(response.finish_reason, "stop");
        assert_eq!(
            response.usage.as_ref().and_then(|usage| usage.input_tokens),
            Some(7)
        );
        assert_eq!(
            response
                .usage
                .as_ref()
                .and_then(|usage| usage.output_tokens),
            Some(3)
        );
    }

    async fn handle_chat_completions(
        State(state): State<MockApiState>,
        headers: HeaderMap,
        Json(payload): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        state.request_count.fetch_add(1, Ordering::SeqCst);

        assert_eq!(
            headers
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer test-token")
        );
        assert_eq!(payload["model"], json!("external-test-model"));
        assert_eq!(payload["messages"][0]["role"], json!("user"));
        assert_eq!(payload["messages"][0]["content"], json!("ping"));
        assert_eq!(payload["max_tokens"], json!(32));
        assert_eq!(payload["temperature"].as_f64(), Some(0.2));

        (
            StatusCode::OK,
            Json(json!({
                "choices": [
                    {
                        "message": {
                            "content": "pong"
                        },
                        "finish_reason": "stop"
                    }
                ],
                "usage": {
                    "prompt_tokens": 7,
                    "completion_tokens": 3
                }
            })),
        )
    }
}
