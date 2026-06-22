use std::time::Duration;
use crate::config::{BackendConfig, HealthCheckConfig};
use tokio::net::TcpStream;
use tokio::time::{timeout, sleep, interval};
use tokio::task::JoinHandle;
use crate::pool::BackendPool;

pub async fn is_backend_healthy(backend: &BackendConfig, timeout_duration: Duration) -> bool {
    matches!(
        timeout(timeout_duration, TcpStream::connect(backend.backend_address)).await,
        Ok(Ok(_stream))
    )
}

pub fn spawn_health_checker(
    pool: BackendPool,
    config: HealthCheckConfig
) -> JoinHandle<()>{
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