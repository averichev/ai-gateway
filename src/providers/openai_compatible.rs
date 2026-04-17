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
