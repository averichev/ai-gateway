use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// Internal text generation request accepted by AI Gateway clients.
///
/// This is the stable gateway contract. It is intentionally not an OpenAI,
/// Anthropic, or Gemini request DTO; provider-specific payloads are created by
/// provider adapters after model routing is resolved.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "model": "smart-default",
    "messages": [
        { "role": "system", "content": "Ты помогаешь CMS." },
        { "role": "user", "content": "Коротко опиши структуру меню." }
    ],
    "options": {
        "temperature": 0.2,
        "max_tokens": 1000
    }
}))]
pub struct GenerateRequestDto {
    /// Internal model alias configured in `model_routes`.
    ///
    /// Clients should use aliases such as `smart-default` instead of provider
    /// model ids. Gateway resolves the alias to a provider and external model.
    #[schema(example = "smart-default")]
    pub model: String,
    /// Ordered conversation messages to send to the resolved provider.
    ///
    /// The list must not be empty. Supported roles are `system`, `user`, and
    /// `assistant`.
    #[schema(min_items = 1)]
    pub messages: Vec<GenerateMessageDto>,
    /// Optional generation controls.
    ///
    /// Omitted fields are not sent to the provider adapter.
    #[serde(default)]
    pub options: GenerateOptionsDto,
}

/// Admin smoke-test request.
///
/// The shape is the same as `GenerateRequestDto`. Provider credentials are
/// resolved from the selected tenant's encrypted provider secret.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(example = json!({
    "model": "smart-default",
    "messages": [
        { "role": "user", "content": "Проверь маршрут и ответь одним предложением." }
    ],
    "options": {
        "temperature": 0.1,
        "max_tokens": 200
    }
}))]
pub struct AdminGenerateRequestDto {
    /// Flattened generation request fields.
    #[serde(flatten)]
    pub request: GenerateRequestDto,
}

/// One message in the generation conversation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GenerateMessageDto {
    /// Message role in the internal gateway contract.
    #[schema(example = "user")]
    pub role: MessageRole,
    /// Plain text message content.
    #[schema(example = "Коротко опиши структуру меню.")]
    pub content: String,
}

/// Optional model generation parameters.
#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct GenerateOptionsDto {
    /// Sampling temperature passed to the provider adapter when supported.
    ///
    /// Gateway does not normalize provider-specific ranges yet; use conservative
    /// values such as `0.0..1.0` for deterministic business flows.
    #[schema(example = 0.2)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Upper bound for generated tokens passed to the provider adapter when
    /// supported.
    #[schema(example = 1000)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i32>,
}

/// Supported message roles in the internal gateway contract.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = "user")]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// System-level instruction.
    System,
    /// User message.
    User,
    /// Assistant message, usually used when passing prior conversation history.
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

/// Normalized generation response returned by the gateway.
///
/// Provider-specific response envelopes are hidden behind provider adapters.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[schema(example = json!({
    "id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
    "model": "smart-default",
    "provider": "openai",
    "output_text": "Структура меню такая: ...",
    "finish_reason": "stop",
    "usage": {
        "input_tokens": 120,
        "output_tokens": 80
    }
}))]
pub struct GenerateResponseDto {
    /// Gateway request id. Use it to find the request in admin history and logs.
    #[schema(example = "req_4d4d6f4df77f4c1e8260f60f052f63cc")]
    pub id: String,
    /// Internal model alias from the request.
    #[schema(example = "smart-default")]
    pub model: String,
    /// Provider code selected by model routing.
    #[schema(example = "openai")]
    pub provider: String,
    /// Generated text returned by the provider adapter.
    #[schema(example = "Структура меню такая: ...")]
    pub output_text: String,
    /// Normalized finish reason from the provider adapter.
    #[schema(example = "stop")]
    pub finish_reason: String,
    /// Token usage reported by the provider, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsageDto>,
}

/// Normalized token usage.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenUsageDto {
    /// Tokens consumed by the input prompt, when the provider reports them.
    #[schema(example = 120)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<i32>,
    /// Tokens generated by the model, when the provider reports them.
    #[schema(example = 80)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<i32>,
}

/// Standard gateway error response.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[schema(example = json!({
    "request_id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
    "error": {
        "code": "provider_timeout",
        "message": "Провайдер не ответил вовремя"
    }
}))]
pub struct ErrorResponseDto {
    /// Gateway request id for diagnostics.
    #[schema(example = "req_4d4d6f4df77f4c1e8260f60f052f63cc")]
    pub request_id: String,
    /// Machine-readable code and human-readable message.
    pub error: ErrorBodyDto,
}

