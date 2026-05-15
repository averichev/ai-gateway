use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    app::{AppHttpError, AppState},
    domain::{
        AdminGenerateRequestDto, AdminUserDto, CreateGatewayClientRequestDto,
        CreateTenantRequestDto, CreateUserRequestDto, CreatedGatewayClientDto, GatewayClientDto,
        GenerateResponseDto, ModelRouteDto, ProviderDto, RequestDetailsDto, RequestListItemDto,
        SaveModelRouteRequestDto, SaveProviderRequestDto, SaveProviderSecretRequestDto, TenantDto,
    },
    http::auth::{TenantPermission, require_admin, require_tenant_access, slugify},
    repositories::RepositoryError,
    security::{generate_gateway_client_token, hash_password, token_hash, token_prefix},
};

#[derive(Debug, Deserialize)]
pub struct RequestsQuery {
    #[serde(default = "default_limit")]
    limit: i64,
}

pub async fn list_tenants(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<TenantDto>>, AppHttpError> {
    let principal = require_admin(&state, &headers).await?;
    let tenants = state
        .auth_repo
        .list_tenants_for_user(principal.user_id, &principal.global_role)
        .await?
        .into_iter()
        .map(|(tenant, role)| TenantDto::from_record_with_role(tenant, role))
        .collect();

    Ok(Json(tenants))
}

pub async fn create_tenant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateTenantRequestDto>,
) -> Result<Json<TenantDto>, AppHttpError> {
    let principal = require_admin(&state, &headers).await?;

    if principal.global_role != "owner" {
        return Err(AppHttpError::forbidden(
            "Создавать tenants может только owner",
        ));
    }

    let name = required_text(&payload.name, "name")?;
    let slug = payload
        .slug
        .as_deref()
        .map(slugify)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| slugify(&name));
    let tenant = state
        .auth_repo
        .create_tenant(&name, &slug, principal.user_id)
        .await
        .map_err(map_write_error)?;

    Ok(Json(TenantDto::from_record_with_role(tenant, "owner")))
}

pub async fn list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AdminUserDto>>, AppHttpError> {
    let principal = require_admin(&state, &headers).await?;

    if principal.global_role != "owner" {
        return Err(AppHttpError::forbidden("Смотреть users может только owner"));
    }

    let users = state
        .auth_repo
        .list_users()
        .await?
        .into_iter()
        .map(AdminUserDto::from)
        .collect();

    Ok(Json(users))
}

pub async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateUserRequestDto>,
) -> Result<Json<AdminUserDto>, AppHttpError> {
    let principal = require_admin(&state, &headers).await?;

    if principal.global_role != "owner" {
        return Err(AppHttpError::forbidden(
            "Создавать users может только owner",
        ));
    }

    let email = normalize_email(&payload.email)?;
    let password = payload.password.trim();

    if password.len() < 12 {
        return Err(AppHttpError::bad_request(
            "invalid_request",
            "Пароль должен быть не короче 12 символов",
        ));
    }

    let tenant_role = match payload.tenant_id {
        Some(_) => Some(normalize_tenant_role(payload.tenant_role.as_deref())?),
        None => None,
    };

    let password_hash = hash_password(password).map_err(|_| {
        AppHttpError::internal(
            "gateway_misconfigured",
            "Не удалось подготовить password hash",
        )
    })?;
    let user = state
        .auth_repo
        .create_user(
            &email,
            &password_hash,
            payload.tenant_id,
            tenant_role.as_deref(),
        )
        .await
        .map_err(map_write_error)?;

    Ok(Json(user.into()))
}

pub async fn handle_generate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<Uuid>,
    Json(payload): Json<AdminGenerateRequestDto>,
) -> Result<Json<GenerateResponseDto>, AppHttpError> {
    let principal = require_admin(&state, &headers).await?;
    require_tenant_access(&state, &principal, tenant_id, TenantPermission::Write).await?;
    let response = state
        .generate_service
        .generate_for_admin(payload.request, tenant_id)
        .await?;

    Ok(Json(response))
}

