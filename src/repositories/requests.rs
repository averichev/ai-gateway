use sqlx::query_as;
use uuid::Uuid;

use crate::domain::{RequestLogInsert, RequestLogRecord};

use super::{Database, RepositoryError};

#[derive(Clone)]
pub struct RequestsRepository {
    pool: Database,
}

impl RequestsRepository {
    pub fn new(pool: impl Into<Database>) -> Self {
        Self { pool: pool.into() }
    }

    pub async fn count_content_previews(
        &self,
        tenant_id: Option<Uuid>,
    ) -> Result<i64, RepositoryError> {
        let count = with_pool!(&self.pool, |pool| {
            sqlx::query_scalar(
                r#"
                SELECT COUNT(*) FROM requests
                WHERE (tenant_id = $1 OR $1 IS NULL)
                  AND (prompt_preview IS NOT NULL OR response_preview IS NOT NULL)
                "#,
            )
            .bind(tenant_id)
            .fetch_one(pool)
            .await
        })?;
        Ok(count)
    }

    /// Explicit admin operation. Only preview columns change; history is retained.
    pub async fn clear_content_previews(
        &self,
        tenant_id: Option<Uuid>,
    ) -> Result<u64, RepositoryError> {
        let cleared = with_pool!(&self.pool, |pool| {
            sqlx::query(
                r#"
                UPDATE requests SET prompt_preview = NULL, response_preview = NULL
                WHERE (tenant_id = $1 OR $1 IS NULL)
                  AND (prompt_preview IS NOT NULL OR response_preview IS NOT NULL)
                "#,
            )
            .bind(tenant_id)
            .execute(pool)
            .await?
            .rows_affected()
        });
        Ok(cleared)
    }

    pub async fn insert_request(&self, request: &RequestLogInsert) -> Result<(), RepositoryError> {
        with_pool!(&self.pool, |pool| {
            sqlx::query(
                r#"
                INSERT INTO requests (
                    id,
                    tenant_id,
                    gateway_client_id,
                    model_alias,
                    provider_code,
                    external_model,
                    status,
                    latency_ms,
                    error_message,
                    input_tokens,
                    output_tokens,
                    prompt_preview,
                    response_preview
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                "#,
            )
            .bind(&request.id)
            .bind(request.tenant_id)
            .bind(request.gateway_client_id)
            .bind(&request.model_alias)
            .bind(&request.provider_code)
            .bind(&request.external_model)
            .bind(&request.status)
            .bind(request.latency_ms)
            .bind(&request.error_message)
            .bind(request.input_tokens)
            .bind(request.output_tokens)
            .bind(&request.prompt_preview)
            .bind(&request.response_preview)
            .execute(pool)
            .await?;
        });

        Ok(())
    }

    pub async fn list_requests(
        &self,
        tenant_id: Uuid,
        limit: i64,
    ) -> Result<Vec<RequestLogRecord>, RepositoryError> {
        let items = with_pool!(&self.pool, |pool| {
            query_as::<_, RequestLogRecord>(
                r#"
                SELECT
                    r.id,
                    r.tenant_id,
                    r.gateway_client_id,
                    gc.name AS gateway_client_name,
                    r.created_at,
                    r.model_alias,
                    r.provider_code,
                    r.external_model,
                    r.status,
                    r.latency_ms,
                    r.error_message,
                    r.input_tokens,
                    r.output_tokens,
                    r.prompt_preview,
                    r.response_preview
                FROM requests r
                LEFT JOIN gateway_clients gc ON gc.id = r.gateway_client_id
                WHERE r.tenant_id = $1
                ORDER BY r.created_at DESC
                LIMIT $2
                "#,
            )
            .bind(tenant_id)
            .bind(limit)
            .fetch_all(pool)
            .await
        })?;

        Ok(items)
    }

    pub async fn get_request(
        &self,
        tenant_id: Uuid,
        id: &str,
    ) -> Result<Option<RequestLogRecord>, RepositoryError> {
        let item = with_pool!(&self.pool, |pool| {
            query_as::<_, RequestLogRecord>(
                r#"
                SELECT
                    r.id,
                    r.tenant_id,
                    r.gateway_client_id,
                    gc.name AS gateway_client_name,
                    r.created_at,
                    r.model_alias,
                    r.provider_code,
                    r.external_model,
                    r.status,
                    r.latency_ms,
                    r.error_message,
                    r.input_tokens,
                    r.output_tokens,
                    r.prompt_preview,
                    r.response_preview
                FROM requests r
                LEFT JOIN gateway_clients gc ON gc.id = r.gateway_client_id
                WHERE r.tenant_id = $1
                  AND r.id = $2
                LIMIT 1
                "#,
            )
            .bind(tenant_id)
            .bind(id)
            .fetch_optional(pool)
            .await
        })?;

        Ok(item)
    }
}
