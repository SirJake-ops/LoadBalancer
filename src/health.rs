use crate::config::{BackendConfig, HealthCheckConfig};
use crate::pool::BackendPool;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};

pub async fn is_backend_healthy(backend: &BackendConfig, timeout_duration: Duration) -> bool {
    matches!(
        timeout(
            timeout_duration,
            TcpStream::connect(backend.backend_address)
        )
        .await,
        Ok(Ok(_stream))
    )
}

pub fn spawn_health_checker(pool: BackendPool, config: HealthCheckConfig) -> JoinHandle<()> {
    tokio::spawn(async move {
        let interval_duration = Duration::from_secs(config.interval_seconds);
        let timeout_duration = Duration::from_secs(config.timeout_seconds);
        loop {
            let backends = pool.backends();
            for backend in backends {
                let healthy = is_backend_healthy(&backend, timeout_duration).await;
                if healthy {
                    pool.mark_healthy(backend.backend_id);
                } else {
                    pool.mark_unhealthy(backend.backend_id);
                }
            }
            sleep(interval_duration).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use crate::config::{BackendConfig, HealthCheckConfig};
    use crate::health::{is_backend_healthy, spawn_health_checker};
    use crate::pool::BackendPool;
    use std::time::Duration;
    use tokio::net::TcpListener;
    use tokio::time::sleep;

    #[tokio::test]
    async fn is_backend_healthy_returns_true_when_tcp_connect_succeeds() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend = BackendConfig {
            backend_address: listener.local_addr().unwrap(),
            backend_id: "backend-1".to_string(),
            weight: None,
        };

        let accept_task = tokio::spawn(async move {
            let _ = listener.accept().await.unwrap();
        });

        assert!(is_backend_healthy(&backend, Duration::from_secs(1)).await);
        accept_task.await.unwrap();
    }

    #[tokio::test]
    async fn is_backend_healthy_returns_false_when_tcp_connect_fails() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_addr = listener.local_addr().unwrap();
        drop(listener);

        let backend = BackendConfig {
            backend_address: backend_addr,
            backend_id: "backend-1".to_string(),
            weight: None,
        };

        assert!(!is_backend_healthy(&backend, Duration::from_secs(1)).await);
    }

    #[tokio::test]
    async fn health_checker_marks_backends_by_tcp_connectivity() {
        let healthy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let healthy_addr = healthy_listener.local_addr().unwrap();
        let unhealthy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let unhealthy_addr = unhealthy_listener.local_addr().unwrap();
        drop(unhealthy_listener);

        let accept_task = tokio::spawn(async move {
            loop {
                let _ = healthy_listener.accept().await.unwrap();
            }
        });

        let pool = BackendPool::new(vec![
            BackendConfig {
                backend_address: healthy_addr,
                backend_id: "healthy".to_string(),
                weight: None,
            },
            BackendConfig {
                backend_address: unhealthy_addr,
                backend_id: "unhealthy".to_string(),
                weight: None,
            },
        ]);

        let health_checker = spawn_health_checker(
            pool.clone(),
            HealthCheckConfig {
                interval_seconds: 1,
                timeout_seconds: 1,
            },
        );

        sleep(Duration::from_millis(50)).await;

        let healthy_backends = pool.healthy_backends();
        assert_eq!(healthy_backends.len(), 1);
        assert_eq!(healthy_backends[0].backend_id, "healthy");

        health_checker.abort();
        accept_task.abort();
    }
}
