use sqlx::query_as;
use uuid::Uuid;

use crate::{
    domain::{
        ModelRouteRecord, ProviderConfigRecord, ProviderSecretRecord, ResolvedRouteRecord,
        SaveModelRouteRequestDto, SaveProviderRequestDto,
    },
    security::EncryptedSecret,
};

use super::{Database, RepositoryError};

#[derive(Clone)]
pub struct RoutesRepository {
    pool: Database,
}

impl RoutesRepository {
    pub fn new(pool: impl Into<Database>) -> Self {
        Self { pool: pool.into() }
    }

    pub async fn resolve_route(
        &self,
        tenant_id: Uuid,
        alias: &str,
    ) -> Result<Option<ResolvedRouteRecord>, RepositoryError> {
        let route = with_pool!(&self.pool, |pool| {
            query_as::<_, ResolvedRouteRecord>(
                r#"
                SELECT
                    p.id AS provider_id,
                    mr.external_model,
                    p.code AS provider_code,
                    p.kind AS provider_kind,
                    p.base_url AS provider_base_url,
                    p.timeout_ms AS provider_timeout_ms
                FROM model_routes mr
                INNER JOIN providers p
                    ON p.tenant_id = mr.tenant_id
                   AND p.code = mr.provider_code
                WHERE mr.tenant_id = $1
                  AND mr.alias = $2
                  AND mr.is_enabled = TRUE
                  AND p.is_enabled = TRUE
                LIMIT 1
                "#,
            )
            .bind(tenant_id)
            .bind(alias)
            .fetch_optional(pool)
            .await
        })?;

        Ok(route)
    }

