use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;

use crate::{
    app::{AppHttpError, AppState},
    domain::{
        AdminGenerateRequestDto, GenerateResponseDto, ModelRouteDto, ProviderDto,
        RequestDetailsDto, RequestListItemDto,
    },
};

#[derive(Debug, Deserialize)]
pub struct RequestsQuery {
    #[serde(default = "default_limit")]
    limit: i64,
}

const LIST_REQUESTS_DESCRIPTION: &str = r#"Возвращает последние записи истории LLM-вызовов.

Список предназначен для admin UI и диагностики: здесь есть routing/status/latency/token usage, но нет полных prompt/response payloads. Для просмотра сохраненных preview одного запроса используйте `GET /api/admin/requests/{id}`.

Параметр `limit` автоматически ограничивается диапазоном `1..200`, чтобы случайный запрос не вытащил слишком большой объем истории."#;

const ADMIN_GENERATE_DESCRIPTION: &str = r#"Admin-only smoke-test генерации.

Метод использует тот же внутренний generation contract, что и `POST /api/v1/generate`, но дополнительно принимает `api_key` для разовой проверки provider route без изменения env и БД.

Если `api_key` не передан, gateway читает секрет из env-переменной, имя которой лежит в `providers.api_key_env`. Переданный `api_key` не сохраняется в PostgreSQL; в history попадают только обычные metadata и preview."#;

const GET_REQUEST_DETAILS_DESCRIPTION: &str = r#"Возвращает детальную запись одного LLM-запроса.

Метод нужен для диагностики конкретного request id: кроме полей из списка он отдает сохраненные `prompt_preview` и `response_preview`. Это именно preview, а не полный audit trail payloads."#;

const LIST_PROVIDERS_DESCRIPTION: &str = r#"Возвращает provider-конфиги из PostgreSQL.

Метод показывает routing-relevant конфигурацию: `code`, `kind`, `base_url`, `api_key_env`, enabled flag и timeout. Секреты не возвращаются и не хранятся в БД: `api_key_env` - только имя env-переменной."#;

const LIST_MODEL_ROUTES_DESCRIPTION: &str = r#"Возвращает model alias routes.

Каждая запись показывает, какой внутренний alias принимает gateway, к какому provider code он привязан и какой provider-specific model id будет передан adapter. Клиенты должны зависеть от `alias`, а не от `external_model`."#;

#[utoipa::path(
    get,
    path = "/api/admin/requests",
    tag = "admin",
    operation_id = "listRequests",
    summary = "Получить последние запросы",
    description = LIST_REQUESTS_DESCRIPTION,
    params(
        ("limit" = Option<i64>, Query, description = "Максимальное количество записей. Значение ограничивается диапазоном 1..200; по умолчанию 100.", example = 100)
    ),
    responses(
        (
            status = 200,
            description = "Список последних запросов, отсортированный repository по актуальности.",
            body = [RequestListItemDto],
            example = json!([
                {
                    "id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
                    "created_at": "2026-04-17T18:20:00Z",
                    "model_alias": "smart-default",
                    "provider_code": "openai",
                    "external_model": "gpt-4.1-mini",
                    "status": "success",
                    "latency_ms": 1824,
                    "error_message": null,
                    "input_tokens": 1200,
                    "output_tokens": 350
                }
            ])
        ),
        (
            status = 500,
            description = "Не удалось прочитать историю запросов из хранилища.",
            body = crate::domain::ErrorResponseDto,
            example = json!({
                "request_id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
                "error": {
                    "code": "storage_error",
                    "message": "Не удалось прочитать данные из хранилища"
                }
            })
        ),
    )
)]
pub async fn list_requests(
    State(state): State<AppState>,
    Query(query): Query<RequestsQuery>,
) -> Result<Json<Vec<RequestListItemDto>>, AppHttpError> {
    let limit = query.limit.clamp(1, 200);
    let items = state
        .requests_repo
        .list_requests(limit)
        .await?
        .into_iter()
        .map(RequestListItemDto::from)
        .collect();

    Ok(Json(items))
}

