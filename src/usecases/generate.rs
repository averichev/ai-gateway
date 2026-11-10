use std::{sync::Arc, time::Instant};

use axum::http::StatusCode;
use tracing::{error, warn};
use uuid::Uuid;

use crate::{
    domain::{
        ErrorBodyDto, ErrorResponseDto, GenerateMessageDto, GenerateRequestDto,
        GenerateResponseDto, ProviderGenerateResult, RequestLogInsert, ResolvedRouteRecord,
    },
    providers::{ProviderError, ProviderRegistry},
    repositories::{PostgresRequestsRepository, PostgresRoutesRepository, RepositoryError},
    security::SecretCrypto,
};

#[derive(Clone)]
pub struct GenerateService {
    routes_repo: Arc<PostgresRoutesRepository>,
    requests_repo: Arc<PostgresRequestsRepository>,
    providers: ProviderRegistry,
    secret_crypto: SecretCrypto,
    request_preview_chars: usize,
    response_preview_chars: usize,
}

impl GenerateService {
    pub fn new(
        routes_repo: Arc<PostgresRoutesRepository>,
        requests_repo: Arc<PostgresRequestsRepository>,
        providers: ProviderRegistry,
        secret_crypto: SecretCrypto,
        request_preview_chars: usize,
        response_preview_chars: usize,
    ) -> Self {
        Self {
            routes_repo,
            requests_repo,
            providers,
            secret_crypto,
            request_preview_chars,
            response_preview_chars,
        }
    }

    pub async fn generate_for_client(
        &self,
        request: GenerateRequestDto,
        tenant_id: Uuid,
        gateway_client_id: Uuid,
    ) -> Result<GenerateResponseDto, ServiceError> {
        self.generate_inner(request, tenant_id, Some(gateway_client_id))
            .await
    }

    pub async fn generate_for_admin(
        &self,
        request: GenerateRequestDto,
        tenant_id: Uuid,
    ) -> Result<GenerateResponseDto, ServiceError> {
        self.generate_inner(request, tenant_id, None).await
    }

