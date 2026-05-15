use chrono::{DateTime, Utc};
use sqlx::{PgPool, query_as};
use uuid::Uuid;

use crate::domain::{
    AdminUserRecord, GatewayClientAuthRecord, GatewayClientRecord, SessionRecord,
    TenantAccessRecord, TenantRecord, UserRecord,
};

use super::RepositoryError;

#[derive(Clone)]
pub struct PostgresAuthRepository {
    pool: PgPool,
}

impl PostgresAuthRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn has_users(&self) -> Result<bool, RepositoryError> {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users LIMIT 1)")
            .fetch_one(&self.pool)
            .await?;

        Ok(exists)
    }

    pub async fn create_owner(
        &self,
        email: &str,
        password_hash: &str,
        tenant_name: &str,
        tenant_slug: &str,
    ) -> Result<(UserRecord, TenantRecord), RepositoryError> {
        let mut tx = self.pool.begin().await?;

        let has_users: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users LIMIT 1)")
            .fetch_one(&mut *tx)
            .await?;

        if has_users {
            return Err(RepositoryError::Conflict(
                "bootstrap registration is already closed".to_owned(),
            ));
        }

        let user = query_as::<_, UserRecord>(
            r#"
            INSERT INTO users (email, password_hash, global_role)
            VALUES ($1, $2, 'owner')
            RETURNING id, email, password_hash, global_role, is_enabled
            "#,
        )
        .bind(email)
        .bind(password_hash)
        .fetch_one(&mut *tx)
        .await?;

        let tenant = query_as::<_, TenantRecord>(
            r#"
            INSERT INTO tenants (name, slug, created_by)
            VALUES ($1, $2, $3)
            ON CONFLICT (slug) DO UPDATE
            SET
                name = EXCLUDED.name,
                updated_at = NOW()
            RETURNING id, name, slug, created_at, updated_at
            "#,
        )
        .bind(tenant_name)
        .bind(tenant_slug)
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO tenant_members (tenant_id, user_id, role)
            VALUES ($1, $2, 'tenant_admin')
            ON CONFLICT (tenant_id, user_id) DO UPDATE
            SET role = EXCLUDED.role
            "#,
        )
        .bind(tenant.id)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok((user, tenant))
    }

    pub async fn find_user_by_email(
        &self,
        email: &str,
    ) -> Result<Option<UserRecord>, RepositoryError> {
        let user = query_as::<_, UserRecord>(
            r#"
            SELECT id, email, password_hash, global_role, is_enabled
            FROM users
            WHERE LOWER(email) = LOWER($1)
            LIMIT 1
            "#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }

    pub async fn create_session(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            INSERT INTO admin_sessions (user_id, token_hash, expires_at)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(user_id)
        .bind(token_hash)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_session(
        &self,
        token_hash: &str,
    ) -> Result<Option<SessionRecord>, RepositoryError> {
        let session = query_as::<_, SessionRecord>(
            r#"
            SELECT
                u.id AS user_id,
                u.email,
                u.global_role,
                u.is_enabled
            FROM admin_sessions s
            INNER JOIN users u ON u.id = s.user_id
            WHERE s.token_hash = $1
              AND s.expires_at > NOW()
            LIMIT 1
            "#,
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(session)
    }

    pub async fn list_tenants_for_user(
        &self,
        user_id: Uuid,
        global_role: &str,
    ) -> Result<Vec<(TenantRecord, String)>, RepositoryError> {
        if global_role == "owner" {
            let tenants = query_as::<_, TenantRecord>(
                r#"
                SELECT id, name, slug, created_at, updated_at
                FROM tenants
                ORDER BY name ASC
                "#,
            )
            .fetch_all(&self.pool)
            .await?;

            return Ok(tenants
                .into_iter()
                .map(|tenant| (tenant, "owner".to_owned()))
                .collect());
        }

        let rows = sqlx::query_as::<_, TenantWithRoleRow>(
            r#"
            SELECT
                t.id,
                t.name,
                t.slug,
                t.created_at,
                t.updated_at,
                tm.role
            FROM tenant_members tm
            INNER JOIN tenants t ON t.id = tm.tenant_id
            WHERE tm.user_id = $1
            ORDER BY t.name ASC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    TenantRecord {
                        id: row.id,
                        name: row.name,
                        slug: row.slug,
                        created_at: row.created_at,
                        updated_at: row.updated_at,
                    },
                    row.role,
                )
            })
            .collect())
    }

    pub async fn get_tenant_access(
        &self,
        user_id: Uuid,
        global_role: &str,
        tenant_id: Uuid,
    ) -> Result<Option<TenantAccessRecord>, RepositoryError> {
        if global_role == "owner" {
            let exists: bool = sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM tenants
                    WHERE id = $1
                )
                "#,
            )
            .bind(tenant_id)
            .fetch_one(&self.pool)
            .await?;

            return Ok(exists.then_some(TenantAccessRecord {
                tenant_id,
                role: "owner".to_owned(),
            }));
        }

        let access = query_as::<_, TenantAccessRecord>(
            r#"
            SELECT tenant_id, role
            FROM tenant_members
            WHERE tenant_id = $1
              AND user_id = $2
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(access)
    }

    pub async fn create_tenant(
        &self,
        name: &str,
        slug: &str,
        created_by: Uuid,
    ) -> Result<TenantRecord, RepositoryError> {
        let mut tx = self.pool.begin().await?;

        let tenant = query_as::<_, TenantRecord>(
            r#"
            INSERT INTO tenants (name, slug, created_by)
            VALUES ($1, $2, $3)
            RETURNING id, name, slug, created_at, updated_at
            "#,
        )
        .bind(name)
        .bind(slug)
        .bind(created_by)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO tenant_members (tenant_id, user_id, role)
            VALUES ($1, $2, 'tenant_admin')
            ON CONFLICT (tenant_id, user_id) DO NOTHING
            "#,
        )
        .bind(tenant.id)
        .bind(created_by)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(tenant)
    }

    pub async fn list_users(&self) -> Result<Vec<AdminUserRecord>, RepositoryError> {
        let users = query_as::<_, AdminUserRecord>(
            r#"
            SELECT
                u.id,
                u.email,
                u.global_role,
                u.is_enabled,
                COUNT(tm.tenant_id)::BIGINT AS tenant_count,
                u.created_at,
                u.updated_at
            FROM users u
            LEFT JOIN tenant_members tm ON tm.user_id = u.id
            GROUP BY u.id
            ORDER BY u.created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(users)
    }

    pub async fn create_user(
        &self,
        email: &str,
        password_hash: &str,
        tenant_id: Option<Uuid>,
        tenant_role: Option<&str>,
    ) -> Result<AdminUserRecord, RepositoryError> {
        let mut tx = self.pool.begin().await?;

        let user = query_as::<_, AdminUserRecord>(
            r#"
            INSERT INTO users (email, password_hash, global_role)
            VALUES ($1, $2, 'user')
            RETURNING
                id,
                email,
                global_role,
                is_enabled,
                0::BIGINT AS tenant_count,
                created_at,
                updated_at
            "#,
        )
        .bind(email)
        .bind(password_hash)
        .fetch_one(&mut *tx)
        .await?;

        if let (Some(tenant_id), Some(tenant_role)) = (tenant_id, tenant_role) {
            sqlx::query(
                r#"
                INSERT INTO tenant_members (tenant_id, user_id, role)
                VALUES ($1, $2, $3)
                "#,
            )
            .bind(tenant_id)
            .bind(user.id)
            .bind(tenant_role)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        self.get_admin_user(user.id)
            .await?
            .ok_or(RepositoryError::NotFound)
    }

    async fn get_admin_user(
        &self,
        user_id: Uuid,
    ) -> Result<Option<AdminUserRecord>, RepositoryError> {
        let user = query_as::<_, AdminUserRecord>(
            r#"
            SELECT
                u.id,
                u.email,
                u.global_role,
                u.is_enabled,
                COUNT(tm.tenant_id)::BIGINT AS tenant_count,
                u.created_at,
                u.updated_at
            FROM users u
            LEFT JOIN tenant_members tm ON tm.user_id = u.id
            WHERE u.id = $1
            GROUP BY u.id
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }

    pub async fn list_gateway_clients(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<GatewayClientRecord>, RepositoryError> {
        let items = query_as::<_, GatewayClientRecord>(
            r#"
            SELECT
                id,
                tenant_id,
                name,
                token_prefix,
                is_enabled,
                last_used_at,
                created_at,
                updated_at
            FROM gateway_clients
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(items)
    }

    pub async fn create_gateway_client(
        &self,
        tenant_id: Uuid,
        name: &str,
        token_hash: &str,
        token_prefix: &str,
        created_by: Uuid,
    ) -> Result<GatewayClientRecord, RepositoryError> {
        let client = query_as::<_, GatewayClientRecord>(
            r#"
            INSERT INTO gateway_clients (
                tenant_id,
                name,
                token_hash,
                token_prefix,
                created_by
            )
            VALUES ($1, $2, $3, $4, $5)
            RETURNING
                id,
                tenant_id,
                name,
                token_prefix,
                is_enabled,
                last_used_at,
                created_at,
                updated_at
            "#,
        )
        .bind(tenant_id)
        .bind(name)
        .bind(token_hash)
        .bind(token_prefix)
        .bind(created_by)
        .fetch_one(&self.pool)
        .await?;

        Ok(client)
    }

    pub async fn authenticate_gateway_client(
        &self,
        token_hash: &str,
    ) -> Result<Option<GatewayClientAuthRecord>, RepositoryError> {
        let client = query_as::<_, GatewayClientAuthRecord>(
            r#"
            SELECT id, tenant_id, is_enabled
            FROM gateway_clients
            WHERE token_hash = $1
            LIMIT 1
            "#,
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(client) = client.as_ref().filter(|client| client.is_enabled) {
            sqlx::query(
                r#"
                UPDATE gateway_clients
                SET last_used_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(client.id)
            .execute(&self.pool)
            .await?;
        }

        Ok(client)
    }
}

#[derive(Debug, sqlx::FromRow)]
struct TenantWithRoleRow {
    id: Uuid,
    name: String,
    slug: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    role: String,
}
