use std::{env, sync::Arc, time::Instant};

use axum::http::StatusCode;
use tracing::{error, warn};
use uuid::Uuid;

use crate::{
    domain::{
        ErrorBodyDto, ErrorResponseDto, GenerateMessageDto, GenerateRequestDto,
        GenerateResponseDto, ProviderGenerateResult, RequestLogInsert,
    },
    providers::{ProviderError, ProviderRegistry},
    repositories::{PostgresRequestsRepository, PostgresRoutesRepository, RepositoryError},
};

#[derive(Clone)]
pub struct GenerateService {
    routes_repo: Arc<PostgresRoutesRepository>,
    requests_repo: Arc<PostgresRequestsRepository>,
    providers: ProviderRegistry,
    request_preview_chars: usize,
    response_preview_chars: usize,
}

impl GenerateService {
    pub fn new(
        routes_repo: Arc<PostgresRoutesRepository>,
        requests_repo: Arc<PostgresRequestsRepository>,
        providers: ProviderRegistry,
        request_preview_chars: usize,
        response_preview_chars: usize,
    ) -> Self {
        Self {
            routes_repo,
            requests_repo,
            providers,
            request_preview_chars,
            response_preview_chars,
        }
    }

    pub async fn generate(
        &self,
        request: GenerateRequestDto,
    ) -> Result<GenerateResponseDto, ServiceError> {
        self.generate_inner(request, None).await
    }

    pub async fn generate_with_api_key_override(
        &self,
        request: GenerateRequestDto,
        api_key_override: Option<String>,
    ) -> Result<GenerateResponseDto, ServiceError> {
        self.generate_inner(request, api_key_override).await
    }

    async fn generate_inner(
        &self,
        request: GenerateRequestDto,
        api_key_override: Option<String>,
    ) -> Result<GenerateResponseDto, ServiceError> {
        let request_id = new_request_id();
        let prompt_preview = preview_messages(&request.messages, self.request_preview_chars);

        if request.model.trim().is_empty() || request.messages.is_empty() {
            let message = "Поля `model` и `messages` обязательны".to_owned();
            self.store_request_log(RequestLogInsert {
                id: request_id.clone(),
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
            .resolve_route(&request.model)
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

        let adapter = self
            .providers
            .get(&route.provider_kind)
            .ok_or_else(|| ProviderError::UnsupportedKind(route.provider_kind.clone()))
            .map_err(|error| {
                self.provider_error_to_service_error(
                    request_id.clone(),
                    &request.model,
                    &route.provider_code,
                    &route.external_model,
                    prompt_preview.clone(),
                    started_at.elapsed().as_millis() as i64,
                    error,
                )
            })?;

        let api_key = match normalize_api_key_override(api_key_override) {
            Some(api_key) => Ok(api_key),
            None => env::var(&route.provider_api_key_env),
        }
        .map_err(|_| ProviderError::MissingApiKey(route.provider_api_key_env.clone()))
        .map_err(|error| {
            self.provider_error_to_service_error(
                request_id.clone(),
                &request.model,
                &route.provider_code,
                &route.external_model,
                prompt_preview.clone(),
                started_at.elapsed().as_millis() as i64,
                error,
            )
        })?;

        let result = adapter
            .generate(&route, &request, &api_key)
            .await
            .map_err(|error| {
                self.provider_error_to_service_error(
                    request_id.clone(),
                    &request.model,
                    &route.provider_code,
                    &route.external_model,
                    prompt_preview.clone(),
                    started_at.elapsed().as_millis() as i64,
                    error,
                )
            })?;

        let latency_ms = started_at.elapsed().as_millis() as i64;
        let response_preview = truncate_preview(&result.output_text, self.response_preview_chars);

        self.store_request_log(RequestLogInsert {
            id: request_id.clone(),
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

    fn provider_error_to_service_error(
        &self,
        request_id: String,
        model_alias: &str,
        provider_code: &str,
        external_model: &str,
        prompt_preview: Option<String>,
        latency_ms: i64,
        error: ProviderError,
    ) -> ServiceError {
        let status_code = match error {
            ProviderError::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
            ProviderError::Upstream { .. }
            | ProviderError::Transport(_)
            | ProviderError::InvalidResponse(_) => StatusCode::BAD_GATEWAY,
            ProviderError::MissingApiKey(_) | ProviderError::UnsupportedKind(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };

        let client_code = error.client_code().to_owned();
        let client_message = error.client_message();
        let storage_status = error.storage_status();
        let storage_message = error.to_string();

        let repo = self.requests_repo.clone();
        let request_id_for_log = request_id.clone();
        let model_alias = model_alias.to_owned();
        let provider_code = provider_code.to_owned();
        let external_model = external_model.to_owned();

        tokio::spawn(async move {
            let request = RequestLogInsert {
                id: request_id_for_log.clone(),
                model_alias,
                provider_code: Some(provider_code),
                external_model: Some(external_model),
                status: storage_status,
                latency_ms: Some(latency_ms),
                error_message: Some(storage_message),
                input_tokens: None,
                output_tokens: None,
                prompt_preview,
                response_preview: None,
            };

            if let Err(save_error) = repo.insert_request(&request).await {
                warn!(
                    error = %save_error,
                    request_id = %request_id_for_log,
                    "failed to persist request error log"
                );
            }
        });

        ServiceError::new(request_id, status_code, client_code, client_message)
    }
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
        finish_reason: result.finish_reason,
        usage: result.usage,
    }
}

fn new_request_id() -> String {
    format!("req_{}", Uuid::new_v4().simple())
}

fn normalize_api_key_override(api_key: Option<String>) -> Option<String> {
    api_key
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn preview_messages(messages: &[GenerateMessageDto], max_chars: usize) -> Option<String> {
    let combined = messages
        .iter()
        .map(|message| format!("{}: {}", message.role.as_str(), message.content))
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
