use crate::config::LoadBalancerConfig;
use crate::proxy::proxy_connection;
use std::path::Path;
use tokio::net::TcpListener;

pub struct LoadBalancer;

impl LoadBalancer {
    pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
        Self::run_with_config("config.toml").await
    }

    pub async fn run_with_config(
        config_path: impl AsRef<Path>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let config = LoadBalancerConfig::from_file(config_path)?;
        let listener = TcpListener::bind(config.listener_address).await?;
        let backend = config
            .backend_list
            .first()
            .ok_or("config must include at least one backend")?
            .clone();

        println!("Listening on {}", listener.local_addr()?);

        loop {
            let (socket, _) = listener.accept().await?;
            let backend = backend.clone();

            tokio::spawn(async move {
                if let Err(e) = proxy_connection(socket, backend).await {
                    eprintln!("Proxy connection error: {}", e);
                }
            });
        }
    }
}
