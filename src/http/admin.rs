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
