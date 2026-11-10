use std::{collections::HashSet, time::Duration};

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    domain::{
        FinishReason, GenerateMessageDto, GenerateRequestDto, ProviderGenerateResult,
        ResolvedRouteRecord, TokenUsageDto, ToolCallDto, ToolChoiceDto, ToolChoicePolicy,
        ToolDefinitionDto,
        validation::{MAX_TOOLS, valid_tool_call},
    },
    providers::{ProviderAdapter, ProviderError},
};

const MAX_UPSTREAM_BODY_BYTES: usize = 4 * 1024 * 1024;

pub struct OpenAiCompatibleProvider {
    client: Client,
}

impl OpenAiCompatibleProvider {
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = Client::builder().user_agent("ai-gateway-mvp/0.1").build()?;
        Ok(Self { client })
    }

    async fn send_chat_completion(
        &self,
        route: &ResolvedRouteRecord,
        api_key: &str,
        payload: &OpenAiRequestPayload<'_>,
    ) -> Result<(StatusCode, String), ProviderError> {
        let url = format!(
            "{}/chat/completions",
            route.provider_base_url.trim_end_matches('/')
        );
        let mut response = self
            .client
            .post(url)
            .bearer_auth(api_key)
            .timeout(Duration::from_millis(route.provider_timeout_ms as u64))
            .json(payload)
            .send()
            .await
            .map_err(map_transport_error)?;
        let status = response.status();
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(map_transport_error)? {
            if body.len() + chunk.len() > MAX_UPSTREAM_BODY_BYTES {
                return Err(invalid_response("response exceeds 4 MiB", None));
            }
            body.extend_from_slice(&chunk);
        }
        let body =
            String::from_utf8(body).map_err(|_| invalid_response("response is not UTF-8", None))?;
        Ok((status, body))
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
        let payload = OpenAiRequestPayload::from_generate_request(
            route,
            request,
            TokenLimitParameter::MaxTokens,
        );
        let (mut status, mut body) = self.send_chat_completion(route, api_key, &payload).await?;
        if should_retry_with_max_completion_tokens(status, &body, request.options.max_tokens) {
            let payload = OpenAiRequestPayload::from_generate_request(
                route,
                request,
                TokenLimitParameter::MaxCompletionTokens,
            );
            (status, body) = self.send_chat_completion(route, api_key, &payload).await?;
        }
        if !status.is_success() {
            if request.uses_tools() && is_tool_calling_unsupported(status, &body) {
                return Err(ProviderError::ToolCallingNotSupported);
            }
            // Upstreams may echo arguments or credentials in their errors.
            return Err(ProviderError::Upstream {
                status: Some(status.as_u16()),
                message: format!("HTTP {} from upstream", status.as_u16()),
            });
        }
        let parsed: OpenAiResponsePayload = serde_json::from_str(&body).map_err(|_| {
            // Keep reported usage even when a malformed choice cannot decode.
            let usage = serde_json::from_str::<OpenAiUsageEnvelope>(&body)
                .ok()
                .and_then(|envelope| envelope.usage)
                .map(TokenUsageDto::from);
            invalid_response("malformed completion payload", usage)
        })?;
        normalize_response(parsed, request)
    }
}

fn invalid_response(message: &str, usage: Option<TokenUsageDto>) -> ProviderError {
    ProviderError::InvalidResponse {
        message: message.to_owned(),
        usage,
    }
}

