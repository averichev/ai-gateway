use std::{path::Path, sync::Arc};

use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, get_service, post},
};
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;

use crate::{
    domain::{ErrorBodyDto, ErrorResponseDto, HealthResponseDto},
    http::{admin, auth, generate},
    openapi::ApiDoc,
    repositories::{
        PostgresAuthRepository, PostgresRequestsRepository, PostgresRoutesRepository,
        RepositoryError,
    },
    security::SecretCrypto,
    usecases::generate::{GenerateService, ServiceError},
};

const HEALTHCHECK_DESCRIPTION: &str = r#"Проверяет, что HTTP-сервис запущен и способен ответить.

Это shallow healthcheck текущего процесса. Он не проверяет PostgreSQL, provider routes, наличие API keys и доступность upstream AI providers."#;

#[derive(Clone)]
pub struct AppState {
    pub generate_service: Arc<GenerateService>,
    pub auth_repo: Arc<PostgresAuthRepository>,
    pub routes_repo: Arc<PostgresRoutesRepository>,
    pub requests_repo: Arc<PostgresRequestsRepository>,
    pub secret_crypto: SecretCrypto,
    pub admin_dist_dir: String,
}

pub fn build_app(state: AppState) -> Router {
    let admin_dist_dir = state.admin_dist_dir.clone();

    let router = Router::new()
        .route("/health", get(healthcheck))
        .route("/api/auth/bootstrap", get(auth::bootstrap_status))
        .route("/api/auth/register", post(auth::bootstrap_register))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/me", get(auth::me))
        .route("/api/v1/generate", post(generate::handle_generate))
        .route("/api/admin/tenants", get(admin::list_tenants))
        .route("/api/admin/tenants", post(admin::create_tenant))
        .route("/api/admin/users", get(admin::list_users))
        .route("/api/admin/users", post(admin::create_user))
        .route(
            "/api/admin/tenants/{tenant_id}/generate",
            post(admin::handle_generate),
        )
        .route(
            "/api/admin/tenants/{tenant_id}/requests",
            get(admin::list_requests),
        )
        .route(
            "/api/admin/tenants/{tenant_id}/requests/{id}",
            get(admin::get_request_details),
        )
        .route(
            "/api/admin/tenants/{tenant_id}/providers",
            get(admin::list_providers),
        )
        .route(
            "/api/admin/tenants/{tenant_id}/providers",
            post(admin::save_provider),
        )
        .route(
            "/api/admin/tenants/{tenant_id}/providers/{provider_id}/secret",
            post(admin::save_provider_secret),
        )
        .route(
            "/api/admin/tenants/{tenant_id}/model-routes",
            get(admin::list_model_routes),
        )
        .route(
            "/api/admin/tenants/{tenant_id}/model-routes",
            post(admin::save_model_route),
        )
        .route(
            "/api/admin/tenants/{tenant_id}/gateway-clients",
            get(admin::list_gateway_clients),
        )
        .route(
            "/api/admin/tenants/{tenant_id}/gateway-clients",
            post(admin::create_gateway_client),
        )
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    if Path::new(&admin_dist_dir).exists() {
        router.fallback_service(get_service(
            ServeDir::new(&admin_dist_dir)
                .not_found_service(ServeFile::new(format!("{admin_dist_dir}/index.html"))),
        ))
    } else {
        router
    }
}

#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    operation_id = "healthcheck",
    summary = "Проверить доступность HTTP-сервиса",
    description = HEALTHCHECK_DESCRIPTION,
    responses(
        (
            status = 200,
            description = "HTTP-сервис отвечает.",
            body = HealthResponseDto,
            example = json!({
                "status": "ok"
            })
        )
    )
)]
pub async fn healthcheck() -> Json<HealthResponseDto> {
    Json(HealthResponseDto {
        status: "ok".to_owned(),
    })
}

#[derive(Debug)]
pub struct AppHttpError {
    status_code: StatusCode,
    body: ErrorResponseDto,
}

impl AppHttpError {
    pub fn new(status_code: StatusCode, body: ErrorResponseDto) -> Self {
        Self { status_code, body }
    }

    pub fn body(code: impl Into<String>, message: impl Into<String>) -> ErrorResponseDto {
        ErrorResponseDto {
            request_id: format!("req_{}", Uuid::new_v4().simple()),
            error: ErrorBodyDto {
                code: code.into(),
                message: message.into(),
            },
        }
    }

    pub fn bad_request(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, Self::body(code, message))
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            Self::body("unauthorized", message),
        )
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, Self::body("forbidden", message))
    }

    pub fn internal(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, Self::body(code, message))
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        let service_error = ServiceError::new(
            format!("req_{}", Uuid::new_v4().simple()),
            StatusCode::NOT_FOUND,
            "not_found",
            message.into(),
        );

        service_error.into()
    }
}

impl From<ServiceError> for AppHttpError {
    fn from(value: ServiceError) -> Self {
        Self::new(value.status_code, value.to_error_response())
    }
}

impl From<RepositoryError> for AppHttpError {
    fn from(value: RepositoryError) -> Self {
        ServiceError::from(value).into()
    }
}

impl IntoResponse for AppHttpError {
    fn into_response(self) -> Response {
        (self.status_code, Json(self.body)).into_response()
    }
}
