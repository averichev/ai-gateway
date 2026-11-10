use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::HeaderMap,
};

use crate::{
    app::{AppHttpError, AppState},
    domain::{GenerateRequestDto, GenerateResponseDto},
    http::auth::bearer_token,
    security::token_hash,
};

const GENERATE_DESCRIPTION: &str = r#"Основной клиентский endpoint gateway.

Метод принимает внутренний alias модели и список сообщений, затем определяет tenant по `Authorization: Bearer <gateway-client-token>` и выбирает provider через tenant-local таблицы `model_routes` и `providers`. Клиент не передает внешний model id, provider URL или provider API key.

Контракт специально остается provider-agnostic: OpenAI-compatible, Anthropic-native и другие внешние форматы должны жить внутри отдельных `ProviderAdapter`, а не в этом request/response.

При любом исходе gateway пытается записать запрос в историю. В успешном ответе возвращаются `output_text` (строка, в том числе пустая), структурированные `tool_calls`, нормализованный `finish_reason` и usage. Gateway не выполняет инструменты: клиент отправляет следующий запрос с assistant tool_calls и tool results. Примеры полного цикла: docs/rest-api.md.

Лимиты: 2 MiB JSON, 256 сообщений, 128 инструментов / вызовов за completion, 64 KiB на JSON Schema / аргументы. Имена инструментов: [a-zA-Z0-9_-], 1..64 символа. Аргументы — JSON objects. JSON Schema проверяется без доступа к сети и файлам. Preview для запросов с инструментами не сохраняется."#;

#[utoipa::path(
    post,
    path = "/api/v1/generate",
    tag = "generation",
    operation_id = "generateText",
    summary = "Выполнить генерацию через model alias",
    description = GENERATE_DESCRIPTION,
    request_body(
        content = GenerateRequestDto,
        description = "Внутренний generation request. `model` — alias; `messages` — system/user/assistant/tool; `tools` и `tool_choice` необязательны. tool_choice: auto, none, required или {name: tool_name}.",
        content_type = "application/json",
        examples(
            ("Text" = (value = json!({
                "model": "smart-default",
                "messages": [{"role":"user","content":"Привет"}]
            }))),
            ("ToolDefinition" = (summary = "1. Объявить инструмент", value = json!({
                "model":"smart-default",
                "messages":[{"role":"user","content":"Какая погода в Москве?"}],
                "tools":[{"name":"weather","description":"Текущая погода","parameters":{
                    "type":"object","properties":{"city":{"type":"string"}},"required":["city"]
                }}],
                "tool_choice":"auto"
            }))),
            ("ToolResult" = (summary = "3. Передать результат исполнения из Factum", value = json!({
                "model":"smart-default",
                "messages":[
                    {"role":"user","content":"Какая погода в Москве?"},
                    {"role":"assistant","tool_calls":[{"id":"call_weather_1","name":"weather","arguments":{"city":"Москва"}}]},
                    {"role":"tool","tool_call_id":"call_weather_1","content":"+12 °C, солнечно"}
                ],
                "tool_choice":"none"
            })))
        )
    ),
    responses(
        (
            status = 200,
            description = "Генерация выполнена: текст и/или tool_calls. При ответе только с вызовами output_text равен пустой строке, finish_reason — tool_calls. Пустой список tool_calls опускается.",
            body = GenerateResponseDto,
            examples(
                ("ToolCall" = (summary = "2. Запрос модели на вызов инструмента", value = json!({
                    "id":"req_first","model":"smart-default","provider":"openai","output_text":"",
                    "tool_calls":[{"id":"call_weather_1","name":"weather","arguments":{"city":"Москва"}}],
                    "finish_reason":"tool_calls","usage":{"input_tokens":80,"output_tokens":20}
                }))),
                ("FinalAnswer" = (summary = "4. Обычный текст после Tool Calling", value = json!({
                    "id":"req_second","model":"smart-default","provider":"openai",
                    "output_text":"В Москве солнечно, +12 °C.","finish_reason":"stop",
                    "usage":{"input_tokens":120,"output_tokens":15}
                })))
            )
        ),
        (
            status = 400,
            description = "invalid_request: неверный JSON, схема, имя, tool_choice или история вызовов. tool_calling_not_supported: upstream явно отклонил поддержку tools / tool_choice для выбранной модели.",
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
            status = 401,
            description = "Отсутствующий или неизвестный gateway machine token.",
            body = crate::domain::ErrorResponseDto,
        ),
        (
            status = 413,
            description = "Тело запроса превышает 2 MiB.",
            body = crate::domain::ErrorResponseDto,
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
                    "message": "Провайдер вернул ошибку: HTTP 400 from upstream"
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
    payload: Result<Json<GenerateRequestDto>, JsonRejection>,
) -> Result<Json<GenerateResponseDto>, AppHttpError> {
    let Json(payload) = payload.map_err(generation_json_error)?;
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

pub fn generation_json_error(error: JsonRejection) -> AppHttpError {
    let status = if error.status() == axum::http::StatusCode::PAYLOAD_TOO_LARGE {
        axum::http::StatusCode::PAYLOAD_TOO_LARGE
    } else {
        axum::http::StatusCode::BAD_REQUEST
    };
    // Deserialize errors can include argument values; never expose them.
    AppHttpError::new(
        status,
        AppHttpError::body(
            "invalid_request",
            "Некорректный JSON запроса или превышен лимит 2 MiB",
        ),
    )
}