fn normalize_response(
    parsed: OpenAiResponsePayload,
    request: &GenerateRequestDto,
) -> Result<ProviderGenerateResult, ProviderError> {
    let usage = parsed.usage.map(TokenUsageDto::from);
    let invalid = |message: &str| invalid_response(message, usage.clone());
    let choice = parsed
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| invalid("missing choices[0]"))?;
    let content = choice.message.content;
    let output_text = match content {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text),
        Some(Value::Array(parts)) => {
            let text = parts
                .iter()
                .map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| invalid("invalid text content parts"))?
                .join("");
            Some(text)
        }
        _ => return Err(invalid("invalid message.content")),
    };
    let upstream_calls = choice.message.tool_calls.unwrap_or_default();
    if upstream_calls.len() > MAX_TOOLS {
        return Err(invalid("too many tool calls"));
    }
    let mut ids = HashSet::new();
    let mut tool_calls = Vec::with_capacity(upstream_calls.len());
    for call in upstream_calls {
        if call.kind != "function" || call.function.arguments.len() > 64 * 1024 {
            return Err(invalid("invalid tool call type or arguments size"));
        }
        let arguments = serde_json::from_str(&call.function.arguments)
            .map_err(|_| invalid("tool arguments must be a JSON object"))?;
        let call = ToolCallDto {
            id: call.id,
            name: call.function.name,
            arguments,
        };
        if !valid_tool_call(&call) || !ids.insert(call.id.clone()) {
            return Err(invalid("invalid tool call or duplicate id"));
        }
        let definition = request
            .tools
            .iter()
            .find(|tool| tool.name == call.name)
            .ok_or_else(|| invalid("provider selected an undeclared tool"))?;
        let validator = definition
            .validator()
            .map_err(|_| invalid("invalid tool schema"))?;
        if !validator.is_valid(&Value::Object(call.arguments.clone())) {
            return Err(invalid("tool arguments do not match the declared schema"));
        }
        match &request.tool_choice {
            Some(ToolChoiceDto::Policy(ToolChoicePolicy::None)) => {
                return Err(invalid("provider ignored tool_choice none"));
            }
            Some(ToolChoiceDto::Named(named)) if named.name != call.name => {
                return Err(invalid("provider ignored named tool_choice"));
            }
            _ => {}
        }
        tool_calls.push(call);
    }
    let finish_reason = match choice.finish_reason.as_deref() {
        Some("length") => FinishReason::Length,
        Some("content_filter") => FinishReason::ContentFilter,
        Some("tool_calls") => FinishReason::ToolCalls,
        Some("stop") | None => {
            if tool_calls.is_empty() {
                FinishReason::Stop
            } else {
                FinishReason::ToolCalls
            }
        }
        _ => FinishReason::Unknown,
    };
    if finish_reason == FinishReason::ToolCalls && tool_calls.is_empty() {
        return Err(invalid("tool_calls finish reason without tool calls"));
    }
    if !tool_calls.is_empty() && finish_reason != FinishReason::ToolCalls {
        return Err(invalid("incomplete or inconsistent tool completion"));
    }
    if tool_calls.is_empty() && output_text.is_none() {
        return Err(invalid("missing message.content and tool_calls"));
    }
    if tool_calls.is_empty()
        && finish_reason == FinishReason::Stop
        && matches!(
            request.tool_choice,
            Some(ToolChoiceDto::Policy(ToolChoicePolicy::Required)) | Some(ToolChoiceDto::Named(_))
        )
    {
        return Err(invalid("provider did not return the required tool call"));
    }
    Ok(ProviderGenerateResult {
        output_text: output_text.unwrap_or_default(),
        tool_calls,
        finish_reason,
        usage,
    })
}

fn map_transport_error(error: reqwest::Error) -> ProviderError {
    // reqwest errors may contain URLs with secret query parameters.
    if error.is_timeout() {
        ProviderError::Timeout("request timed out".to_owned())
    } else {
        ProviderError::Transport("upstream request failed".to_owned())
    }
}

