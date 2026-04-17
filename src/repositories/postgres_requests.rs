use sqlx::{PgPool, query_as};

use crate::domain::{RequestLogInsert, RequestLogRecord};

use super::RepositoryError;

#[derive(Clone)]
pub struct PostgresRequestsRepository {
    pool: PgPool,
}

impl PostgresRequestsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert_request(&self, request: &RequestLogInsert) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            INSERT INTO requests (
                id,
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
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(&request.id)
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
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn list_requests(
        &self,
        limit: i64,
    ) -> Result<Vec<RequestLogRecord>, RepositoryError> {
        let items = query_as::<_, RequestLogRecord>(
            r#"
            SELECT
                id,
                created_at,
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
            FROM requests
            ORDER BY created_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(items)
    }

    pub async fn get_request(&self, id: &str) -> Result<Option<RequestLogRecord>, RepositoryError> {
        let item = query_as::<_, RequestLogRecord>(
            r#"
            SELECT
                id,
                created_at,
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
            FROM requests
            WHERE id = $1
            LIMIT 1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(item)
    }
}
