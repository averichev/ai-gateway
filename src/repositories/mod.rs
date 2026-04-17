pub mod postgres_requests;
pub mod postgres_routes;

use crate::config::AppConfig;
use sqlx::PgPool;
use thiserror::Error;

pub use postgres_requests::PostgresRequestsRepository;
pub use postgres_routes::PostgresRoutesRepository;

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub async fn seed_defaults(pool: &PgPool, config: &AppConfig) -> Result<(), RepositoryError> {
    sqlx::query(
        r#"
        INSERT INTO providers (
            code,
            kind,
            base_url,
            api_key_env,
            is_enabled,
            timeout_ms
        )
        VALUES ($1, $2, $3, $4, TRUE, $5)
        ON CONFLICT (code) DO UPDATE
        SET
            kind = EXCLUDED.kind,
            base_url = EXCLUDED.base_url,
            api_key_env = EXCLUDED.api_key_env,
            timeout_ms = EXCLUDED.timeout_ms,
            updated_at = NOW()
        "#,
    )
    .bind(&config.default_provider_code)
    .bind(&config.default_provider_kind)
    .bind(&config.default_provider_base_url)
    .bind(&config.default_provider_api_key_env)
    .bind(config.default_provider_timeout_ms)
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO model_routes (
            alias,
            provider_code,
            external_model,
            is_enabled
        )
        VALUES ($1, $2, $3, TRUE)
        ON CONFLICT (alias) DO UPDATE
        SET
            provider_code = EXCLUDED.provider_code,
            external_model = EXCLUDED.external_model,
            updated_at = NOW()
        "#,
    )
    .bind(&config.default_model_alias)
    .bind(&config.default_provider_code)
    .bind(&config.default_external_model)
    .execute(pool)
    .await?;

    Ok(())
}
