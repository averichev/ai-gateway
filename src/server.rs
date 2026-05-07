use std::{net::SocketAddr, sync::Arc};

use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    app::{AppState, build_app},
    config::AppConfig,
    providers::{OpenAiCompatibleProvider, ProviderRegistry},
    repositories::{PostgresRequestsRepository, PostgresRoutesRepository, seed_defaults},
    usecases::generate::GenerateService,
};

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    init_tracing();

    let config = AppConfig::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    seed_defaults(&pool, &config).await?;

    let routes_repo = Arc::new(PostgresRoutesRepository::new(pool.clone()));
    let requests_repo = Arc::new(PostgresRequestsRepository::new(pool));

    let provider_registry = ProviderRegistry::new(vec![Arc::new(OpenAiCompatibleProvider::new()?)]);
    let generate_service = Arc::new(GenerateService::new(
        routes_repo.clone(),
        requests_repo.clone(),
        provider_registry,
        config.request_preview_chars,
        config.response_preview_chars,
    ));

    let state = AppState {
        generate_service,
        routes_repo,
        requests_repo,
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
        .with(fmt::layer().with_target(false).compact())
        .init();
}
