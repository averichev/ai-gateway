use std::time::Duration;

use sqlx::{
    ConnectOptions, PgPool, SqlitePool,
    migrate::Migrator,
    postgres::{PgConnectOptions, PgPoolOptions},
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};

pub(crate) static POSTGRES_MIGRATOR: Migrator = sqlx::migrate!("./migrations");
pub(crate) static SQLITE_MIGRATOR: Migrator = sqlx::migrate!("./migrations/sqlite");

#[derive(Clone)]
pub enum Database {
    Postgres(PgPool),
    Sqlite(SqlitePool),
}

impl From<PgPool> for Database {
    fn from(pool: PgPool) -> Self {
        Self::Postgres(pool)
    }
}

impl From<SqlitePool> for Database {
    fn from(pool: SqlitePool) -> Self {
        Self::Sqlite(pool)
    }
}

impl Database {
    pub async fn initialize(url: &str) -> Result<Self, &'static str> {
        let database = Self::connect(url, true).await?;
        match &database {
            Self::Postgres(pool) => POSTGRES_MIGRATOR.run(pool).await,
            Self::Sqlite(pool) => SQLITE_MIGRATOR.run(pool).await,
        }
        .map_err(|_| "database_migration_failed")?;
        Ok(database)
    }

    pub async fn open_existing(url: &str) -> Result<Self, &'static str> {
        Self::connect(url, false).await
    }

    async fn connect(url: &str, create_if_missing: bool) -> Result<Self, &'static str> {
        if url.starts_with("sqlite:") {
            let mut options = url
                .parse::<SqliteConnectOptions>()
                .map_err(|_| "invalid_database_url")?
                .create_if_missing(create_if_missing)
                .foreign_keys(true)
                .busy_timeout(Duration::from_secs(10))
                .disable_statement_logging();
            if create_if_missing {
                options = options.journal_mode(SqliteJournalMode::Wal);
            }
            return SqlitePoolOptions::new()
                .max_connections(5)
                .connect_with(options)
                .await
                .map(Self::Sqlite)
                .map_err(|_| "database_connection_failed");
        }
        if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
            return Err("invalid_database_url");
        }
        let options = url
            .parse::<PgConnectOptions>()
            .map_err(|_| "invalid_database_url")?
            .disable_statement_logging();
        PgPoolOptions::new()
            .max_connections(10)
            .connect_with(options)
            .await
            .map(Self::Postgres)
            .map_err(|_| "database_connection_failed")
    }

    pub async fn close(&self) {
        match self {
            Self::Postgres(pool) => pool.close().await,
            Self::Sqlite(pool) => pool.close().await,
        }
    }
}
