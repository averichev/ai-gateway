use std::{net::SocketAddr, sync::Arc};

use sqlx::{
    ConnectOptions,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    app::{AppState, build_app},
    config::AppConfig,
    providers::{OpenAiCompatibleProvider, ProviderRegistry},
    repositories::{
        PostgresAuthRepository, PostgresRequestsRepository, PostgresRoutesRepository, seed_defaults,
    },
    security::SecretCrypto,
    usecases::{
        generate::{ContentPreviewPolicy, GenerateService},
        readiness::ReadinessService,
    },
};

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    init_tracing();

    let config = AppConfig::from_env()?;
    let connection_options = config
        .database_url
        .parse::<PgConnectOptions>()
        .map_err(|_| "invalid_database_url")?
        .disable_statement_logging();
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect_with(connection_options)
        .await
        .map_err(|_| "database_connection_failed")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|_| "database_migration_failed")?;
    seed_defaults(&pool, &config)
        .await
        .map_err(|_| "default_routes_storage_failed")?;
    let secret_crypto = SecretCrypto::from_base64(&config.gateway_master_key)?;

    let auth_repo = Arc::new(PostgresAuthRepository::new(pool.clone()));
    let routes_repo = Arc::new(PostgresRoutesRepository::new(pool.clone()));
    let requests_repo = Arc::new(PostgresRequestsRepository::new(pool));

    let provider_registry = ProviderRegistry::new(vec![Arc::new(OpenAiCompatibleProvider::new()?)]);
    let generate_service = Arc::new(GenerateService::new(
        routes_repo.clone(),
        requests_repo.clone(),
        provider_registry.clone(),
        secret_crypto.clone(),
        ContentPreviewPolicy {
            enabled: config.store_content_previews,
            request_chars: config.request_preview_chars,
            response_chars: config.response_preview_chars,
        },
    ));

    let state = AppState {
        generate_service,
        readiness_service: Arc::new(ReadinessService::new(
            routes_repo.clone(),
            provider_registry,
            secret_crypto.clone(),
        )),
        auth_repo,
        routes_repo,
        requests_repo,
        secret_crypto,
        admin_dist_dir: config.admin_dist_dir.clone(),
    };

    let app = build_app(state);
    let address: SocketAddr = format!("{}:{}", config.server_host, config.server_port).parse()?;
    let listener = TcpListener::bind(address).await?;

    info!(address = %address, "ai-gateway is listening");

    axum::serve(listener, app).await?;

    Ok(())
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info,sqlx=warn"));

    tracing_subscriber::registry()
        .with(env_filter)
        // Dependency debug events can include upstream URLs and database values.
        // Expose our structured diagnostics and the sanitized HTTP trace only.
        .with(tracing_subscriber::filter::filter_fn(
            safe_diagnostic_target,
        ))
        .with(fmt::layer().with_target(false).compact())
        .init();
}

pub fn safe_diagnostic_target(metadata: &tracing::Metadata<'_>) -> bool {
    metadata.target().starts_with("ai_gateway")
        || metadata.target().starts_with("tower_http::trace")
}
