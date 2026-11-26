use std::{future::IntoFuture, net::SocketAddr, path::Path, sync::Arc, time::Duration};

use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    app::{AppState, build_app},
    config::{self, AppConfig},
    providers::{OpenAiCompatibleProvider, ProviderRegistry},
    repositories::{AuthRepository, Database, RequestsRepository, RoutesRepository, seed_defaults},
    security::{SecretCrypto, master_key},
    updates::{Updates, VERSION},
    usecases::{
        generate::{ContentPreviewPolicy, GenerateService},
        readiness::ReadinessService,
    },
};

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().skip(1).eq(["--version"]) {
        println!("ai-gateway {VERSION}");
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    if crate::updates::linux::build_info().handle_cli()? {
        return Ok(());
    }
    dotenvy::dotenv().ok();
    let address = config::listen_address()?;
    let updates = Updates::launch(address).await?;
    init_tracing();

    let config = AppConfig::from_env()?;
    let configured_key = config
        .gateway_master_key
        .as_deref()
        .map(SecretCrypto::from_base64)
        .transpose()
        .map_err(|_| config::ConfigError::InvalidMasterKey)?;
    let pool = Database::initialize(&config.database_url).await?;
    let routes_repo = Arc::new(RoutesRepository::new(pool.clone()));
    let secret_crypto = match configured_key {
        Some(key) => key,
        None => {
            let has_provider_secrets = routes_repo
                .has_provider_secrets()
                .await
                .map_err(|_| "provider_secrets_storage_failed")?;
            master_key::load_or_create(Path::new("."), has_provider_secrets)?
        }
    };
    seed_defaults(&pool, &config)
        .await
        .map_err(|_| "default_routes_storage_failed")?;

    let auth_repo = Arc::new(AuthRepository::new(pool.clone()));
    let requests_repo = Arc::new(RequestsRepository::new(pool.clone()));

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
        updates: updates.clone(),
    };

    let app = build_app(state);
    let listener = TcpListener::bind(address).await?;

    info!(address = %listener.local_addr()?, "ai-gateway is listening");

    let (draining, drain_started) = tokio::sync::oneshot::channel();
    let shutdown_updates = updates.clone();
    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        tokio::select! {
            _ = shutdown_updates.shutdown_requested() => {},
            _ = termination_signal() => {},
        }
        let _ = draining.send(());
    })
    .into_future();
    tokio::pin!(server);
    tokio::select! {
        result = &mut server => result?,
        _ = drain_started => tokio::time::timeout(Duration::from_secs(60), &mut server).await??,
    }
    tokio::time::timeout(Duration::from_secs(10), pool.close()).await?;
    updates.finish_shutdown()?;
    Ok(())
}

async fn termination_signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("could not register SIGTERM handler");
        tokio::select! { _ = term.recv() => {}, _ = tokio::signal::ctrl_c() => {} }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c()
        .await
        .expect("could not register Ctrl-C handler");
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
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(false)
                .compact(),
        )
        .init();
}

pub fn safe_diagnostic_target(metadata: &tracing::Metadata<'_>) -> bool {
    metadata.target().starts_with("ai_gateway")
        || metadata.target().starts_with("tower_http::trace")
}