#[utoipa::path(
    post,
    path = "/api/admin/generate",
    tag = "admin",
    operation_id = "adminGenerateText",
    summary = "Проверить генерацию из admin API",
    description = ADMIN_GENERATE_DESCRIPTION,
    request_body(
        content = AdminGenerateRequestDto,
        description = "Generation request plus optional one-time `api_key` override. `api_key` is write-only and is never persisted.",
        content_type = "application/json",
        example = json!({
            "model": "smart-default",
            "api_key": "sk-test-placeholder",
            "messages": [
                { "role": "user", "content": "Проверь маршрут и ответь одним предложением." }
            ],
            "options": {
                "temperature": 0.1,
                "max_tokens": 200
            }
        })
    ),
    responses(
        (
            status = 200,
            description = "Smoke-test выполнен через тот же GenerateService и тот же provider routing, что и публичный generation endpoint.",
            body = GenerateResponseDto,
            example = json!({
                "id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
                "model": "smart-default",
                "provider": "openai",
                "output_text": "Маршрут работает.",
                "finish_reason": "stop",
                "usage": {
                    "input_tokens": 20,
                    "output_tokens": 5
                }
            })
        ),
        (
            status = 400,
            description = "Пустой `model` или пустой `messages`.",
            body = crate::domain::ErrorResponseDto,
            example = json!({
                "request_id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
                "error": {
                    "code": "invalid_request",
                    "message": "Поля `model` и `messages` обязательны"
                }
            })
        ),
        (
            status = 404,
            description = "Alias модели не найден в `model_routes` или route отключен.",
            body = crate::domain::ErrorResponseDto,
            example = json!({
                "request_id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
                "error": {
                    "code": "route_not_found",
                    "message": "Alias `smart-default` не найден или выключен"
                }
            })
        ),
        (
            status = 500,
            description = "Ошибка конфигурации gateway или storage failure.",
            body = crate::domain::ErrorResponseDto,
            example = json!({
                "request_id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
                "error": {
                    "code": "gateway_misconfigured",
                    "message": "Gateway не умеет работать с provider kind `anthropic`"
                }
            })
        ),
        (
            status = 502,
            description = "Provider error, invalid provider response или transport error.",
            body = crate::domain::ErrorResponseDto,
            example = json!({
                "request_id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
                "error": {
                    "code": "provider_error",
                    "message": "Провайдер вернул ошибку: invalid model"
                }
            })
        ),
        (
            status = 504,
            description = "Provider не ответил за configured timeout.",
            body = crate::domain::ErrorResponseDto,
            example = json!({
                "request_id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
                "error": {
                    "code": "provider_timeout",
                    "message": "Провайдер не ответил вовремя"
                }
            })
        ),
    )
)]
pub async fn handle_generate(
    State(state): State<AppState>,
    Json(payload): Json<AdminGenerateRequestDto>,
) -> Result<Json<GenerateResponseDto>, AppHttpError> {
    let response = state
        .generate_service
        .generate_with_api_key_override(payload.request, payload.api_key)
        .await?;

    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/admin/requests/{id}",
    tag = "admin",
    operation_id = "getRequestDetails",
    summary = "Получить детали запроса",
    description = GET_REQUEST_DETAILS_DESCRIPTION,
    params(
        ("id" = String, Path, description = "Gateway request id из generation response или списка `/api/admin/requests`.", example = "req_4d4d6f4df77f4c1e8260f60f052f63cc")
    ),
    responses(
        (
            status = 200,
            description = "Детальная запись запроса с prompt/response preview.",
            body = RequestDetailsDto,
            example = json!({
                "id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
                "created_at": "2026-04-17T18:20:00Z",
                "model_alias": "smart-default",
                "provider_code": "openai",
                "external_model": "gpt-4.1-mini",
                "status": "success",
                "latency_ms": 1824,
                "error_message": null,
                "input_tokens": 1200,
                "output_tokens": 350,
                "prompt_preview": "system: Ты помогаешь CMS.\nuser: Коротко опиши структуру меню.",
                "response_preview": "Структура меню такая: ..."
            })
        ),
        (
            status = 404,
            description = "Запрос с таким id не найден в истории.",
            body = crate::domain::ErrorResponseDto,
            example = json!({
                "request_id": "req_9f84f09d2f9b4f7eb53b01923f21fd4f",
                "error": {
                    "code": "not_found",
                    "message": "Запрос `req_missing` не найден"
                }
            })
        ),
        (
            status = 500,
            description = "Не удалось прочитать запись запроса из хранилища.",
            body = crate::domain::ErrorResponseDto,
            example = json!({
                "request_id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
                "error": {
                    "code": "storage_error",
                    "message": "Не удалось прочитать данные из хранилища"
                }
            })
        ),
    )
)]
pub async fn get_request_details(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RequestDetailsDto>, AppHttpError> {
    let item = state.requests_repo.get_request(&id).await?;

    match item {
        Some(item) => Ok(Json(item.into())),
        None => Err(AppHttpError::not_found(format!("Запрос `{id}` не найден"))),
    }
}

#[utoipa::path(
    get,
    path = "/api/admin/providers",
    tag = "admin",
    operation_id = "listProviders",
    summary = "Получить provider-конфиги",
    description = LIST_PROVIDERS_DESCRIPTION,
    responses(
        (
            status = 200,
            description = "Список providers без секретов. API keys представлены только именами env-переменных.",
            body = [ProviderDto],
            example = json!([
                {
                    "code": "openai",
                    "kind": "openai-compatible",
                    "base_url": "https://api.openai.com/v1",
                    "api_key_env": "OPENAI_API_KEY",
                    "is_enabled": true,
                    "timeout_ms": 30000,
                    "updated_at": "2026-04-17T18:20:00Z"
                }
            ])
        ),
        (
            status = 500,
            description = "Не удалось прочитать provider-конфиги из хранилища.",
            body = crate::domain::ErrorResponseDto,
            example = json!({
                "request_id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
                "error": {
                    "code": "storage_error",
                    "message": "Не удалось прочитать данные из хранилища"
                }
            })
        ),
    )
)]
pub async fn list_providers(
    State(state): State<AppState>,
) -> Result<Json<Vec<ProviderDto>>, AppHttpError> {
    let items = state
        .routes_repo
        .list_providers()
        .await?
        .into_iter()
        .map(ProviderDto::from)
        .collect();

    Ok(Json(items))
}

