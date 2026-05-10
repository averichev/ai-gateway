use axum::{Json, extract::State};

use crate::{
    app::{AppHttpError, AppState},
    domain::{GenerateRequestDto, GenerateResponseDto},
};

#[utoipa::path(
    post,
    path = "/api/v1/generate",
    tag = "generation",
    request_body = GenerateRequestDto,
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
    Json(payload): Json<GenerateRequestDto>,
) -> Result<Json<GenerateResponseDto>, AppHttpError> {
    let response = state.generate_service.generate(payload).await?;

    Ok(Json(response))
}