fn extract_error_message(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    value
        .pointer("/error/message")
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn is_tool_calling_unsupported(status: StatusCode, body: &str) -> bool {
    if !matches!(
        status,
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY | StatusCode::NOT_IMPLEMENTED
    ) {
        return false;
    }
    let Ok(error) = serde_json::from_str::<OpenAiErrorPayload>(body) else {
        return false;
    };
    let error = error.error;
    let message = error.message.to_ascii_lowercase();
    let mentions_tools = [
        "tools",
        "tool_choice",
        "tool_calls",
        "tool calling",
        "function calling",
    ]
    .iter()
    .any(|word| message.contains(word))
        || error.param.as_deref().is_some_and(|param| {
            param.starts_with("tools")
                || param.starts_with("tool_choice")
                || param.contains("tool_calls")
        });
    mentions_tools
        && (error.code.as_deref().is_some_and(|code| {
            matches!(
                code,
                "unsupported_parameter" | "unsupported_value" | "not_supported"
            )
        }) || [
            "not support",
            "unsupported",
            "not available",
            "does not allow",
        ]
        .iter()
        .any(|word| message.contains(word)))
}

fn should_retry_with_max_completion_tokens(
    status: StatusCode,
    body: &str,
    max_tokens: Option<i32>,
) -> bool {
    if max_tokens.is_none()
        || !matches!(
            status,
            StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY
        )
    {
        return false;
    }
    let Some(message) = extract_error_message(body) else {
        return false;
    };
    let message = message.to_ascii_lowercase();
    message.contains("max_tokens") && message.contains("max_completion_tokens")
}

#[derive(Debug, Serialize)]
struct OpenAiRequestPayload<'a> {
    model: &'a str,
    messages: Vec<OpenAiMessagePayload<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OpenAiToolDefinitionPayload<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<OpenAiToolChoicePayload<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<i32>,
}

