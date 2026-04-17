use axum::{Json, extract::State};

use crate::{
    app::{AppHttpError, AppState},
    domain::{GenerateRequestDto, GenerateResponseDto},
};

pub async fn handle_generate(
    State(state): State<AppState>,
    Json(payload): Json<GenerateRequestDto>,
) -> Result<Json<GenerateResponseDto>, AppHttpError> {
    let response = state.generate_service.generate(payload).await?;

    Ok(Json(response))
}
