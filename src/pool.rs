use crate::config::BackendConfig;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct PoolState {
    pub active_connections: usize,
    pub max_connections: usize,
    pub total_connections: u64,
}

#[derive(Debug, Clone)]
pub struct ProxyPool {
    inner: Arc<Mutex<PoolState>>,
}

#[derive(Debug)]
pub struct BackendState {
    pub config: BackendConfig,
    pub healthy: bool,
    pub active_connections: usize,
}

#[derive(Debug, Clone)]
pub struct BackendPool {
    inner: Arc<Mutex<Vec<BackendState>>>,
}

impl ProxyPool {
    pub fn new() -> Self {
        Self::with_max_connections(1024)
    }

    pub fn with_max_connections(max_connections: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(PoolState {
                active_connections: 0,
                max_connections,
                total_connections: 0,
            })),
        }
    }

    pub fn get_snapshot(&self) -> (usize, usize, u64) {
        let state = self.inner.lock().unwrap();
        (
            state.active_connections,
            state.max_connections,
            state.total_connections,
        )
    }

    pub(crate) fn try_acquire(&self) -> Option<PoolGuard> {
        let mut state = self.inner.lock().unwrap();
        if state.active_connections < state.max_connections {
            state.active_connections += 1;
            state.total_connections += 1;
            Some(PoolGuard {
                inner: self.inner.clone(),
            })
        } else {
            None
        }
    }
}

impl BackendPool {
    pub fn new(backends: Vec<BackendConfig>) -> Self {
        let mut state = Vec::new();
        for backend in backends {
            state.push(BackendState {
                config: backend,
                healthy: true,
                active_connections: 0,
            });
        }

        Self {
            inner: Arc::new(Mutex::new(state)),
        }
    }

    pub fn healthy_backends(&self) -> Vec<BackendConfig> {
        let state = self.inner.lock().unwrap();
        state
            .iter()
            .filter(|backend| backend.healthy)
            .map(|backend| backend.config.clone())
            .collect()
    }

    pub fn mark_healthy(&self, backend_id: String) {
        let mut state = self.inner.lock().unwrap();
        for backend in state.iter_mut() {
            if backend.config.backend_id == backend_id {
                backend.healthy = true;
            }
        }
    }

    pub fn mark_unhealthy(&self, backend_id: String) {
        let mut state = self.inner.lock().unwrap();
        for backend in state.iter_mut() {
            if backend.config.backend_id == backend_id {
                backend.healthy = false;
            }
        }
    }

    pub fn increment_connections(&self, backend_id: String) {
        let mut state = self.inner.lock().unwrap();
        for backend in state.iter_mut() {
            if backend.config.backend_id == backend_id {
                backend.active_connections += 1;
            }
        }
    }

    pub fn decrement_connections(&self, backend_id: String) {
        let mut state = self.inner.lock().unwrap();
        for backend in state.iter_mut() {
            if backend.config.backend_id == backend_id && backend.active_connections > 0 {
                backend.active_connections -= 1;
            }
        }
    }

    pub fn active_connections(&self, backend_id: &str) -> Option<usize> {
        let state = self.inner.lock().unwrap();
        state
            .iter()
            .find(|backend| backend.config.backend_id == backend_id)
            .map(|backend| backend.active_connections)
    }

    pub fn try_acquire(&self, backend_id: &str) -> Option<BackendGuard> {
        let mut state = self.inner.lock().unwrap();
        let backend = state
            .iter_mut()
            .find(|backend| backend.config.backend_id == backend_id)?;

        if !backend.healthy {
            return None;
        }

        backend.active_connections += 1;

        Some(BackendGuard {
            inner: self.inner.clone(),
            backend_id: backend.config.backend_id.clone(),
        })
    }
}

pub struct PoolGuard {
    inner: Arc<Mutex<PoolState>>,
}

impl Drop for PoolGuard {
    fn drop(&mut self) {
        let mut state = self.inner.lock().unwrap();
        if state.active_connections > 0 {
            state.active_connections -= 1;
        }
    }
}

pub struct BackendGuard {
    inner: Arc<Mutex<Vec<BackendState>>>,
    backend_id: String,
}

impl Drop for BackendGuard {
    fn drop(&mut self) {
        let mut state = self.inner.lock().unwrap();

        if let Some(backend) = state
            .iter_mut()
            .find(|backend| backend.config.backend_id == self.backend_id)
        {
            if backend.active_connections > 0 {
                backend.active_connections -= 1;
            }
        } else {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend(id: u64, port: u16) -> BackendConfig {
        BackendConfig::new(format!("127.0.0.1:{port}"), id, None)
    }

    #[test]
    fn empty_pool_returns_no_healthy_backends() {
        let pool = BackendPool::new(Vec::new());

        assert!(pool.healthy_backends().is_empty());
    }

    #[test]
    fn new_pool_marks_configured_backends_as_healthy() {
        let pool = BackendPool::new(vec![backend(1, 8081), backend(2, 8082)]);

        let healthy_backends = pool.healthy_backends();

        assert_eq!(healthy_backends.len(), 2);
        assert_eq!(healthy_backends[0].backend_id, "1");
        assert_eq!(healthy_backends[1].backend_id, "2");
    }

    #[test]
    fn unhealthy_backend_is_excluded_from_healthy_backends() {
        let pool = BackendPool::new(vec![backend(1, 8081), backend(2, 8082)]);

        pool.mark_unhealthy("1".to_string());

        let healthy_backends = pool.healthy_backends();

        assert_eq!(healthy_backends.len(), 1);
        assert_eq!(healthy_backends[0].backend_id, "2");
    }

    #[test]
    fn backend_can_be_marked_healthy_after_being_unhealthy() {
        let pool = BackendPool::new(vec![backend(1, 8081)]);

        pool.mark_unhealthy("1".to_string());
        assert!(pool.healthy_backends().is_empty());

        pool.mark_healthy("1".to_string());

        let healthy_backends = pool.healthy_backends();
        assert_eq!(healthy_backends.len(), 1);
        assert_eq!(healthy_backends[0].backend_id, "1");
    }

    #[test]
    fn active_connection_count_can_increment_and_decrement() {
        let pool = BackendPool::new(vec![backend(1, 8081)]);

        assert_eq!(pool.active_connections("1"), Some(0));

        pool.increment_connections("1".to_string());
        pool.increment_connections("1".to_string());
        assert_eq!(pool.active_connections("1"), Some(2));

        pool.decrement_connections("1".to_string());
        assert_eq!(pool.active_connections("1"), Some(1));
    }

    #[test]
    fn decrementing_zero_active_connections_does_not_underflow() {
        let pool = BackendPool::new(vec![backend(1, 8081)]);

        pool.decrement_connections("1".to_string());

        assert_eq!(pool.active_connections("1"), Some(0));
    }
}
