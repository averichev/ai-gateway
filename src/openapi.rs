use utoipa::OpenApi;

use crate::{
    domain::{
        ErrorBodyDto, ErrorResponseDto, FinishReason, GenerateMessageDto, GenerateOptionsDto,
        GenerateRequestDto, GenerateResponseDto, HealthResponseDto, NamedToolChoiceDto,
        TokenUsageDto, ToolCallDto, ToolChoiceDto, ToolChoicePolicy, ToolDefinitionDto,
    },
    http::generate,
};

const API_DESCRIPTION: &str = r#"AI Gateway - внутренний HTTP gateway для LLM-вызовов.

Цель проекта - дать клиентским приложениям один стабильный URL и один внутренний provider-agnostic контракт, а различия внешних AI API скрыть внутри gateway.

Основной поток:

1. Клиент вызывает `POST /api/v1/generate`.
2. В запросе указывает внутренний alias модели, например `smart-default`.
3. Gateway находит alias в `model_routes`.
4. Через `provider_code` получает provider-конфиг из `providers`.
5. По `providers.kind` выбирает `ProviderAdapter`.
6. Adapter вызывает внешний AI API.
7. Gateway определяет tenant по machine token и сохраняет запись в истории `requests`.
8. Клиент получает нормализованный response.

Что решает gateway:

- не размазывает API keys и provider-specific настройки по приложениям;
- позволяет переключать реальные модели через alias без изменений в клиентах;
- хранит историю LLM-вызовов и ошибок в одном месте;
- дает admin API и admin UI для диагностики providers, model routes и запросов;
- позволяет добавлять новых providers через отдельные adapters без изменения core API.

Граница текущего MVP:

- публичный клиентский контракт - `POST /api/v1/generate`;
- текущий реализованный provider kind - `openai-compatible`;
- сервис не является прозрачной заменой OpenAI, Anthropic или Claude SDK;
- Tool Calling передаёт определения, вызовы и результаты инструментов; gateway их не выполняет;
- vendor-compatible endpoints вроде `/v1/chat/completions`, streaming, embeddings и quotas пока не реализованы;
- provider secrets хранятся encrypted-at-rest в `provider_secrets`, master key приходит из `GATEWAY_MASTER_KEY`.
"#;

#[derive(OpenApi)]
#[openapi(
    paths(
        generate::handle_generate,
        crate::app::healthcheck,
    ),
    components(
        schemas(
            ErrorBodyDto,
            ErrorResponseDto,
            GenerateMessageDto,
            GenerateOptionsDto,
            GenerateRequestDto,
            GenerateResponseDto,
            HealthResponseDto,
            ToolDefinitionDto,
            ToolCallDto,
            ToolChoiceDto,
            ToolChoicePolicy,
            NamedToolChoiceDto,
            FinishReason,
            TokenUsageDto,
        )
    ),
    tags(
        (name = "health", description = "Проверка доступности HTTP-сервиса."),
        (name = "generation", description = "Основной provider-agnostic API для клиентских приложений."),
    ),
    info(
        title = "AI Gateway API",
        version = "0.1.0",
        description = API_DESCRIPTION
    )
)]
pub struct ApiDoc;

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use utoipa::OpenApi;

    use super::ApiDoc;

    #[test]
    fn exposes_expected_paths() {
        let doc = serde_json::to_value(ApiDoc::openapi()).expect("openapi should serialize");
        let paths = doc
            .get("paths")
            .and_then(Value::as_object)
            .expect("openapi should contain paths");

        for path in ["/health", "/api/v1/generate"] {
            assert!(paths.contains_key(path), "missing OpenAPI path: {path}");
        }
    }

    #[test]
    fn documents_project_and_operations() {
        let doc = serde_json::to_value(ApiDoc::openapi()).expect("openapi should serialize");

        let info_description = doc
            .pointer("/info/description")
            .and_then(Value::as_str)
            .expect("OpenAPI info should contain project description");
        assert!(
            info_description.contains("provider-agnostic"),
            "project description should explain gateway boundaries"
        );

        let paths = doc
            .get("paths")
            .and_then(Value::as_object)
            .expect("openapi should contain paths");

        for (path, method) in [("/health", "get"), ("/api/v1/generate", "post")] {
            let operation = paths
                .get(path)
                .and_then(|path_item| path_item.get(method))
                .unwrap_or_else(|| panic!("missing operation {method} {path}"));

            for field in ["operationId", "summary", "description"] {
                let value = operation
                    .get(field)
                    .and_then(Value::as_str)
                    .unwrap_or_else(|| panic!("missing `{field}` for {method} {path}"));
                assert!(
                    !value.trim().is_empty(),
                    "`{field}` should not be empty for {method} {path}"
                );
            }
        }
    }

    #[test]
    fn documents_valid_tool_cycle_and_string_output_text() {
        let doc = serde_json::to_value(ApiDoc::openapi()).unwrap();
        let examples = doc
            .pointer(
                "/paths/~1api~1v1~1generate/post/requestBody/content/application~1json/examples",
            )
            .and_then(Value::as_object)
            .unwrap();
        for name in ["Text", "ToolDefinition", "ToolResult"] {
            let request: crate::domain::GenerateRequestDto =
                serde_json::from_value(examples[name]["value"].clone()).unwrap();
            request.validate().unwrap();
        }
        let examples = doc
            .pointer(
                "/paths/~1api~1v1~1generate/post/responses/200/content/application~1json/examples",
            )
            .and_then(Value::as_object)
            .unwrap();
        assert_eq!(examples["ToolCall"]["value"]["output_text"], "");
        assert!(examples["FinalAnswer"]["value"]["output_text"].is_string());
        let call: crate::domain::ToolCallDto =
            serde_json::from_value(examples["ToolCall"]["value"]["tool_calls"][0].clone()).unwrap();
        assert_eq!(call.id, "call_weather_1");
        let response = &doc["components"]["schemas"]["GenerateResponseDto"];
        assert_eq!(response["properties"]["output_text"]["type"], "string");
        assert!(
            response["required"]
                .as_array()
                .unwrap()
                .contains(&Value::String("output_text".into()))
        );
    }
}
