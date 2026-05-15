pub mod postgres_auth;
pub mod postgres_requests;
pub mod postgres_routes;

use crate::config::AppConfig;
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

pub use postgres_auth::PostgresAuthRepository;
pub use postgres_requests::PostgresRequestsRepository;
pub use postgres_routes::PostgresRoutesRepository;

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("record not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
}

pub async fn seed_defaults(pool: &PgPool, config: &AppConfig) -> Result<(), RepositoryError> {
    let default_tenant_id: Uuid = sqlx::query_scalar(
        r#"
        SELECT id
        FROM tenants
        WHERE slug = 'default'
        LIMIT 1
        "#,
    )
    .fetch_one(pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO providers (
            tenant_id,
            code,
            kind,
            base_url,
            api_key_env,
            is_enabled,
            timeout_ms
        )
        VALUES ($1, $2, $3, $4, $5, TRUE, $6)
        ON CONFLICT (tenant_id, code) DO UPDATE
        SET
            kind = EXCLUDED.kind,
            base_url = EXCLUDED.base_url,
            api_key_env = EXCLUDED.api_key_env,
            timeout_ms = EXCLUDED.timeout_ms,
            updated_at = NOW()
        "#,
    )
    .bind(default_tenant_id)
    .bind(&config.default_provider_code)
    .bind(&config.default_provider_kind)
    .bind(&config.default_provider_base_url)
    .bind("")
    .bind(config.default_provider_timeout_ms)
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO model_routes (
            tenant_id,
            alias,
            provider_code,
            external_model,
            is_enabled
        )
        VALUES ($1, $2, $3, $4, TRUE)
        ON CONFLICT (tenant_id, alias) DO UPDATE
        SET
            provider_code = EXCLUDED.provider_code,
            external_model = EXCLUDED.external_model,
            updated_at = NOW()
        "#,
    )
    .bind(default_tenant_id)
    .bind(&config.default_model_alias)
    .bind(&config.default_provider_code)
    .bind(&config.default_external_model)
    .execute(pool)
    .await?;

    Ok(())
}