pub async fn list_requests(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<Uuid>,
    Query(query): Query<RequestsQuery>,
) -> Result<Json<Vec<RequestListItemDto>>, AppHttpError> {
    let principal = require_admin(&state, &headers).await?;
    require_tenant_access(&state, &principal, tenant_id, TenantPermission::Read).await?;
    let limit = query.limit.clamp(1, 200);
    let items = state
        .requests_repo
        .list_requests(tenant_id, limit)
        .await?
        .into_iter()
        .map(RequestListItemDto::from)
        .collect();

    Ok(Json(items))
}

pub async fn get_request_details(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, id)): Path<(Uuid, String)>,
) -> Result<Json<RequestDetailsDto>, AppHttpError> {
    let principal = require_admin(&state, &headers).await?;
    require_tenant_access(&state, &principal, tenant_id, TenantPermission::Read).await?;
    let item = state.requests_repo.get_request(tenant_id, &id).await?;

    match item {
        Some(item) => Ok(Json(item.into())),
        None => Err(AppHttpError::not_found(format!("Запрос `{id}` не найден"))),
    }
}

pub async fn list_providers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<Vec<ProviderDto>>, AppHttpError> {
    let principal = require_admin(&state, &headers).await?;
    require_tenant_access(&state, &principal, tenant_id, TenantPermission::Read).await?;
    let items = state
        .routes_repo
        .list_providers(tenant_id)
        .await?
        .into_iter()
        .map(ProviderDto::from)
        .collect();

    Ok(Json(items))
}

pub async fn save_provider(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<Uuid>,
    Json(payload): Json<SaveProviderRequestDto>,
) -> Result<Json<ProviderDto>, AppHttpError> {
    let principal = require_admin(&state, &headers).await?;
    require_tenant_access(&state, &principal, tenant_id, TenantPermission::Write).await?;
    validate_provider_payload(&payload)?;

    let mut provider = state
        .routes_repo
        .save_provider(tenant_id, &payload)
        .await
        .map_err(map_write_error)?;

    if let Some(api_key) = payload
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let encrypted = state.secret_crypto.encrypt(api_key).map_err(|_| {
            AppHttpError::internal(
                "gateway_misconfigured",
                "Не удалось зашифровать provider API key",
            )
        })?;
        state
            .routes_repo
            .save_provider_secret(tenant_id, provider.id, &encrypted, principal.user_id)
            .await?;
        provider.api_key_configured = true;
    }

    Ok(Json(provider.into()))
}

pub async fn save_provider_secret(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, provider_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<SaveProviderSecretRequestDto>,
) -> Result<Json<ProviderDto>, AppHttpError> {
    let principal = require_admin(&state, &headers).await?;
    require_tenant_access(&state, &principal, tenant_id, TenantPermission::Write).await?;
    let api_key = required_text(&payload.api_key, "api_key")?;
    let encrypted = state.secret_crypto.encrypt(&api_key).map_err(|_| {
        AppHttpError::internal(
            "gateway_misconfigured",
            "Не удалось зашифровать provider API key",
        )
    })?;

    state
        .routes_repo
        .save_provider_secret(tenant_id, provider_id, &encrypted, principal.user_id)
        .await
        .map_err(map_read_or_write_error)?;

    let providers = state.routes_repo.list_providers(tenant_id).await?;
    let provider = providers
        .into_iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| AppHttpError::not_found("Provider не найден"))?;

    Ok(Json(provider.into()))
}

pub async fn list_model_routes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<Vec<ModelRouteDto>>, AppHttpError> {
    let principal = require_admin(&state, &headers).await?;
    require_tenant_access(&state, &principal, tenant_id, TenantPermission::Read).await?;
    let items = state
        .routes_repo
        .list_model_routes(tenant_id)
        .await?
        .into_iter()
        .map(ModelRouteDto::from)
        .collect();

    Ok(Json(items))
}

