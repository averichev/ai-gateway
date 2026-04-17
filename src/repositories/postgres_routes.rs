use sqlx::{PgPool, query_as};

use crate::domain::{ModelRouteRecord, ProviderConfigRecord, ResolvedRouteRecord};

use super::RepositoryError;

#[derive(Clone)]
pub struct PostgresRoutesRepository {
    pool: PgPool,
}

impl PostgresRoutesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn resolve_route(
        &self,
        alias: &str,
    ) -> Result<Option<ResolvedRouteRecord>, RepositoryError> {
        let route = query_as::<_, ResolvedRouteRecord>(
            r#"
            SELECT
                mr.external_model,
                p.code AS provider_code,
                p.kind AS provider_kind,
                p.base_url AS provider_base_url,
                p.api_key_env AS provider_api_key_env,
                p.timeout_ms AS provider_timeout_ms
            FROM model_routes mr
            INNER JOIN providers p ON p.code = mr.provider_code
            WHERE mr.alias = $1
              AND mr.is_enabled = TRUE
              AND p.is_enabled = TRUE
            LIMIT 1
            "#,
        )
        .bind(alias)
        .fetch_optional(&self.pool)
        .await?;

        Ok(route)
    }

    pub async fn list_providers(&self) -> Result<Vec<ProviderConfigRecord>, RepositoryError> {
        let items = query_as::<_, ProviderConfigRecord>(
            r#"
            SELECT
                id,
                code,
                kind,
                base_url,
                api_key_env,
                is_enabled,
                timeout_ms,
                created_at,
                updated_at
            FROM providers
            ORDER BY code ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(items)
    }

    pub async fn list_model_routes(&self) -> Result<Vec<ModelRouteRecord>, RepositoryError> {
        let items = query_as::<_, ModelRouteRecord>(
            r#"
            SELECT
                id,
                alias,
                provider_code,
                external_model,
                is_enabled,
                created_at,
                updated_at
            FROM model_routes
            ORDER BY alias ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(items)
    }
}
