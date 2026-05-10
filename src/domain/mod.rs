use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GenerateRequestDto {
    pub model: String,
    pub messages: Vec<GenerateMessageDto>,
    #[serde(default)]
    pub options: GenerateOptionsDto,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AdminGenerateRequestDto {
    #[serde(flatten)]
    pub request: GenerateRequestDto,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GenerateMessageDto {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct GenerateOptionsDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

impl MessageRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GenerateResponseDto {
    pub id: String,
    pub model: String,
    pub provider: String,
    pub output_text: String,
    pub finish_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsageDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenUsageDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<i32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ErrorResponseDto {
    pub request_id: String,
    pub error: ErrorBodyDto,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ErrorBodyDto {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct HealthResponseDto {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ProviderConfigRecord {
    pub id: uuid::Uuid,
    pub code: String,
    pub kind: String,
    pub base_url: String,
    pub api_key_env: String,
    pub is_enabled: bool,
    pub timeout_ms: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ModelRouteRecord {
    pub id: uuid::Uuid,
    pub alias: String,
    pub provider_code: String,
    pub external_model: String,
    pub is_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ResolvedRouteRecord {
    pub external_model: String,
    pub provider_code: String,
    pub provider_kind: String,
    pub provider_base_url: String,
    pub provider_api_key_env: String,
    pub provider_timeout_ms: i32,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct RequestLogRecord {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub model_alias: String,
    pub provider_code: Option<String>,
    pub external_model: Option<String>,
    pub status: String,
    pub latency_ms: Option<i64>,
    pub error_message: Option<String>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub prompt_preview: Option<String>,
    pub response_preview: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RequestLogInsert {
    pub id: String,
    pub model_alias: String,
    pub provider_code: Option<String>,
    pub external_model: Option<String>,
    pub status: String,
    pub latency_ms: Option<i64>,
    pub error_message: Option<String>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub prompt_preview: Option<String>,
    pub response_preview: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProviderGenerateResult {
    pub output_text: String,
    pub finish_reason: String,
    pub usage: Option<TokenUsageDto>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RequestListItemDto {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub model_alias: String,
    pub provider_code: Option<String>,
    pub external_model: Option<String>,
    pub status: String,
    pub latency_ms: Option<i64>,
    pub error_message: Option<String>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RequestDetailsDto {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub model_alias: String,
    pub provider_code: Option<String>,
    pub external_model: Option<String>,
    pub status: String,
    pub latency_ms: Option<i64>,
    pub error_message: Option<String>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub prompt_preview: Option<String>,
    pub response_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProviderDto {
    pub code: String,
    pub kind: String,
    pub base_url: String,
    pub api_key_env: String,
    pub is_enabled: bool,
    pub timeout_ms: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ModelRouteDto {
    pub alias: String,
    pub provider_code: String,
    pub external_model: String,
    pub is_enabled: bool,
    pub updated_at: DateTime<Utc>,
}

impl From<RequestLogRecord> for RequestListItemDto {
    fn from(value: RequestLogRecord) -> Self {
        Self {
            id: value.id,
            created_at: value.created_at,
            model_alias: value.model_alias,
            provider_code: value.provider_code,
            external_model: value.external_model,
            status: value.status,
            latency_ms: value.latency_ms,
            error_message: value.error_message,
            input_tokens: value.input_tokens,
            output_tokens: value.output_tokens,
        }
    }
}

impl From<RequestLogRecord> for RequestDetailsDto {
    fn from(value: RequestLogRecord) -> Self {
        Self {
            id: value.id,
            created_at: value.created_at,
            model_alias: value.model_alias,
            provider_code: value.provider_code,
            external_model: value.external_model,
            status: value.status,
            latency_ms: value.latency_ms,
            error_message: value.error_message,
            input_tokens: value.input_tokens,
            output_tokens: value.output_tokens,
            prompt_preview: value.prompt_preview,
            response_preview: value.response_preview,
        }
    }
}

impl From<ProviderConfigRecord> for ProviderDto {
    fn from(value: ProviderConfigRecord) -> Self {
        Self {
            code: value.code,
            kind: value.kind,
            base_url: value.base_url,
            api_key_env: value.api_key_env,
            is_enabled: value.is_enabled,
            timeout_ms: value.timeout_ms,
            updated_at: value.updated_at,
        }
    }
}

impl From<ModelRouteRecord> for ModelRouteDto {
    fn from(value: ModelRouteRecord) -> Self {
        Self {
            alias: value.alias,
            provider_code: value.provider_code,
            external_model: value.external_model,
            is_enabled: value.is_enabled,
            updated_at: value.updated_at,
        }
    }
}
