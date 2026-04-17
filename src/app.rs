use std::{path::Path, sync::Arc};

use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, get_service, post},
};
use serde_json::json;
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use uuid::Uuid;

use crate::{
    domain::ErrorResponseDto,
    http::{admin, generate},
    repositories::{PostgresRequestsRepository, PostgresRoutesRepository, RepositoryError},
    usecases::generate::{GenerateService, ServiceError},
};

#[derive(Clone)]
pub struct AppState {
    pub generate_service: Arc<GenerateService>,
    pub routes_repo: Arc<PostgresRoutesRepository>,
    pub requests_repo: Arc<PostgresRequestsRepository>,
    pub admin_dist_dir: String,
}

pub fn build_app(state: AppState) -> Router {
    let admin_dist_dir = state.admin_dist_dir.clone();

    let router = Router::new()
        .route("/health", get(healthcheck))
        .route("/api/v1/generate", post(generate::handle_generate))
        .route("/api/admin/requests", get(admin::list_requests))
        .route("/api/admin/requests/{id}", get(admin::get_request_details))
        .route("/api/admin/providers", get(admin::list_providers))
        .route("/api/admin/model-routes", get(admin::list_model_routes))
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

async fn healthcheck() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok"
    }))
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
