#[tokio::main]
async fn main() {
    if let Err(error) = ai_gateway::ctl::run_from_env().await {
        ai_gateway::ctl::print_fatal(&error);
        std::process::exit(1);
    }
}
