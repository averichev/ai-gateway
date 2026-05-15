use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
};
use chrono::{Duration, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    app::{AppHttpError, AppState},
    domain::{
        AuthResponseDto, BootstrapRegisterRequestDto, BootstrapStatusDto, CurrentUserDto,
        LoginRequestDto, TenantDto, TenantRecord,
    },
    repositories::RepositoryError,
    security::{generate_admin_session_token, hash_password, token_hash, verify_password},
};

const SESSION_TTL_DAYS: i64 = 7;

#[derive(Debug, Clone, Serialize)]
pub struct AdminPrincipal {
    pub user_id: Uuid,
    pub email: String,
    pub global_role: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantPermission {
    Read,
    Write,
}

pub async fn bootstrap_status(
    State(state): State<AppState>,
) -> Result<Json<BootstrapStatusDto>, AppHttpError> {
    let has_users = state.auth_repo.has_users().await?;
    Ok(Json(BootstrapStatusDto { has_users }))
}

pub async fn bootstrap_register(
    State(state): State<AppState>,
    Json(payload): Json<BootstrapRegisterRequestDto>,
) -> Result<Json<AuthResponseDto>, AppHttpError> {
    let email = normalize_email(&payload.email)?;
    let password = payload.password.trim();

    if password.len() < 12 {
        return Err(AppHttpError::bad_request(
            "invalid_request",
            "Пароль должен быть не короче 12 символов",
        ));
    }

    let tenant_name = payload
        .tenant_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Default tenant");
    let tenant_slug = slugify(tenant_name);
    let password_hash = hash_password(password).map_err(|error| {
        AppHttpError::internal(
            "gateway_misconfigured",
            format!("Не удалось подготовить password hash: {error}"),
        )
    })?;

    let (user, tenant) = state
        .auth_repo
        .create_owner(&email, &password_hash, tenant_name, &tenant_slug)
        .await
        .map_err(map_bootstrap_error)?;

    let token = create_session(&state, user.id).await?;
    let tenants = vec![TenantDto::from_record_with_role(tenant, "owner")];

    Ok(Json(AuthResponseDto {
        token,
        user: CurrentUserDto {
            id: user.id,
            email: user.email,
            global_role: user.global_role,
        },
        tenants,
    }))
}

pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequestDto>,
) -> Result<Json<AuthResponseDto>, AppHttpError> {
    let email = normalize_email(&payload.email)?;
    let user = state
        .auth_repo
        .find_user_by_email(&email)
        .await?
        .ok_or_else(invalid_credentials)?;

    if !user.is_enabled {
        return Err(AppHttpError::forbidden("Пользователь отключен"));
    }

    let password_ok =
        verify_password(payload.password.trim(), &user.password_hash).map_err(|_| {
            AppHttpError::internal(
                "gateway_misconfigured",
                "Не удалось проверить password hash",
            )
        })?;

    if !password_ok {
        return Err(invalid_credentials());
    }

    let token = create_session(&state, user.id).await?;
    let tenants = load_tenants(&state, user.id, &user.global_role).await?;

    Ok(Json(AuthResponseDto {
        token,
        user: CurrentUserDto {
            id: user.id,
            email: user.email,
            global_role: user.global_role,
        },
        tenants,
    }))
}

pub async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthResponseDto>, AppHttpError> {
    let principal = require_admin(&state, &headers).await?;
    let tenants = load_tenants(&state, principal.user_id, &principal.global_role).await?;

    Ok(Json(AuthResponseDto {
        token: String::new(),
        user: CurrentUserDto {
            id: principal.user_id,
            email: principal.email,
            global_role: principal.global_role,
        },
        tenants,
    }))
}

pub async fn require_admin(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminPrincipal, AppHttpError> {
    let token = bearer_token(headers)?;
    let session = state
        .auth_repo
        .get_session(&token_hash(&token))
        .await?
        .ok_or_else(|| AppHttpError::unauthorized("Требуется авторизация"))?;

    if !session.is_enabled {
        return Err(AppHttpError::forbidden("Пользователь отключен"));
    }

    Ok(AdminPrincipal {
        user_id: session.user_id,
        email: session.email,
        global_role: session.global_role,
    })
}

pub async fn require_tenant_access(
    state: &AppState,
    principal: &AdminPrincipal,
    tenant_id: Uuid,
    permission: TenantPermission,
) -> Result<String, AppHttpError> {
    let access = state
        .auth_repo
        .get_tenant_access(principal.user_id, &principal.global_role, tenant_id)
        .await?
        .ok_or_else(|| AppHttpError::forbidden("Нет доступа к tenant"))?;

    if permission == TenantPermission::Write
        && principal.global_role != "owner"
        && access.role != "tenant_admin"
    {
        return Err(AppHttpError::forbidden(
            "Недостаточно прав для изменения tenant",
        ));
    }

    Ok(access.role)
}

pub fn bearer_token(headers: &HeaderMap) -> Result<String, AppHttpError> {
    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppHttpError::unauthorized("Отсутствует Authorization Bearer token"))?;

    value
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppHttpError::unauthorized("Некорректный Authorization Bearer token"))
}

pub fn slugify(value: &str) -> String {
    let slug = value
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    if slug.is_empty() {
        format!("tenant-{}", Uuid::new_v4().simple())
    } else {
        slug
    }
}

async fn create_session(state: &AppState, user_id: Uuid) -> Result<String, AppHttpError> {
    let token = generate_admin_session_token();
    let expires_at = Utc::now() + Duration::days(SESSION_TTL_DAYS);
    state
        .auth_repo
        .create_session(user_id, &token_hash(&token), expires_at)
        .await?;

    Ok(token)
}

async fn load_tenants(
    state: &AppState,
    user_id: Uuid,
    global_role: &str,
) -> Result<Vec<TenantDto>, AppHttpError> {
    let tenants = state
        .auth_repo
        .list_tenants_for_user(user_id, global_role)
        .await?
        .into_iter()
        .map(|(tenant, role): (TenantRecord, String)| {
            TenantDto::from_record_with_role(tenant, role)
        })
        .collect();

    Ok(tenants)
}

fn normalize_email(value: &str) -> Result<String, AppHttpError> {
    let email = value.trim().to_lowercase();

    if email.contains('@') && email.len() <= 320 {
        Ok(email)
    } else {
        Err(AppHttpError::bad_request(
            "invalid_request",
            "Некорректный email",
        ))
    }
}

fn invalid_credentials() -> AppHttpError {
    AppHttpError::unauthorized("Неверный email или пароль")
}

fn map_bootstrap_error(error: RepositoryError) -> AppHttpError {
    match error {
        RepositoryError::Conflict(message) => AppHttpError::new(
            StatusCode::CONFLICT,
            AppHttpError::body("registration_closed", message),
        ),
        other => AppHttpError::from(other),
    }
}
