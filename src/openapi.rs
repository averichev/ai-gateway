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
        (name = "health", description = "Service health"),
        (name = "generation", description = "Public generation API"),
        (name = "admin", description = "Admin API"),
    ),
    info(
        title = "AI Gateway API",
        version = "0.1.0",
        description = "Internal model-routing gateway API"
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
}