/// Error details returned by the gateway.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ErrorBodyDto {
    /// Stable machine-readable error code.
    ///
    /// Known values include `invalid_request`, `route_not_found`,
    /// `provider_timeout`, `provider_error`, `provider_invalid_response`,
    /// `provider_transport_error`, `gateway_misconfigured`, `storage_error`,
    /// and `not_found`.
    #[schema(example = "provider_timeout")]
    pub code: String,
    /// Human-readable error message safe to show in admin tooling.
    #[schema(example = "Провайдер не ответил вовремя")]
    pub message: String,
}

/// HTTP service health response.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct HealthResponseDto {
    /// Health status of the HTTP service.
    #[schema(example = "ok")]
    pub status: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ProviderConfigRecord {
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub code: String,
    pub kind: String,
    pub base_url: String,
    pub api_key_configured: bool,
    pub is_enabled: bool,
    pub timeout_ms: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ModelRouteRecord {
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub alias: String,
    pub provider_code: String,
    pub external_model: String,
    pub is_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ResolvedRouteRecord {
    pub provider_id: uuid::Uuid,
    pub external_model: String,
    pub provider_code: String,
    pub provider_kind: String,
    pub provider_base_url: String,
    pub provider_timeout_ms: i32,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct RequestLogRecord {
    pub id: String,
    pub tenant_id: uuid::Uuid,
    pub gateway_client_id: Option<uuid::Uuid>,
    pub gateway_client_name: Option<String>,
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
    pub tenant_id: uuid::Uuid,
    pub gateway_client_id: Option<uuid::Uuid>,
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

#[derive(Debug, Clone, FromRow)]
pub struct ProviderSecretRecord {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub algorithm: String,
    pub key_version: i32,
}

#[derive(Debug, Clone, FromRow)]
pub struct UserRecord {
    pub id: uuid::Uuid,
    pub email: String,
    pub password_hash: String,
    pub global_role: String,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AdminUserRecord {
    pub id: uuid::Uuid,
    pub email: String,
    pub global_role: String,
    pub is_enabled: bool,
    pub tenant_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct TenantRecord {
    pub id: uuid::Uuid,
    pub name: String,
    pub slug: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct TenantAccessRecord {
    pub tenant_id: uuid::Uuid,
    pub role: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct SessionRecord {
    pub user_id: uuid::Uuid,
    pub email: String,
    pub global_role: String,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct GatewayClientRecord {
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub name: String,
    pub token_prefix: String,
    pub is_enabled: bool,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct GatewayClientAuthRecord {
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BootstrapStatusDto {
    pub has_users: bool,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct BootstrapRegisterRequestDto {
    #[schema(example = "owner@example.com")]
    pub email: String,
    #[schema(example = "change-me-strong-password", write_only)]
    pub password: String,
    #[schema(example = "Default tenant")]
    #[serde(default)]
    pub tenant_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct LoginRequestDto {
    #[schema(example = "owner@example.com")]
    pub email: String,
    #[schema(example = "change-me-strong-password", write_only)]
    pub password: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AuthResponseDto {
    pub token: String,
    pub user: CurrentUserDto,
    pub tenants: Vec<TenantDto>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CurrentUserDto {
    pub id: uuid::Uuid,
    pub email: String,
    pub global_role: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TenantDto {
    pub id: uuid::Uuid,
    pub name: String,
    pub slug: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateTenantRequestDto {
    #[schema(example = "Factum Production")]
    pub name: String,
    #[schema(example = "factum-production")]
    #[serde(default)]
    pub slug: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateGatewayClientRequestDto {
    #[schema(example = "factum-backend-prod")]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GatewayClientDto {
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub name: String,
    pub token_prefix: String,
    pub is_enabled: bool,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CreatedGatewayClientDto {
    #[serde(flatten)]
    pub client: GatewayClientDto,
    pub token: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SaveProviderRequestDto {
    #[schema(example = "openai")]
    pub code: String,
    #[schema(example = "openai-compatible")]
    pub kind: String,
    #[schema(example = "https://api.openai.com/v1")]
    pub base_url: String,
    #[schema(example = true)]
    #[serde(default = "default_enabled")]
    pub is_enabled: bool,
    #[schema(example = 60000)]
    #[serde(default = "default_provider_timeout_ms")]
    pub timeout_ms: i32,
    #[schema(example = "sk-test-placeholder", write_only)]
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SaveProviderSecretRequestDto {
    #[schema(example = "sk-test-placeholder", write_only)]
    pub api_key: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SaveModelRouteRequestDto {
    #[schema(example = "smart-default")]
    pub alias: String,
    #[schema(example = "openai")]
    pub provider_code: String,
    #[schema(example = "gpt-4.1-mini")]
    pub external_model: String,
    #[schema(example = true)]
    #[serde(default = "default_enabled")]
    pub is_enabled: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AdminUserDto {
    pub id: uuid::Uuid,
    pub email: String,
    pub global_role: String,
    pub is_enabled: bool,
    pub tenant_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateUserRequestDto {
    #[schema(example = "admin@example.com")]
    pub email: String,
    #[schema(example = "change-me-strong-password", write_only)]
    pub password: String,
    #[serde(default)]
    pub tenant_id: Option<uuid::Uuid>,
    #[schema(example = "viewer")]
    #[serde(default)]
    pub tenant_role: Option<String>,
}

/// One item in the admin request history list.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RequestListItemDto {
    /// Gateway request id.
    #[schema(example = "req_4d4d6f4df77f4c1e8260f60f052f63cc")]
    pub id: String,
    /// Tenant that owns the request.
    pub tenant_id: uuid::Uuid,
    /// Machine client that authenticated the request, if applicable.
    pub gateway_client_id: Option<uuid::Uuid>,
    /// Machine client display name, if available.
    pub gateway_client_name: Option<String>,
    /// Request creation time in UTC.
    #[schema(example = "2026-04-17T18:20:00Z")]
    pub created_at: DateTime<Utc>,
    /// Internal model alias requested by the client.
    #[schema(example = "smart-default")]
    pub model_alias: String,
    /// Provider code selected by routing, or `null` if routing failed first.
    #[schema(example = "openai")]
    pub provider_code: Option<String>,
    /// External provider model id selected by routing, or `null` if routing
    /// failed first.
    #[schema(example = "gpt-4.1-mini")]
    pub external_model: Option<String>,
    /// Final request status.
    ///
    /// Typical values: `success`, `invalid_request`, `route_not_found`,
    /// `provider_timeout`, `provider_error`, `provider_invalid_response`,
    /// `provider_transport_error`, `gateway_misconfigured`, `storage_error`.
    #[schema(example = "success")]
    pub status: String,
    /// End-to-end request latency in milliseconds, when available.
    #[schema(example = 1824)]
    pub latency_ms: Option<i64>,
    /// Stored diagnostic error message, or `null` for successful requests.
    #[schema(example = "provider timeout: request timed out")]
    pub error_message: Option<String>,
    /// Input tokens reported by the provider, when available.
    #[schema(example = 1200)]
    pub input_tokens: Option<i32>,
    /// Output tokens reported by the provider, when available.
    #[schema(example = 350)]
    pub output_tokens: Option<i32>,
}

/// Detailed admin view of a single request.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RequestDetailsDto {
    /// Gateway request id.
    #[schema(example = "req_4d4d6f4df77f4c1e8260f60f052f63cc")]
    pub id: String,
    /// Tenant that owns the request.
    pub tenant_id: uuid::Uuid,
    /// Machine client that authenticated the request, if applicable.
    pub gateway_client_id: Option<uuid::Uuid>,
    /// Machine client display name, if available.
    pub gateway_client_name: Option<String>,
    /// Request creation time in UTC.
    #[schema(example = "2026-04-17T18:20:00Z")]
    pub created_at: DateTime<Utc>,
    /// Internal model alias requested by the client.
    #[schema(example = "smart-default")]
    pub model_alias: String,
    /// Provider code selected by routing, or `null` if routing failed first.
    #[schema(example = "openai")]
    pub provider_code: Option<String>,
    /// External provider model id selected by routing, or `null` if routing
    /// failed first.
    #[schema(example = "gpt-4.1-mini")]
    pub external_model: Option<String>,
    /// Final request status.
    #[schema(example = "success")]
    pub status: String,
    /// End-to-end request latency in milliseconds, when available.
    #[schema(example = 1824)]
    pub latency_ms: Option<i64>,
    /// Stored diagnostic error message, or `null` for successful requests.
    #[schema(example = "provider timeout: request timed out")]
    pub error_message: Option<String>,
    /// Input tokens reported by the provider, when available.
    #[schema(example = 1200)]
    pub input_tokens: Option<i32>,
    /// Output tokens reported by the provider, when available.
    #[schema(example = 350)]
    pub output_tokens: Option<i32>,
    /// Truncated prompt preview stored for diagnostics.
    #[schema(example = "system: Ты помогаешь CMS.\nuser: Коротко опиши структуру меню.")]
    pub prompt_preview: Option<String>,
    /// Truncated response preview stored for diagnostics.
    #[schema(example = "Структура меню такая: ...")]
    pub response_preview: Option<String>,
}

/// Provider configuration visible to the admin API.
///
/// Secrets are stored separately in `provider_secrets` as encrypted-at-rest
/// ciphertext. The plaintext API key is never returned by API responses.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProviderDto {
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    /// Stable provider code referenced by model routes.
    #[schema(example = "openai")]
    pub code: String,
    /// Provider adapter kind registered in the gateway.
    ///
    /// Current MVP supports `openai-compatible`.
    #[schema(example = "openai-compatible")]
    pub kind: String,
    /// Base URL for the provider adapter.
    ///
    /// For `openai-compatible`, the adapter calls
    /// `POST {base_url}/chat/completions`.
    #[schema(example = "https://api.openai.com/v1")]
    pub base_url: String,
    /// Whether encrypted provider secret exists for this provider.
    #[schema(example = true)]
    pub api_key_configured: bool,
    /// Whether this provider can be used by model routing.
    #[schema(example = true)]
    pub is_enabled: bool,
    /// Per-request upstream timeout in milliseconds.
    #[schema(example = 30000)]
    pub timeout_ms: i32,
    /// Last provider configuration update time in UTC.
    #[schema(example = "2026-04-17T18:20:00Z")]
    pub updated_at: DateTime<Utc>,
}

/// Model alias route visible to the admin API.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ModelRouteDto {
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    /// Internal model alias accepted by `POST /api/v1/generate`.
    #[schema(example = "smart-default")]
    pub alias: String,
    /// Provider code selected for the alias.
    #[schema(example = "openai")]
    pub provider_code: String,
    /// Provider-specific model id used only inside the selected adapter.
    #[schema(example = "gpt-4.1-mini")]
    pub external_model: String,
    /// Whether this route can be resolved by generation requests.
    #[schema(example = true)]
    pub is_enabled: bool,
    /// Last route update time in UTC.
    #[schema(example = "2026-04-17T18:20:00Z")]
    pub updated_at: DateTime<Utc>,
}

impl From<RequestLogRecord> for RequestListItemDto {
    fn from(value: RequestLogRecord) -> Self {
        Self {
            id: value.id,
            tenant_id: value.tenant_id,
            gateway_client_id: value.gateway_client_id,
            gateway_client_name: value.gateway_client_name,
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
            tenant_id: value.tenant_id,
            gateway_client_id: value.gateway_client_id,
            gateway_client_name: value.gateway_client_name,
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
            id: value.id,
            tenant_id: value.tenant_id,
            code: value.code,
            kind: value.kind,
            base_url: value.base_url,
            api_key_configured: value.api_key_configured,
            is_enabled: value.is_enabled,
            timeout_ms: value.timeout_ms,
            updated_at: value.updated_at,
        }
    }
}

impl From<ModelRouteRecord> for ModelRouteDto {
    fn from(value: ModelRouteRecord) -> Self {
        Self {
            id: value.id,
            tenant_id: value.tenant_id,
            alias: value.alias,
            provider_code: value.provider_code,
            external_model: value.external_model,
            is_enabled: value.is_enabled,
            updated_at: value.updated_at,
        }
    }
}

impl TenantDto {
    pub fn from_record_with_role(value: TenantRecord, role: impl Into<String>) -> Self {
        Self {
            id: value.id,
            name: value.name,
            slug: value.slug,
            role: role.into(),
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<GatewayClientRecord> for GatewayClientDto {
    fn from(value: GatewayClientRecord) -> Self {
        Self {
            id: value.id,
            tenant_id: value.tenant_id,
            name: value.name,
            token_prefix: value.token_prefix,
            is_enabled: value.is_enabled,
            last_used_at: value.last_used_at,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<AdminUserRecord> for AdminUserDto {
    fn from(value: AdminUserRecord) -> Self {
        Self {
            id: value.id,
            email: value.email,
            global_role: value.global_role,
            is_enabled: value.is_enabled,
            tenant_count: value.tenant_count,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

fn default_enabled() -> bool {
    true
}

fn default_provider_timeout_ms() -> i32 {
    60_000
}
