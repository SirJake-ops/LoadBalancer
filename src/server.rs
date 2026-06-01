use crate::config::LoadBalancerConfig;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;

pub struct LoadBalancer;

impl LoadBalancer {
    pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
        Self::run_with_config("config.toml").await
    }

    pub async fn run_with_config(
        config_path: impl AsRef<Path>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let config = LoadBalancerConfig::from_file(config_path)?;
        let listener = TcpListener::bind(config.listener_address)?;
        println!("Listening on {}", listener.local_addr()?);

        loop {
            let (mut socket, _) = listener.accept()?;
            tokio::spawn(async move {
                let mut buf = vec![0; 1024];

                loop {
                    match socket.read(&mut buf) {
                        Ok(0) => return,
                        Ok(n) => {
                            println!("Received {} bytes", n);
                            if socket.write_all(&buf[0..n]).is_err() {
                                return;
                            }
                        }
                        Err(e) => println!("Error: {}", e),
                    }
                }
            });
        }
    }
}
