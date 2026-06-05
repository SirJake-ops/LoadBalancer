use crate::config::LoadBalancerConfig;
use crate::pool::BackendPool;
use crate::proxy::proxy_connection;
use std::future::Future;
use std::path::Path;
use tokio::net::TcpListener;
use tokio::task::JoinSet;

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

        if config.backend_list.is_empty() {
            return Err("config must include at least one backend".into());
        }

        let backend_pool = BackendPool::new(config.backend_list);

        println!("Listening on {}", listener.local_addr()?);
        Self::serve(listener, backend_pool).await
    }

    pub(crate) async fn serve_until_shutdown(
        listener: TcpListener,
        pool: BackendPool,
        shutdown: impl Future<Output = std::io::Result<()>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        tokio::pin!(shutdown);

        let mut tasks = JoinSet::new();

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((socket, client_addr)) => {
                            let healthy_backends = pool.healthy_backends();

                            if healthy_backends.is_empty() {
                                println!("No healthy backends found");
                                continue;
                            }

                            let backend = healthy_backends.first().unwrap().clone();
                            println!("Accepted connection from {}", client_addr);

                            match pool.try_acquire(&backend.backend_id) {
                                Some(guard) => {
                                    tasks.spawn(async move {
                                        let _pool_slot = guard;

                                        println!("Proxy connection started for {}", client_addr);

                                        if let Err(e) = proxy_connection(socket, backend).await {
                                            eprintln!("Proxy connection error for {}: {}", client_addr, e);
                                        }

                                        println!("Proxy connection ended for {}", client_addr);
                                    });
                                }
                                None => {
                                    eprintln!("Backend unavailable! Rejecting client connection from {}", client_addr);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to accept incoming connection: {}", e);
                        }
                    }
                }
                shutdown_result = &mut shutdown => {
                    shutdown_result?;
                    println!("Shutdown signal received; stopping listener");
                    break;
                }
            }
        }

        while let Some(result) = tasks.join_next().await {
            result?;
        }

        Ok(())
    }

    pub(crate) async fn serve(
        listener: TcpListener,
        pool: BackendPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Self::serve_until_shutdown(listener, pool, tokio::signal::ctrl_c()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BackendConfig;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::oneshot;
    use tokio::time::{Duration, timeout};

    fn backend(id: u64, port: u16) -> BackendConfig {
        BackendConfig::new(format!("127.0.0.1:{port}"), id, None)
    }

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

        let pool = BackendPool::new(vec![backend.clone()]);
        let server_task = tokio::spawn(async move {
            let _ = LoadBalancer::serve(server_listener, pool).await;
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

    #[tokio::test]
    async fn test_server_handles_multiple_connections() {
        let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_addr = backend_listener.local_addr().unwrap();

        tokio::spawn(async move {
            loop {
                let (mut backend_socket, _) = backend_listener.accept().await.unwrap();

                tokio::spawn(async move {
                    let mut buffer = [0; 1024];
                    let bytes_read = backend_socket.read(&mut buffer).await.unwrap();
                    backend_socket
                        .write_all(&buffer[..bytes_read])
                        .await
                        .unwrap();
                });
            }
        });

        let server_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();
        let backend = BackendConfig {
            backend_address: backend_addr,
            backend_id: "backend-1".to_string(),
            weight: None,
        };

        let pool = BackendPool::new(vec![backend.clone()]);
        let server_task = tokio::spawn(async move {
            let _ = LoadBalancer::serve(server_listener, pool).await;
        });

        let test_result = timeout(Duration::from_secs(2), async {
            let first_client = async {
                let mut client = TcpStream::connect(server_addr).await.unwrap();
                client.write_all(b"ping").await.unwrap();

                let mut response = [0; 4];
                client.read_exact(&mut response).await.unwrap();
                assert_eq!(&response, b"ping");
            };

            let second_client = async {
                let mut client = TcpStream::connect(server_addr).await.unwrap();
                client.write_all(b"pong").await.unwrap();

                let mut response = [0; 4];
                client.read_exact(&mut response).await.unwrap();
                assert_eq!(&response, b"pong");
            };

            tokio::join!(first_client, second_client);
        })
        .await;

        server_task.abort();
        test_result.unwrap();
    }

    #[tokio::test]
    async fn test_server_stops_accepting_on_shutdown() {
        let server_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();
        let backend = BackendConfig {
            backend_address: "127.0.0.1:1".parse().unwrap(),
            backend_id: "backend-1".to_string(),
            weight: None,
        };
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let shutdown_driver = async {
            TcpStream::connect(server_addr).await.unwrap();
            shutdown_tx.send(()).unwrap();
        };

        let pool = BackendPool::new(vec![backend.clone()]);
        let server = LoadBalancer::serve_until_shutdown(server_listener, pool, async {
            shutdown_rx.await.map_err(std::io::Error::other)
        });

        let shutdown_result = timeout(Duration::from_secs(2), async {
            tokio::join!(server, shutdown_driver).0
        })
        .await
        .unwrap();

        assert!(shutdown_result.is_ok());
    }
}
