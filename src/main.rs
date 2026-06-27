use load_balancer::cli::CliArgs;
use load_balancer::server::LoadBalancer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let args = CliArgs::parse();

    LoadBalancer::run_with_config(args.config).await?;
    Ok(())
}