    pub(crate) async fn has_provider_secrets(&self) -> Result<bool, RepositoryError> {
        let exists = with_pool!(&self.pool, |pool| {
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM provider_secrets)")
                .fetch_one(pool)
                .await
        })?;
        Ok(exists)
    }

    pub async fn get_provider_secret(
        &self,
        tenant_id: Uuid,
        provider_id: Uuid,
    ) -> Result<Option<ProviderSecretRecord>, RepositoryError> {
        let secret = with_pool!(&self.pool, |pool| {
            query_as::<_, ProviderSecretRecord>(
                r#"
                SELECT
                    ciphertext,
                    nonce,
                    algorithm,
                    key_version
                FROM provider_secrets
                WHERE tenant_id = $1
                  AND provider_id = $2
                LIMIT 1
                "#,
            )
            .bind(tenant_id)
            .bind(provider_id)
            .fetch_optional(pool)
            .await
        })?;

        Ok(secret)
    }

    pub async fn list_providers(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ProviderConfigRecord>, RepositoryError> {
        let items = with_pool!(&self.pool, |pool| {
            query_as::<_, ProviderConfigRecord>(
                r#"
                SELECT
                    p.id,
                    p.tenant_id,
                    p.code,
                    p.kind,
                    p.base_url,
                    EXISTS (
                        SELECT 1
                        FROM provider_secrets ps
                        WHERE ps.provider_id = p.id
                    ) AS api_key_configured,
                    p.is_enabled,
                    p.timeout_ms,
                    p.created_at,
                    p.updated_at
                FROM providers p
                WHERE p.tenant_id = $1
                ORDER BY p.code ASC
                "#,
            )
            .bind(tenant_id)
            .fetch_all(pool)
            .await
        })?;

        Ok(items)
    }

    pub async fn save_provider(
        &self,
        tenant_id: Uuid,
        payload: &SaveProviderRequestDto,
    ) -> Result<ProviderConfigRecord, RepositoryError> {
        with_pool!(&self.pool, |pool| {
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
                VALUES ($1, $2, $3, $4, $5, '', $6, $7)
                ON CONFLICT (tenant_id, code) DO UPDATE
                SET
                    kind = EXCLUDED.kind,
                    base_url = EXCLUDED.base_url,
                    is_enabled = EXCLUDED.is_enabled,
                    timeout_ms = EXCLUDED.timeout_ms,
                    updated_at = CURRENT_TIMESTAMP
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(tenant_id)
            .bind(payload.code.trim())
            .bind(payload.kind.trim())
            .bind(payload.base_url.trim())
            .bind(payload.is_enabled)
            .bind(payload.timeout_ms)
            .execute(pool)
            .await?;
        });

        self.get_provider_by_code(tenant_id, payload.code.trim())
            .await?
            .ok_or(RepositoryError::NotFound)
    }

    pub async fn save_provider_secret(
        &self,
        tenant_id: Uuid,
        provider_id: Uuid,
        encrypted: &EncryptedSecret,
        created_by: Uuid,
    ) -> Result<(), RepositoryError> {
        let provider_exists: bool = with_pool!(&self.pool, |pool| {
            sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM providers
                    WHERE tenant_id = $1
                      AND id = $2
                )
                "#,
            )
            .bind(tenant_id)
            .bind(provider_id)
            .fetch_one(pool)
            .await
        })?;

        if !provider_exists {
            return Err(RepositoryError::NotFound);
        }

        with_pool!(&self.pool, |pool| {
            sqlx::query(
                r#"
                INSERT INTO provider_secrets (
                    id,
                    tenant_id,
                    provider_id,
                    ciphertext,
                    nonce,
                    algorithm,
                    key_version,
                    created_by
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                ON CONFLICT (provider_id) DO UPDATE
                SET
                    ciphertext = EXCLUDED.ciphertext,
                    nonce = EXCLUDED.nonce,
                    algorithm = EXCLUDED.algorithm,
                    key_version = EXCLUDED.key_version,
                    updated_at = CURRENT_TIMESTAMP
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(tenant_id)
            .bind(provider_id)
            .bind(&encrypted.ciphertext)
            .bind(&encrypted.nonce)
            .bind(&encrypted.algorithm)
            .bind(encrypted.key_version)
            .bind(created_by)
            .execute(pool)
            .await?;
        });

        Ok(())
    }

    pub async fn list_model_routes(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ModelRouteRecord>, RepositoryError> {
        let items = with_pool!(&self.pool, |pool| {
            query_as::<_, ModelRouteRecord>(
                r#"
                SELECT
                    id,
                    tenant_id,
                    alias,
                    provider_code,
                    external_model,
                    is_enabled,
                    created_at,
                    updated_at
                FROM model_routes
                WHERE tenant_id = $1
                ORDER BY alias ASC
                "#,
            )
            .bind(tenant_id)
            .fetch_all(pool)
            .await
        })?;

        Ok(items)
    }

    pub async fn save_model_route(
        &self,
        tenant_id: Uuid,
        payload: &SaveModelRouteRequestDto,
    ) -> Result<ModelRouteRecord, RepositoryError> {
        with_pool!(&self.pool, |pool| {
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
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT (tenant_id, alias) DO UPDATE
                SET
                    provider_code = EXCLUDED.provider_code,
                    external_model = EXCLUDED.external_model,
                    is_enabled = EXCLUDED.is_enabled,
                    updated_at = CURRENT_TIMESTAMP
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(tenant_id)
            .bind(payload.alias.trim())
            .bind(payload.provider_code.trim())
            .bind(payload.external_model.trim())
            .bind(payload.is_enabled)
            .execute(pool)
            .await?;
        });

        self.get_model_route_by_alias(tenant_id, payload.alias.trim())
            .await?
            .ok_or(RepositoryError::NotFound)
    }

    pub async fn get_provider_by_code(
        &self,
        tenant_id: Uuid,
        code: &str,
    ) -> Result<Option<ProviderConfigRecord>, RepositoryError> {
        let item = with_pool!(&self.pool, |pool| {
            query_as::<_, ProviderConfigRecord>(
                r#"
                SELECT
                    p.id,
                    p.tenant_id,
                    p.code,
                    p.kind,
                    p.base_url,
                    EXISTS (
                        SELECT 1
                        FROM provider_secrets ps
                        WHERE ps.provider_id = p.id
                    ) AS api_key_configured,
                    p.is_enabled,
                    p.timeout_ms,
                    p.created_at,
                    p.updated_at
                FROM providers p
                WHERE p.tenant_id = $1
                  AND p.code = $2
                LIMIT 1
                "#,
            )
            .bind(tenant_id)
            .bind(code)
            .fetch_optional(pool)
            .await
        })?;

        Ok(item)
    }

    pub async fn get_model_route_by_alias(
        &self,
        tenant_id: Uuid,
        alias: &str,
    ) -> Result<Option<ModelRouteRecord>, RepositoryError> {
        let item = with_pool!(&self.pool, |pool| {
            query_as::<_, ModelRouteRecord>(
                r#"
                SELECT
                    id,
                    tenant_id,
                    alias,
                    provider_code,
                    external_model,
                    is_enabled,
                    created_at,
                    updated_at
                FROM model_routes
                WHERE tenant_id = $1
                  AND alias = $2
                LIMIT 1
                "#,
            )
            .bind(tenant_id)
            .bind(alias)
            .fetch_optional(pool)
            .await
        })?;

        Ok(item)
    }
}
