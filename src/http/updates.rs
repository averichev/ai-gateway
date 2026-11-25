use std::net::SocketAddr;

use axum::{
    Json, Router,
    extract::{ConnectInfo, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use standalone_updater::{OperationStatus, UpdateError};

use crate::{
    app::{AppHttpError, AppState},
    http::auth::require_admin,
    updates::{Summary, VERSION},
};

async fn owner_only(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AppHttpError> {
    let principal = require_admin(&state, request.headers()).await?;
    if principal.global_role != "owner" {
        return Err(AppHttpError::forbidden(
            "Управлять обновлениями может только владелец шлюза",
        ));
    }
    // Browser calls use the same-origin admin UI and an explicit bearer token.
    if let Some(site) = request.headers().get("sec-fetch-site")
        && site != "same-origin"
        && site != "none"
    {
        return Err(AppHttpError::forbidden(
            "Требуется запрос из админки этого шлюза",
        ));
    }
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}

pub(crate) fn routes(state: AppState) -> Router<AppState> {
    #[cfg(not(target_os = "linux"))]
    use self::{unsupported as cancel, unsupported as install, unsupported as prepare};
    #[cfg(target_os = "linux")]
    use linux::{cancel, install, prepare};
    Router::new()
        .route("/api/admin/updates", get(summary))
        .route("/api/admin/updates/check", post(check))
        .route("/api/admin/updates/prepare", post(prepare))
        .route("/api/admin/updates/install", post(install))
        .route("/api/admin/updates/cancel", post(cancel))
        .route_layer(middleware::from_fn_with_state(state, owner_only))
}

pub(crate) async fn readiness(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> Response {
    if !peer.ip().is_loopback() {
        return StatusCode::FORBIDDEN.into_response();
    }
    if state.auth_repo.has_users().await.is_err() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    #[cfg(target_os = "linux")]
    if let crate::updates::Updates::Managed(manager) = &state.updates {
        return Json(manager.ready_response()).into_response();
    }
    Json(serde_json::json!({"status": "ok", "version": VERSION})).into_response()
}

async fn summary(State(state): State<AppState>) -> Result<Json<Summary>, AppHttpError> {
    Ok(Json(state.updates.summary()?))
}

type Accepted = Result<(StatusCode, Json<OperationStatus>), AppHttpError>;

async fn check(State(state): State<AppState>) -> Accepted {
    Ok((StatusCode::ACCEPTED, Json(state.updates.check()?)))
}

#[cfg(not(target_os = "linux"))]
async fn unsupported() -> AppHttpError {
    UpdateError::Unsupported("установка доступна для отдельного Linux-сервера").into()
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use crate::http::generate::generation_json_error;
    use crate::updates::Updates;
    use axum::extract::rejection::JsonRejection;
    use serde::Deserialize;
    use standalone_updater::runtime::{InstallRequest, UpdateManager};

    fn manager(state: &AppState) -> Result<&UpdateManager, UpdateError> {
        match &state.updates {
            Updates::Managed(manager) => Ok(manager),
            _ => Err(UpdateError::Unsupported(
                "установкой управляет система развёртывания",
            )),
        }
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(super) struct Operation {
        operation_id: String,
    }

    pub(super) async fn prepare(
        State(state): State<AppState>,
        payload: Result<Json<InstallRequest>, JsonRejection>,
    ) -> Accepted {
        let Json(request) = payload.map_err(generation_json_error)?;
        Ok((
            StatusCode::ACCEPTED,
            Json(manager(&state)?.prepare(request)?),
        ))
    }

    pub(super) async fn install(
        State(state): State<AppState>,
        payload: Result<Json<Operation>, JsonRejection>,
    ) -> Accepted {
        let Json(request) = payload.map_err(generation_json_error)?;
        Ok((
            StatusCode::ACCEPTED,
            Json(manager(&state)?.install(&request.operation_id).await?),
        ))
    }

    pub(super) async fn cancel(
        State(state): State<AppState>,
        payload: Result<Json<Operation>, JsonRejection>,
    ) -> Accepted {
        let Json(request) = payload.map_err(generation_json_error)?;
        Ok((
            StatusCode::OK,
            Json(manager(&state)?.cancel_prepared(&request.operation_id)?),
        ))
    }
}

impl From<UpdateError> for AppHttpError {
    fn from(error: UpdateError) -> Self {
        let (status, code) = match error {
            UpdateError::Busy => (StatusCode::CONFLICT, "update_busy"),
            UpdateError::Stale => (StatusCode::CONFLICT, "update_stale"),
            UpdateError::Unsupported(_) => (StatusCode::CONFLICT, "update_unsupported"),
            UpdateError::Trust(_) | UpdateError::Integrity(_) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "update_verification_failed",
            ),
            UpdateError::NoSpace => (StatusCode::INSUFFICIENT_STORAGE, "update_no_space"),
            _ => (StatusCode::SERVICE_UNAVAILABLE, "update_unavailable"),
        };
        Self::new(status, Self::body(code, error.to_string()))
    }
}
