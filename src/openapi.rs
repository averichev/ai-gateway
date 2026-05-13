use utoipa::OpenApi;

use crate::{
    domain::{
        AdminGenerateRequestDto, ErrorBodyDto, ErrorResponseDto, GenerateMessageDto,
        GenerateOptionsDto, GenerateRequestDto, GenerateResponseDto, HealthResponseDto,
        MessageRole, ModelRouteDto, ProviderDto, RequestDetailsDto, RequestListItemDto,
        TokenUsageDto,
    },
    http::{admin, generate},
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
7. Gateway сохраняет запись в истории `requests`.
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
- vendor-compatible endpoints вроде `/v1/chat/completions`, streaming, embeddings, tools/function calling и auth/quotas пока не реализованы;
- secrets не хранятся в PostgreSQL: в `providers.api_key_env` хранится только имя env-переменной с ключом.
"#;

#[derive(OpenApi)]
#[openapi(
    paths(
        generate::handle_generate,
        admin::handle_generate,
        admin::list_requests,
        admin::get_request_details,
        admin::list_providers,
        admin::list_model_routes,
        crate::app::healthcheck,
    ),
    components(
        schemas(
            AdminGenerateRequestDto,
            ErrorBodyDto,
            ErrorResponseDto,
            GenerateMessageDto,
            GenerateOptionsDto,
            GenerateRequestDto,
            GenerateResponseDto,
            HealthResponseDto,
            MessageRole,
            ModelRouteDto,
            ProviderDto,
            RequestDetailsDto,
            RequestListItemDto,
            TokenUsageDto,
        )
    ),
    tags(
        (name = "health", description = "Проверка доступности HTTP-сервиса."),
        (name = "generation", description = "Основной provider-agnostic API для клиентских приложений."),
        (name = "admin", description = "Admin API для диагностики providers, model routes и истории запросов."),
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

        for path in [
            "/health",
            "/api/v1/generate",
            "/api/admin/generate",
            "/api/admin/requests",
            "/api/admin/requests/{id}",
            "/api/admin/providers",
            "/api/admin/model-routes",
        ] {
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

        for (path, method) in [
            ("/health", "get"),
            ("/api/v1/generate", "post"),
            ("/api/admin/generate", "post"),
            ("/api/admin/requests", "get"),
            ("/api/admin/requests/{id}", "get"),
            ("/api/admin/providers", "get"),
            ("/api/admin/model-routes", "get"),
        ] {
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
}
