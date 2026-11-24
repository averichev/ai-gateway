use ai_gateway::repositories::PostgresRequestsRepository;
use clap::Parser;
use sqlx::{ConnectOptions, postgres::PgConnectOptions, postgres::PgPoolOptions};
use uuid::Uuid;

/// Count legacy content previews; clear them only with --apply.
/// Back up the database first. Clearing is irreversible without a backup.
/// Request history, routing, usage, status and latency are retained.
#[derive(Parser)]
struct Args {
    /// Explicitly clear previews. Without this flag, only print the count.
    #[arg(long)]
    apply: bool,
    /// Limit the operation to one tenant. Omit to select all tenants.
    #[arg(long)]
    tenant_id: Option<Uuid>,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let args = Args::parse();
    dotenvy::dotenv().ok();
    match run(args).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(category) => {
            // No database errors, connection URLs, credentials or row data.
            eprintln!("preview_cleanup_failed: {category}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run(args: Args) -> Result<(), &'static str> {
    let url = std::env::var("DATABASE_URL").map_err(|_| "missing_database_url")?;
    let options = url
        .parse::<PgConnectOptions>()
        .map_err(|_| "invalid_database_url")?
        .disable_statement_logging();
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|_| "database_connection")?;
    let repo = PostgresRequestsRepository::new(pool);
    let count = repo
        .count_content_previews(args.tenant_id)
        .await
        .map_err(|_| "storage_error")?;
    println!("records_with_previews: {count}");
    if args.apply {
        let cleared = repo
            .clear_content_previews(args.tenant_id)
            .await
            .map_err(|_| "storage_error")?;
        println!("records_cleared: {cleared}");
    } else {
        println!("dry_run: true; back up the database, then pass --apply to clear previews");
    }
    Ok(())
}
