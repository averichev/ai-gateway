use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match ai_gateway::server::run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}