    async fn generate_inner(
        &self,
        request: GenerateRequestDto,
        tenant_id: Uuid,
        gateway_client_id: Option<Uuid>,
    ) -> Result<GenerateResponseDto, ServiceError> {
        let request_id = new_request_id();
        // Tools, arguments and results may contain secrets. Do not persist
        // previews for any request participating in a tool conversation.
        let prompt_preview = if request.uses_tools() {
            None
        } else {
            preview_messages(&request.messages, self.request_preview_chars)
        };

        if let Err(message) = request.validate() {
            let message = message.to_owned();
            self.store_request_log(RequestLogInsert {
                id: request_id.clone(),
                tenant_id,
                gateway_client_id,
                model_alias: request.model.clone(),
                provider_code: None,
                external_model: None,
                status: "invalid_request".to_owned(),
                latency_ms: None,
                error_message: Some(message.clone()),
                input_tokens: None,
                output_tokens: None,
                prompt_preview: prompt_preview.clone(),
                response_preview: None,
            })
            .await;

            return Err(ServiceError::new(
                request_id,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                message,
            ));
        }

        let started_at = Instant::now();

        let route = self
            .routes_repo
            .resolve_route(tenant_id, &request.model)
            .await
            .map_err(|error| {
                error!(error = %error, "failed to resolve model route");
                ServiceError::new(
                    request_id.clone(),
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage_error",
                    "Не удалось прочитать конфигурацию маршрутов".to_owned(),
                )
            })?;

        let route = match route {
            Some(route) => route,
            None => {
                let message = format!("Alias `{}` не найден или выключен", request.model);
                self.store_request_log(RequestLogInsert {
                    id: request_id.clone(),
                    tenant_id,
                    gateway_client_id,
                    model_alias: request.model.clone(),
                    provider_code: None,
                    external_model: None,
                    status: "route_not_found".to_owned(),
                    latency_ms: Some(started_at.elapsed().as_millis() as i64),
                    error_message: Some(message.clone()),
                    input_tokens: None,
                    output_tokens: None,
                    prompt_preview: prompt_preview.clone(),
                    response_preview: None,
                })
                .await;

                return Err(ServiceError::new(
                    request_id,
                    StatusCode::NOT_FOUND,
                    "route_not_found",
                    message,
                ));
            }
        };

        let context = GenerationContext {
            request_id: &request_id,
            request: &request,
            tenant_id,
            gateway_client_id,
            prompt_preview: prompt_preview.clone(),
            started_at,
        };

        let Some(adapter) = self.providers.get(&route.provider_kind) else {
            return Err(self
                .provider_error_to_service_error(
                    &context,
                    &route,
                    ProviderError::UnsupportedKind(route.provider_kind.clone()),
                )
                .await);
        };
        let provider_secret = self
            .routes_repo
            .get_provider_secret(tenant_id, route.provider_id)
            .await
            .map_err(|error| {
                error!(error = %error, "failed to read provider secret");
                ServiceError::new(
                    request_id.clone(),
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage_error",
                    "Не удалось прочитать provider secret",
                )
            })?;
        let Some(provider_secret) = provider_secret else {
            return Err(self
                .provider_error_to_service_error(
                    &context,
                    &route,
                    ProviderError::MissingProviderSecret(route.provider_code.clone()),
                )
                .await);
        };
        if provider_secret.algorithm != "AES-256-GCM" || provider_secret.key_version != 1 {
            return Err(self
                .provider_error_to_service_error(
                    &context,
                    &route,
                    ProviderError::ProviderSecretUnavailable(route.provider_code.clone()),
                )
                .await);
        }
        let api_key = match self
            .secret_crypto
            .decrypt(&provider_secret.ciphertext, &provider_secret.nonce)
        {
            Ok(key) => key,
            Err(_) => {
                return Err(self
                    .provider_error_to_service_error(
                        &context,
                        &route,
                        ProviderError::ProviderSecretUnavailable(route.provider_code.clone()),
                    )
                    .await);
            }
        };
        let result = match adapter.generate(&route, &request, &api_key).await {
            Ok(result) => result,
            Err(error) => {
                return Err(self
                    .provider_error_to_service_error(&context, &route, error)
                    .await);
            }
        };

        let latency_ms = started_at.elapsed().as_millis() as i64;
        let response_preview = if request.uses_tools() || !result.tool_calls.is_empty() {
            None
        } else {
            truncate_preview(&result.output_text, self.response_preview_chars)
        };

        self.store_request_log(RequestLogInsert {
            id: request_id.clone(),
            tenant_id,
            gateway_client_id,
            model_alias: request.model.clone(),
            provider_code: Some(route.provider_code.clone()),
            external_model: Some(route.external_model.clone()),
            status: "success".to_owned(),
            latency_ms: Some(latency_ms),
            error_message: None,
            input_tokens: result.usage.as_ref().and_then(|usage| usage.input_tokens),
            output_tokens: result.usage.as_ref().and_then(|usage| usage.output_tokens),
            prompt_preview,
            response_preview,
        })
        .await;

        Ok(build_response(
            request_id,
            &request.model,
            &route.provider_code,
            result,
        ))
    }

    async fn store_request_log(&self, request: RequestLogInsert) {
        if let Err(error) = self.requests_repo.insert_request(&request).await {
            warn!(error = %error, request_id = %request.id, "failed to persist request log");
        }
    }

