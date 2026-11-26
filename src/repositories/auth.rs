use chrono::{DateTime, Utc};
use sqlx::query_as;
use uuid::Uuid;

use crate::domain::{
    AdminUserRecord, GatewayClientAuthRecord, GatewayClientRecord, SessionRecord,
    TenantAccessRecord, TenantRecord, UserRecord,
};

use super::{Database, RepositoryError};

#[derive(Clone)]
pub struct AuthRepository {
    pool: Database,
}

impl AuthRepository {
    pub fn new(pool: impl Into<Database>) -> Self {
        Self { pool: pool.into() }
    }

    pub async fn has_users(&self) -> Result<bool, RepositoryError> {
        let exists: bool = with_pool!(&self.pool, |pool| {
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users LIMIT 1)")
                .fetch_one(pool)
                .await
        })?;

        Ok(exists)
    }

    pub async fn create_owner(
        &self,
        email: &str,
        password_hash: &str,
        tenant_name: &str,
        tenant_slug: &str,
    ) -> Result<(UserRecord, TenantRecord), RepositoryError> {
        with_pool!(&self.pool, |pool| {
            let mut tx = pool.begin().await?;

            let user = query_as::<_, UserRecord>(
                r#"
                INSERT INTO users (id, email, password_hash, global_role)
                SELECT $1, $2, $3, 'owner' WHERE NOT EXISTS (SELECT 1 FROM users)
                RETURNING id, email, password_hash, global_role, is_enabled
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(email)
            .bind(password_hash)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| {
                RepositoryError::Conflict("bootstrap registration is already closed".to_owned())
            })?;

            let tenant = query_as::<_, TenantRecord>(
                r#"
                INSERT INTO tenants (id, name, slug, created_by)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (slug) DO UPDATE
                SET
                    name = EXCLUDED.name,
                    updated_at = CURRENT_TIMESTAMP
                RETURNING id, name, slug, created_at, updated_at
                "#,
            )
            .bind(Uuid::new_v4())
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
        })
    }

    pub async fn find_user_by_email(
        &self,
        email: &str,
    ) -> Result<Option<UserRecord>, RepositoryError> {
        let user = with_pool!(&self.pool, |pool| {
            query_as::<_, UserRecord>(
                r#"
                SELECT id, email, password_hash, global_role, is_enabled
                FROM users
                WHERE LOWER(email) = LOWER($1)
                LIMIT 1
                "#,
            )
            .bind(email)
            .fetch_optional(pool)
            .await
        })?;

        Ok(user)
    }

    pub async fn create_session(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), RepositoryError> {
        with_pool!(&self.pool, |pool| {
            sqlx::query(
                r#"
                INSERT INTO admin_sessions (id, user_id, token_hash, expires_at)
                VALUES ($1, $2, $3, $4)
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(user_id)
            .bind(token_hash)
            .bind(expires_at)
            .execute(pool)
            .await?;
        });

        Ok(())
    }

    pub async fn get_session(
        &self,
        token_hash: &str,
    ) -> Result<Option<SessionRecord>, RepositoryError> {
        let session = with_pool!(&self.pool, |pool| {
            query_as::<_, SessionRecord>(
                r#"
                SELECT
                    u.id AS user_id,
                    u.email,
                    u.global_role,
                    u.is_enabled
                FROM admin_sessions s
                INNER JOIN users u ON u.id = s.user_id
                WHERE s.token_hash = $1
                  AND s.expires_at > $2
                LIMIT 1
                "#,
            )
            .bind(token_hash)
            .bind(Utc::now())
            .fetch_optional(pool)
            .await
        })?;

        Ok(session)
    }

    pub async fn list_tenants_for_user(
        &self,
        user_id: Uuid,
        global_role: &str,
    ) -> Result<Vec<(TenantRecord, String)>, RepositoryError> {
        if global_role == "owner" {
            let tenants = with_pool!(&self.pool, |pool| {
                query_as::<_, TenantRecord>(
                    r#"
                    SELECT id, name, slug, created_at, updated_at
                    FROM tenants
                    ORDER BY name ASC
                    "#,
                )
                .fetch_all(pool)
                .await
            })?;

            return Ok(tenants
                .into_iter()
                .map(|tenant| (tenant, "owner".to_owned()))
                .collect());
        }

        let rows = with_pool!(&self.pool, |pool| {
            sqlx::query_as::<_, TenantWithRoleRow>(
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
            .fetch_all(pool)
            .await
        })?;

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
            let exists: bool = with_pool!(&self.pool, |pool| {
                sqlx::query_scalar(
                    r#"
                    SELECT EXISTS (
                        SELECT 1
                        FROM tenants
                        WHERE id = $1
                    )
                    "#,
                )
                .bind(tenant_id)
                .fetch_one(pool)
                .await
            })?;

            return Ok(exists.then_some(TenantAccessRecord {
                tenant_id,
                role: "owner".to_owned(),
            }));
        }

        let access = with_pool!(&self.pool, |pool| {
            query_as::<_, TenantAccessRecord>(
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
            .fetch_optional(pool)
            .await
        })?;

        Ok(access)
    }

    pub async fn create_tenant(
        &self,
        name: &str,
        slug: &str,
        created_by: Uuid,
    ) -> Result<TenantRecord, RepositoryError> {
        with_pool!(&self.pool, |pool| {
            let mut tx = pool.begin().await?;

            let tenant = query_as::<_, TenantRecord>(
                r#"
                INSERT INTO tenants (id, name, slug, created_by)
                VALUES ($1, $2, $3, $4)
                RETURNING id, name, slug, created_at, updated_at
                "#,
            )
            .bind(Uuid::new_v4())
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
        })
    }

    pub async fn list_users(&self) -> Result<Vec<AdminUserRecord>, RepositoryError> {
        let users = with_pool!(&self.pool, |pool| {
            query_as::<_, AdminUserRecord>(
                r#"
                SELECT
                    u.id,
                    u.email,
                    u.global_role,
                    u.is_enabled,
                    COUNT(tm.tenant_id) AS tenant_count,
                    u.created_at,
                    u.updated_at
                FROM users u
                LEFT JOIN tenant_members tm ON tm.user_id = u.id
                GROUP BY u.id
                ORDER BY u.created_at DESC
                "#,
            )
            .fetch_all(pool)
            .await
        })?;

        Ok(users)
    }

    pub async fn create_user(
        &self,
        email: &str,
        password_hash: &str,
        tenant_id: Option<Uuid>,
        tenant_role: Option<&str>,
    ) -> Result<AdminUserRecord, RepositoryError> {
        let user_id = with_pool!(&self.pool, |pool| {
            let mut tx = pool.begin().await?;

            let user_id: Uuid = sqlx::query_scalar(
                r#"
                INSERT INTO users (id, email, password_hash, global_role)
                VALUES ($1, $2, $3, 'user')
                RETURNING id
                "#,
            )
            .bind(Uuid::new_v4())
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
                .bind(user_id)
                .bind(tenant_role)
                .execute(&mut *tx)
                .await?;
            }

            tx.commit().await?;
            user_id
        });

        self.get_admin_user(user_id)
            .await?
            .ok_or(RepositoryError::NotFound)
    }

    async fn get_admin_user(
        &self,
        user_id: Uuid,
    ) -> Result<Option<AdminUserRecord>, RepositoryError> {
        let user = with_pool!(&self.pool, |pool| {
            query_as::<_, AdminUserRecord>(
                r#"
                SELECT
                    u.id,
                    u.email,
                    u.global_role,
                    u.is_enabled,
                    COUNT(tm.tenant_id) AS tenant_count,
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
            .fetch_optional(pool)
            .await
        })?;

        Ok(user)
    }

    pub async fn list_gateway_clients(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<GatewayClientRecord>, RepositoryError> {
        let items = with_pool!(&self.pool, |pool| {
            query_as::<_, GatewayClientRecord>(
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
            .fetch_all(pool)
            .await
        })?;

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
        let client = with_pool!(&self.pool, |pool| {
            query_as::<_, GatewayClientRecord>(
                r#"
                INSERT INTO gateway_clients (
                    id,
                    tenant_id,
                    name,
                    token_hash,
                    token_prefix,
                    created_by
                )
                VALUES ($1, $2, $3, $4, $5, $6)
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
            .bind(Uuid::new_v4())
            .bind(tenant_id)
            .bind(name)
            .bind(token_hash)
            .bind(token_prefix)
            .bind(created_by)
            .fetch_one(pool)
            .await
        })?;

        Ok(client)
    }

    pub async fn authenticate_gateway_client(
        &self,
        token_hash: &str,
    ) -> Result<Option<GatewayClientAuthRecord>, RepositoryError> {
        let client = with_pool!(&self.pool, |pool| {
            query_as::<_, GatewayClientAuthRecord>(
                r#"
                SELECT id, tenant_id, is_enabled
                FROM gateway_clients
                WHERE token_hash = $1
                LIMIT 1
                "#,
            )
            .bind(token_hash)
            .fetch_optional(pool)
            .await
        })?;

        if let Some(client) = client.as_ref().filter(|client| client.is_enabled) {
            with_pool!(&self.pool, |pool| {
                sqlx::query(
                    r#"
                    UPDATE gateway_clients
                    SET last_used_at = CURRENT_TIMESTAMP
                    WHERE id = $1
                    "#,
                )
                .bind(client.id)
                .execute(pool)
                .await?;
            });
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
