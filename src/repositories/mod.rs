// SQLx uses different native encodings for UUIDs and timestamps on each backend.
// Dispatch on the pool so both drivers type-check the same query or transaction.
macro_rules! with_pool {
    ($database:expr, |$pool:ident| $body:expr) => {
        match $database {
            $crate::repositories::Database::Postgres($pool) => $body,
            $crate::repositories::Database::Sqlite($pool) => $body,
        }
    };
}

pub mod auth;
pub(crate) mod database;
pub mod requests;
pub mod routes;

use crate::config::AppConfig;
use thiserror::Error;
use uuid::Uuid;

pub use auth::AuthRepository;
pub use database::Database;
pub use requests::RequestsRepository;
pub use routes::RoutesRepository;

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("record not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
}

impl RepositoryError {
    /// Database Display/Debug can include failing row values, SQL and secrets.
    pub fn category(&self) -> &'static str {
        match self {
            Self::Database(sqlx::Error::PoolTimedOut) => "pool_timeout",
            Self::Database(sqlx::Error::Io(_) | sqlx::Error::Tls(_)) => "connection",
            Self::Database(_) => "database",
            Self::NotFound => "not_found",
            Self::Conflict(_) => "conflict",
        }
    }
}

pub async fn seed_defaults(database: &Database, config: &AppConfig) -> Result<(), RepositoryError> {
    with_pool!(database, |pool| {
        let mut tx = pool.begin().await?;
        sqlx::query(
            r#"
            INSERT INTO tenants (id, name, slug) VALUES ($1, 'Default tenant', 'default')
            ON CONFLICT (slug) DO NOTHING
            "#,
        )
        .bind(Uuid::new_v4())
        .execute(&mut *tx)
        .await?;

        let default_tenant_id: Uuid = sqlx::query_scalar(
            r#"
            SELECT id
            FROM tenants
            WHERE slug = 'default'
            LIMIT 1
            "#,
        )
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO providers (
                id,
                tenant_id,
                code,
                kind,
                base_url,
                api_key_env,
                is_enabled,
                timeout_ms
            )
            VALUES ($1, $2, $3, $4, $5, $6, TRUE, $7)
            ON CONFLICT (tenant_id, code) DO UPDATE
            SET
                kind = EXCLUDED.kind,
                base_url = EXCLUDED.base_url,
                api_key_env = EXCLUDED.api_key_env,
                timeout_ms = EXCLUDED.timeout_ms,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(default_tenant_id)
        .bind(&config.default_provider_code)
        .bind(&config.default_provider_kind)
        .bind(&config.default_provider_base_url)
        .bind("")
        .bind(config.default_provider_timeout_ms)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO model_routes (
                id,
                tenant_id,
                alias,
                provider_code,
                external_model,
                is_enabled
            )
            VALUES ($1, $2, $3, $4, $5, TRUE)
            ON CONFLICT (tenant_id, alias) DO UPDATE
            SET
                provider_code = EXCLUDED.provider_code,
                external_model = EXCLUDED.external_model,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(default_tenant_id)
        .bind(&config.default_model_alias)
        .bind(&config.default_provider_code)
        .bind(&config.default_external_model)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    })
}