#[utoipa::path(
    get,
    path = "/api/admin/model-routes",
    tag = "admin",
    operation_id = "listModelRoutes",
    summary = "Получить маршруты model alias",
    description = LIST_MODEL_ROUTES_DESCRIPTION,
    responses(
        (
            status = 200,
            description = "Список alias routes, используемых `POST /api/v1/generate` для выбора provider и external model.",
            body = [ModelRouteDto],
            example = json!([
                {
                    "alias": "smart-default",
                    "provider_code": "openai",
                    "external_model": "gpt-4.1-mini",
                    "is_enabled": true,
                    "updated_at": "2026-04-17T18:20:00Z"
                }
            ])
        ),
        (
            status = 500,
            description = "Не удалось прочитать model routes из хранилища.",
            body = crate::domain::ErrorResponseDto,
            example = json!({
                "request_id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
                "error": {
                    "code": "storage_error",
                    "message": "Не удалось прочитать данные из хранилища"
                }
            })
        ),
    )
)]
pub async fn list_model_routes(
    State(state): State<AppState>,
) -> Result<Json<Vec<ModelRouteDto>>, AppHttpError> {
    let items = state
        .routes_repo
        .list_model_routes()
        .await?
        .into_iter()
        .map(ModelRouteDto::from)
        .collect();

    Ok(Json(items))
}

fn default_limit() -> i64 {
    100
}
