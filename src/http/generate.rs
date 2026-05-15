use axum::{Json, extract::State, http::HeaderMap};

use crate::{
    app::{AppHttpError, AppState},
    domain::{GenerateRequestDto, GenerateResponseDto},
    http::auth::bearer_token,
    security::token_hash,
};

const GENERATE_DESCRIPTION: &str = r#"Основной клиентский endpoint gateway.

Метод принимает внутренний alias модели и список сообщений, затем определяет tenant по `Authorization: Bearer <gateway-client-token>` и выбирает provider через tenant-local таблицы `model_routes` и `providers`. Клиент не передает внешний model id, provider URL или provider API key.

Контракт специально остается provider-agnostic: OpenAI-compatible, Anthropic-native и другие внешние форматы должны жить внутри отдельных `ProviderAdapter`, а не в этом request/response.

При любом исходе gateway пытается записать запрос в историю. В успешном ответе возвращается нормализованный текст, provider code и usage, если upstream его сообщил."#;

#[utoipa::path(
    post,
    path = "/api/v1/generate",
    tag = "generation",
    operation_id = "generateText",
    summary = "Выполнить генерацию через model alias",
    description = GENERATE_DESCRIPTION,
    request_body(
        content = GenerateRequestDto,
        description = "Внутренний generation request. `model` - alias из `model_routes`; `messages` - непустой список сообщений; `options` - необязательные параметры генерации.",
        content_type = "application/json",
        example = json!({
            "model": "smart-default",
            "messages": [
                { "role": "system", "content": "Ты помогаешь CMS." },
                { "role": "user", "content": "Коротко опиши структуру меню." }
            ],
            "options": {
                "temperature": 0.2,
                "max_tokens": 1000
            }
        })
    ),
    responses(
        (
            status = 200,
            description = "Генерация выполнена. Ответ содержит request id, исходный alias модели, выбранный provider code, текст и usage при наличии.",
            body = GenerateResponseDto,
            example = json!({
                "id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
                "model": "smart-default",
                "provider": "openai",
                "output_text": "Структура меню такая: ...",
                "finish_reason": "stop",
                "usage": {
                    "input_tokens": 120,
                    "output_tokens": 80
                }
            })
        ),
        (
            status = 400,
            description = "Внутренняя бизнес-валидация не пройдена: пустой `model` или пустой `messages`.",
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
            description = "Alias модели не найден в `model_routes` или соответствующий route отключен.",
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
            description = "Ошибка конфигурации gateway или хранилища: не удалось прочитать routes/providers, отсутствует encrypted provider secret, либо provider kind не зарегистрирован.",
            body = crate::domain::ErrorResponseDto,
            example = json!({
                "request_id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
                "error": {
                    "code": "gateway_misconfigured",
                    "message": "Gateway не настроен: отсутствует provider secret для `openai`"
                }
            })
        ),
        (
            status = 502,
            description = "Upstream provider вернул ошибку, недоступен на транспортном уровне или вернул некорректный response для adapter.",
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
            description = "Provider не ответил за `providers.timeout_ms`.",
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
    headers: HeaderMap,
    Json(payload): Json<GenerateRequestDto>,
) -> Result<Json<GenerateResponseDto>, AppHttpError> {
    let token = bearer_token(&headers)?;
    let client = state
        .auth_repo
        .authenticate_gateway_client(&token_hash(&token))
        .await?
        .ok_or_else(|| AppHttpError::unauthorized("Неизвестный gateway client token"))?;

    if !client.is_enabled {
        return Err(AppHttpError::forbidden("Gateway client token отключен"));
    }

    let response = state
        .generate_service
        .generate_for_client(payload, client.tenant_id, client.id)
        .await?;

    Ok(Json(response))
}
