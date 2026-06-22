use crate::balancing::{BalanceError, BalancingStrategy, LeastConnections, RoundRobin};
use crate::config::{BackendConfig, LoadBalancerConfig, StrategyKind};
use crate::pool::BackendPool;
use crate::proxy::proxy_connection;
use std::future::Future;
use std::path::Path;
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use crate::health;

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

        let strategy = Self::strategy_for(config.strategy);
        let backend_pool = BackendPool::new(config.backend_list);

        let _health_checker = health::spawn_health_checker(backend_pool.clone(), config.health_check_interval);

        println!("Listening on {}", listener.local_addr()?);
        Self::serve_with_strategy(listener, backend_pool, strategy).await
    }

    fn strategy_for(strategy: StrategyKind) -> Box<dyn BalancingStrategy> {
        match strategy {
            StrategyKind::RoundRobin => Box::new(RoundRobin::new()),
            StrategyKind::LeastConnections => Box::new(LeastConnections::new()),
        }
    }

    #[cfg(test)]
    pub(crate) async fn serve_until_shutdown(
        listener: TcpListener,
        pool: BackendPool,
        shutdown: impl Future<Output = std::io::Result<()>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Self::serve_with_strategy_until_shutdown(
            listener,
            pool,
            Box::new(RoundRobin::new()),
            shutdown,
        )
        .await
    }

    pub(crate) async fn serve_with_strategy_until_shutdown(
        listener: TcpListener,
        pool: BackendPool,
        mut strategy: Box<dyn BalancingStrategy>,
        shutdown: impl Future<Output = std::io::Result<()>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        tokio::pin!(shutdown);

        let mut tasks = JoinSet::new();

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((socket, client_addr)) => {

                            let backend = match Self::select_backend_for_client(&pool, strategy.as_mut()) {
                                Ok(backend) => backend,
                                Err(BalanceError::NoBackendsAvailable) => {
                                    eprintln!("No healthy backends; rejecting client {}", client_addr);
                                    continue;
                                }
                            };

                            println!(
                                "Accepted connection from {}; selected backend {}",
                                client_addr, backend.backend_id
                            );

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

    pub(crate) async fn serve_with_strategy(
        listener: TcpListener,
        pool: BackendPool,
        strategy: Box<dyn BalancingStrategy>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Self::serve_with_strategy_until_shutdown(listener, pool, strategy, tokio::signal::ctrl_c())
            .await
    }

    #[cfg(test)]
    pub(crate) async fn serve(
        listener: TcpListener,
        pool: BackendPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Self::serve_with_strategy(listener, pool, Box::new(RoundRobin::new())).await
    }

    fn select_backend_for_client(
        pool: &BackendPool,
        strategy: &mut dyn BalancingStrategy,
    ) -> Result<BackendConfig, BalanceError> {
        let candidates = pool.healthy_candidates();
        strategy.select_backend(&candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BackendCandidate, BackendConfig};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::oneshot;
    use tokio::time::{Duration, timeout};

    fn backend(id: u64, port: u16) -> BackendConfig {
        BackendConfig::new(format!("127.0.0.1:{port}"), id, None)
    }

    #[test]
    fn configured_strategy_controls_selection_logic() {
        let candidates = vec![
            BackendCandidate {
                backend: backend(1, 8081),
                active_connections: 10,
            },
            BackendCandidate {
                backend: backend(2, 8082),
                active_connections: 1,
            },
        ];

        let mut round_robin = LoadBalancer::strategy_for(StrategyKind::RoundRobin);
        assert_eq!(
            round_robin.select_backend(&candidates).unwrap().backend_id,
            "1"
        );

        let mut least_connections = LoadBalancer::strategy_for(StrategyKind::LeastConnections);
        assert_eq!(
            least_connections
                .select_backend(&candidates)
                .unwrap()
                .backend_id,
            "2"
        );
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
    async fn test_server_routes_connections_round_robin_across_backends() {
        let first_backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let first_backend_addr = first_backend_listener.local_addr().unwrap();
        let second_backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let second_backend_addr = second_backend_listener.local_addr().unwrap();

        async fn respond_once(listener: TcpListener, response: &'static [u8]) {
            let (mut backend_socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 1024];
            let _ = backend_socket.read(&mut buffer).await.unwrap();
            backend_socket.write_all(response).await.unwrap();
        }

        tokio::spawn(respond_once(first_backend_listener, b"one"));
        tokio::spawn(respond_once(second_backend_listener, b"two"));

        let server_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();
        let pool = BackendPool::new(vec![
            BackendConfig {
                backend_address: first_backend_addr,
                backend_id: "backend-1".to_string(),
                weight: None,
            },
            BackendConfig {
                backend_address: second_backend_addr,
                backend_id: "backend-2".to_string(),
                weight: None,
            },
        ]);

        let server_task = tokio::spawn(async move {
            let _ = LoadBalancer::serve(server_listener, pool).await;
        });

        let test_result = timeout(Duration::from_secs(2), async {
            let mut first_client = TcpStream::connect(server_addr).await.unwrap();
            first_client.write_all(b"ping").await.unwrap();
            let mut first_response = [0; 3];
            first_client.read_exact(&mut first_response).await.unwrap();
            assert_eq!(&first_response, b"one");

            let mut second_client = TcpStream::connect(server_addr).await.unwrap();
            second_client.write_all(b"ping").await.unwrap();
            let mut second_response = [0; 3];
            second_client
                .read_exact(&mut second_response)
                .await
                .unwrap();
            assert_eq!(&second_response, b"two");
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
    async fn test_server_rejects_client_when_no_backend_is_healthy_and_keeps_serving() {
        let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_addr = backend_listener.local_addr().unwrap();
        let backend = BackendConfig {
            backend_address: backend_addr,
            backend_id: "backend-1".to_string(),
            weight: None,
        };
        let pool = BackendPool::new(vec![backend]);
        pool.mark_unhealthy("backend-1".to_string());

        let server_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let server_pool = pool.clone();
        let server = tokio::spawn(async move {
            let _ = LoadBalancer::serve_until_shutdown(server_listener, server_pool, async {
                shutdown_rx.await.map_err(std::io::Error::other)
            })
            .await;
        });

        timeout(Duration::from_secs(2), async {
            let mut rejected_client = TcpStream::connect(server_addr).await.unwrap();
            let mut response = [0; 1];
            let bytes_read = rejected_client.read(&mut response).await.unwrap();
            assert_eq!(bytes_read, 0);
        })
        .await
        .unwrap();

        pool.mark_healthy("backend-1".to_string());
        tokio::spawn(async move {
            let (mut backend_socket, _) = backend_listener.accept().await.unwrap();
            let mut buffer = [0; 4];
            backend_socket.read_exact(&mut buffer).await.unwrap();
            backend_socket.write_all(&buffer).await.unwrap();
        });

        timeout(Duration::from_secs(2), async {
            let mut client = TcpStream::connect(server_addr).await.unwrap();
            client.write_all(b"ping").await.unwrap();

            let mut response = [0; 4];
            client.read_exact(&mut response).await.unwrap();
            assert_eq!(&response, b"ping");
        })
        .await
        .unwrap();

        shutdown_tx.send(()).unwrap();
        timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap();
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
