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

#[utoipa::path(
    get,
    path = "/api/admin/requests",
    tag = "admin",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum number of requests to return, clamped to 1..200")
    ),
    responses(
        (status = 200, description = "Recent requests", body = [RequestListItemDto]),
        (status = 500, description = "Storage error", body = crate::domain::ErrorResponseDto),
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
    request_body = AdminGenerateRequestDto,
    responses(
        (status = 200, description = "Generation completed", body = GenerateResponseDto),
        (status = 400, description = "Invalid request", body = crate::domain::ErrorResponseDto),
        (status = 404, description = "Model route was not found", body = crate::domain::ErrorResponseDto),
        (status = 500, description = "Gateway misconfiguration or storage error", body = crate::domain::ErrorResponseDto),
        (status = 502, description = "Provider error", body = crate::domain::ErrorResponseDto),
        (status = 504, description = "Provider timeout", body = crate::domain::ErrorResponseDto),
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
    params(
        ("id" = String, Path, description = "Request id")
    ),
    responses(
        (status = 200, description = "Request details", body = RequestDetailsDto),
        (status = 404, description = "Request was not found", body = crate::domain::ErrorResponseDto),
        (status = 500, description = "Storage error", body = crate::domain::ErrorResponseDto),
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
    responses(
        (status = 200, description = "Provider configurations", body = [ProviderDto]),
        (status = 500, description = "Storage error", body = crate::domain::ErrorResponseDto),
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
    responses(
        (status = 200, description = "Model alias routes", body = [ModelRouteDto]),
        (status = 500, description = "Storage error", body = crate::domain::ErrorResponseDto),
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
