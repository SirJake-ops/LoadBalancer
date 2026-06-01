use crate::config::{BackendConfig, LoadBalancerConfig};
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
        Self::serve(listener, backend).await
    }

    async fn serve(
        listener: TcpListener,
        backend: BackendConfig,
    ) -> Result<(), Box<dyn std::error::Error>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::time::{Duration, timeout};

    #[tokio::test]
    async fn test_server_proxies_to_first_backend() {
        let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_addr = backend_listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut backend_socket, _) = backend_listener.accept().await.unwrap();
            let mut buffer = [0; 1024];
            let bytes_read = backend_socket.read(&mut buffer).await.unwrap();
            backend_socket
                .write_all(&buffer[..bytes_read])
                .await
                .unwrap();
        });

        let server_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();
        let backend = BackendConfig {
            backend_address: backend_addr,
            backend_id: "backend-1".to_string(),
            weight: None,
        };

        let server_task = tokio::spawn(async move {
            let _ = LoadBalancer::serve(server_listener, backend).await;
        });

        let test_result = timeout(Duration::from_secs(2), async {
            let mut client = TcpStream::connect(server_addr).await.unwrap();
            client.write_all(b"ping").await.unwrap();

            let mut response = [0; 4];
            client.read_exact(&mut response).await.unwrap();

            assert_eq!(&response, b"ping");
        })
        .await;

        server_task.abort();
        test_result.unwrap();
    }
}