    async fn provider_error_to_service_error(
        &self,
        context: &GenerationContext<'_>,
        route: &ResolvedRouteRecord,
        error: ProviderError,
    ) -> ServiceError {
        let request_id = context.request_id.to_owned();
        let model_alias = &context.request.model;
        let provider_code = &route.provider_code;
        let external_model = &route.external_model;
        let tenant_id = context.tenant_id;
        let gateway_client_id = context.gateway_client_id;
        let prompt_preview = context.prompt_preview.clone();
        let latency_ms = context.started_at.elapsed().as_millis() as i64;
        let status_code = match error {
            ProviderError::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
            ProviderError::Upstream { .. }
            | ProviderError::Transport(_)
            | ProviderError::InvalidResponse { .. } => StatusCode::BAD_GATEWAY,
            ProviderError::ToolCallingNotSupported => StatusCode::BAD_REQUEST,
            ProviderError::MissingProviderSecret(_)
            | ProviderError::ProviderSecretUnavailable(_)
            | ProviderError::UnsupportedKind(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let client_code = error.client_code().to_owned();
        let client_message = error.client_message();
        let storage_status = error.storage_status();
        let storage_message = error.to_string();
        let input_tokens = error.usage().and_then(|usage| usage.input_tokens);
        let output_tokens = error.usage().and_then(|usage| usage.output_tokens);

        self.store_request_log(RequestLogInsert {
            id: request_id.clone(),
            tenant_id,
            gateway_client_id,
            model_alias: model_alias.to_owned(),
            provider_code: Some(provider_code.to_owned()),
            external_model: Some(external_model.to_owned()),
            status: storage_status,
            latency_ms: Some(latency_ms),
            error_message: Some(storage_message),
            input_tokens,
            output_tokens,
            prompt_preview,
            response_preview: None,
        })
        .await;

        ServiceError::new(request_id, status_code, client_code, client_message)
    }
}

struct GenerationContext<'a> {
    request_id: &'a str,
    request: &'a GenerateRequestDto,
    tenant_id: Uuid,
    gateway_client_id: Option<Uuid>,
    prompt_preview: Option<String>,
    started_at: Instant,
}

fn build_response(
    request_id: String,
    model_alias: &str,
    provider_code: &str,
    result: ProviderGenerateResult,
) -> GenerateResponseDto {
    GenerateResponseDto {
        id: request_id,
        model: model_alias.to_owned(),
        provider: provider_code.to_owned(),
        output_text: result.output_text,
        tool_calls: result.tool_calls,
        finish_reason: result.finish_reason,
        usage: result.usage,
    }
}

fn new_request_id() -> String {
    format!("req_{}", Uuid::new_v4().simple())
}

fn preview_messages(messages: &[GenerateMessageDto], max_chars: usize) -> Option<String> {
    let combined = messages
        .iter()
        .filter_map(|message| match message {
            GenerateMessageDto::System { content } => Some(format!("system: {content}")),
            GenerateMessageDto::User { content } => Some(format!("user: {content}")),
            GenerateMessageDto::Assistant {
                content,
                tool_calls,
            } if tool_calls.is_empty() => content.as_ref().map(|text| format!("assistant: {text}")),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    truncate_preview(&combined, max_chars)
}

fn truncate_preview(value: &str, max_chars: usize) -> Option<String> {
    if value.trim().is_empty() {
        return None;
    }

    let mut preview = value.chars().take(max_chars).collect::<String>();

    if value.chars().count() > max_chars {
        preview.push_str("...");
    }

    Some(preview)
}

#[derive(Debug, Clone)]
pub struct ServiceError {
    pub request_id: String,
    pub status_code: StatusCode,
    pub code: String,
    pub message: String,
}

impl ServiceError {
    pub fn new(
        request_id: String,
        status_code: StatusCode,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            request_id,
            status_code,
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn to_error_response(&self) -> ErrorResponseDto {
        ErrorResponseDto {
            request_id: self.request_id.clone(),
            error: ErrorBodyDto {
                code: self.code.clone(),
                message: self.message.clone(),
            },
        }
    }
}

impl From<RepositoryError> for ServiceError {
    fn from(error: RepositoryError) -> Self {
        error!(error = %error, "repository failure");
        Self::new(
            new_request_id(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_error",
            "Не удалось прочитать данные из хранилища".to_owned(),
        )
    }
}