impl<'a> OpenAiRequestPayload<'a> {
    fn from_generate_request(
        route: &'a ResolvedRouteRecord,
        request: &'a GenerateRequestDto,
        token_limit_parameter: TokenLimitParameter,
    ) -> Self {
        let (max_tokens, max_completion_tokens) = match token_limit_parameter {
            TokenLimitParameter::MaxTokens => (request.options.max_tokens, None),
            TokenLimitParameter::MaxCompletionTokens => (None, request.options.max_tokens),
        };
        Self {
            model: &route.external_model,
            messages: request
                .messages
                .iter()
                .map(OpenAiMessagePayload::from)
                .collect(),
            tools: request
                .tools
                .iter()
                .map(|tool| OpenAiToolDefinitionPayload {
                    kind: "function",
                    function: tool,
                })
                .collect(),
            tool_choice: request.tool_choice.as_ref().map(|choice| match choice {
                ToolChoiceDto::Policy(policy) => OpenAiToolChoicePayload::Policy(policy),
                ToolChoiceDto::Named(named) => OpenAiToolChoicePayload::Named {
                    kind: "function",
                    function: OpenAiNamedFunction { name: &named.name },
                },
            }),
            temperature: request.options.temperature,
            max_tokens,
            max_completion_tokens,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum TokenLimitParameter {
    MaxTokens,
    MaxCompletionTokens,
}

#[derive(Debug, Serialize)]
struct OpenAiMessagePayload<'a> {
    role: &'static str,
    content: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<OpenAiToolCallPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<&'a str>,
}

impl<'a> From<&'a GenerateMessageDto> for OpenAiMessagePayload<'a> {
    fn from(message: &'a GenerateMessageDto) -> Self {
        let (role, content, calls, tool_call_id) = match message {
            GenerateMessageDto::System { content } => {
                ("system", Some(content.as_str()), &[][..], None)
            }
            GenerateMessageDto::User { content } => ("user", Some(content.as_str()), &[][..], None),
            GenerateMessageDto::Assistant {
                content,
                tool_calls,
            } => ("assistant", content.as_deref(), tool_calls.as_slice(), None),
            GenerateMessageDto::Tool {
                content,
                tool_call_id,
            } => (
                "tool",
                Some(content.as_str()),
                &[][..],
                Some(tool_call_id.as_str()),
            ),
        };
        Self {
            role,
            content,
            tool_call_id,
            tool_calls: calls
                .iter()
                .map(|call| OpenAiToolCallPayload {
                    id: call.id.clone(),
                    kind: "function".to_owned(),
                    function: OpenAiFunctionCallPayload {
                        name: call.name.clone(),
                        arguments: Value::Object(call.arguments.clone()).to_string(),
                    },
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct OpenAiToolDefinitionPayload<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: &'a ToolDefinitionDto,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OpenAiToolChoicePayload<'a> {
    Policy(&'a ToolChoicePolicy),
    Named {
        #[serde(rename = "type")]
        kind: &'static str,
        function: OpenAiNamedFunction<'a>,
    },
}

#[derive(Debug, Serialize)]
struct OpenAiNamedFunction<'a> {
    name: &'a str,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiToolCallPayload {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    function: OpenAiFunctionCallPayload,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiFunctionCallPayload {
    name: String,
    arguments: String,
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
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiToolCallPayload>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsagePayload {
    #[serde(default)]
    prompt_tokens: Option<i32>,
    #[serde(default)]
    completion_tokens: Option<i32>,
}

impl From<OpenAiUsagePayload> for TokenUsageDto {
    fn from(usage: OpenAiUsagePayload) -> Self {
        Self {
            input_tokens: usage.prompt_tokens,
            output_tokens: usage.completion_tokens,
        }
    }
}

#[derive(Deserialize)]
struct OpenAiUsageEnvelope {
    usage: Option<OpenAiUsagePayload>,
}

#[derive(Deserialize)]
struct OpenAiErrorPayload {
    error: OpenAiErrorDetails,
}

#[derive(Deserialize)]
struct OpenAiErrorDetails {
    #[serde(default)]
    message: String,
    param: Option<String>,
    code: Option<String>,
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
    use crate::domain::{GenerateMessageDto, GenerateOptionsDto};

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
            provider_id: uuid::Uuid::new_v4(),
            external_model: "external-test-model".to_owned(),
            provider_code: "mock-provider".to_owned(),
            provider_kind: "openai-compatible".to_owned(),
            provider_base_url: format!("http://{address}"),
            provider_timeout_ms: 1_000,
        };
        let request = GenerateRequestDto {
            model: "test-model".to_owned(),
            messages: vec![GenerateMessageDto::User {
                content: "ping".to_owned(),
            }],
            tools: vec![],
            tool_choice: None,
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
        assert_eq!(response.finish_reason, FinishReason::Stop);
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

    #[tokio::test]
    async fn generate_retries_with_max_completion_tokens_when_model_rejects_max_tokens() {
        let request_count = Arc::new(AtomicUsize::new(0));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new()
            .route(
                "/chat/completions",
                post(handle_chat_completions_requiring_max_completion_tokens),
            )
            .with_state(MockApiState {
                request_count: request_count.clone(),
            });
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let provider = OpenAiCompatibleProvider::new().unwrap();
        let route = ResolvedRouteRecord {
            provider_id: uuid::Uuid::new_v4(),
            external_model: "external-test-model".to_owned(),
            provider_code: "mock-provider".to_owned(),
            provider_kind: "openai-compatible".to_owned(),
            provider_base_url: format!("http://{address}"),
            provider_timeout_ms: 1_000,
        };
        let request = GenerateRequestDto {
            model: "test-model".to_owned(),
            messages: vec![GenerateMessageDto::User {
                content: "ping".to_owned(),
            }],
            tools: vec![],
            tool_choice: None,
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

        assert_eq!(request_count.load(Ordering::SeqCst), 2);
        assert_eq!(response.output_text, "pong");
        assert_eq!(response.finish_reason, FinishReason::Stop);
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

    async fn handle_chat_completions_requiring_max_completion_tokens(
        State(state): State<MockApiState>,
        headers: HeaderMap,
        Json(payload): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        assert_eq!(
            headers
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer test-token")
        );
        assert_eq!(payload["model"], json!("external-test-model"));
        assert_eq!(payload["messages"][0]["role"], json!("user"));
        assert_eq!(payload["messages"][0]["content"], json!("ping"));
        assert_eq!(payload["temperature"].as_f64(), Some(0.2));

        match state.request_count.fetch_add(1, Ordering::SeqCst) {
            0 => {
                assert_eq!(payload["max_tokens"], json!(32));
                assert!(payload.get("max_completion_tokens").is_none());

                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": {
                            "message": "Unsupported parameter: 'max_tokens' is not supported with this model. Use 'max_completion_tokens' instead."
                        }
                    })),
                )
            }
            1 => {
                assert!(payload.get("max_tokens").is_none());
                assert_eq!(payload["max_completion_tokens"], json!(32));

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
                        ]
                    })),
                )
            }
            count => panic!("unexpected request count {count}"),
        }
    }
}