pub async fn save_model_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<Uuid>,
    Json(payload): Json<SaveModelRouteRequestDto>,
) -> Result<Json<ModelRouteDto>, AppHttpError> {
    let principal = require_admin(&state, &headers).await?;
    require_tenant_access(&state, &principal, tenant_id, TenantPermission::Write).await?;
    required_text(&payload.alias, "alias")?;
    required_text(&payload.provider_code, "provider_code")?;
    required_text(&payload.external_model, "external_model")?;

    let route = state
        .routes_repo
        .save_model_route(tenant_id, &payload)
        .await
        .map_err(map_write_error)?;

    Ok(Json(route.into()))
}

pub async fn list_gateway_clients(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<Vec<GatewayClientDto>>, AppHttpError> {
    let principal = require_admin(&state, &headers).await?;
    require_tenant_access(&state, &principal, tenant_id, TenantPermission::Read).await?;
    let items = state
        .auth_repo
        .list_gateway_clients(tenant_id)
        .await?
        .into_iter()
        .map(GatewayClientDto::from)
        .collect();

    Ok(Json(items))
}

pub async fn create_gateway_client(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<Uuid>,
    Json(payload): Json<CreateGatewayClientRequestDto>,
) -> Result<Json<CreatedGatewayClientDto>, AppHttpError> {
    let principal = require_admin(&state, &headers).await?;
    require_tenant_access(&state, &principal, tenant_id, TenantPermission::Write).await?;
    let name = required_text(&payload.name, "name")?;
    let token = generate_gateway_client_token();
    let client = state
        .auth_repo
        .create_gateway_client(
            tenant_id,
            &name,
            &token_hash(&token),
            &token_prefix(&token),
            principal.user_id,
        )
        .await
        .map_err(map_write_error)?;

    Ok(Json(CreatedGatewayClientDto {
        client: client.into(),
        token,
    }))
}

fn validate_provider_payload(payload: &SaveProviderRequestDto) -> Result<(), AppHttpError> {
    required_text(&payload.code, "code")?;
    required_text(&payload.kind, "kind")?;
    required_text(&payload.base_url, "base_url")?;

    if payload.timeout_ms <= 0 {
        return Err(AppHttpError::bad_request(
            "invalid_request",
            "`timeout_ms` должен быть положительным",
        ));
    }

    Ok(())
}

fn required_text(value: &str, field: &str) -> Result<String, AppHttpError> {
    let normalized = value.trim().to_owned();

    if normalized.is_empty() {
        Err(AppHttpError::bad_request(
            "invalid_request",
            format!("Поле `{field}` обязательно"),
        ))
    } else {
        Ok(normalized)
    }
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

fn normalize_tenant_role(value: Option<&str>) -> Result<String, AppHttpError> {
    match value.unwrap_or("viewer").trim() {
        "tenant_admin" => Ok("tenant_admin".to_owned()),
        "viewer" | "" => Ok("viewer".to_owned()),
        _ => Err(AppHttpError::bad_request(
            "invalid_request",
            "`tenant_role` должен быть `tenant_admin` или `viewer`",
        )),
    }
}

fn map_read_or_write_error(error: RepositoryError) -> AppHttpError {
    match error {
        RepositoryError::NotFound => AppHttpError::not_found("Запись не найдена"),
        other => map_write_error(other),
    }
}

fn map_write_error(error: RepositoryError) -> AppHttpError {
    match error {
        RepositoryError::Conflict(message) => AppHttpError::new(
            axum::http::StatusCode::CONFLICT,
            AppHttpError::body("conflict", message),
        ),
        RepositoryError::Database(sqlx::Error::Database(database))
            if database.code().as_deref() == Some("23505") =>
        {
            AppHttpError::new(
                axum::http::StatusCode::CONFLICT,
                AppHttpError::body("conflict", "Запись с такими уникальными полями уже есть"),
            )
        }
        other => AppHttpError::from(other),
    }
}

fn default_limit() -> i64 {
    100
}
