use crate::config::{BackendCandidate, BackendConfig};
use std::sync::{Arc, Mutex};

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

    #[cfg(test)]
    pub fn healthy_backends(&self) -> Vec<BackendConfig> {
        let state = self.inner.lock().unwrap();
        state
            .iter()
            .filter(|backend| backend.healthy)
            .map(|backend| backend.config.clone())
            .collect()
    }

    pub fn healthy_candidates(&self) -> Vec<BackendCandidate> {
        let state = self.inner.lock().unwrap();
        state
            .iter()
            .filter(|backend| backend.healthy)
            .map(|backend| BackendCandidate {
                backend: backend.config.clone(),
                active_connections: backend.active_connections,
            })
            .collect()
    }

    #[cfg(test)]
    pub fn mark_healthy(&self, backend_id: String) {
        let mut state = self.inner.lock().unwrap();
        for backend in state.iter_mut() {
            if backend.config.backend_id == backend_id {
                backend.healthy = true;
            }
        }
    }

    #[cfg(test)]
    pub fn mark_unhealthy(&self, backend_id: String) {
        let mut state = self.inner.lock().unwrap();
        for backend in state.iter_mut() {
            if backend.config.backend_id == backend_id {
                backend.healthy = false;
            }
        }
    }

    #[cfg(test)]
    pub fn increment_connections(&self, backend_id: String) {
        let mut state = self.inner.lock().unwrap();
        for backend in state.iter_mut() {
            if backend.config.backend_id == backend_id {
                backend.active_connections += 1;
            }
        }
    }

    #[cfg(test)]
    pub fn decrement_connections(&self, backend_id: String) {
        let mut state = self.inner.lock().unwrap();
        for backend in state.iter_mut() {
            if backend.config.backend_id == backend_id && backend.active_connections > 0 {
                backend.active_connections -= 1;
            }
        }
    }

    #[cfg(test)]
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

    #[test]
    fn healthy_candidates_include_active_connection_counts() {
        let pool = BackendPool::new(vec![backend(1, 8081), backend(2, 8082)]);

        let _first_guard = pool.try_acquire("1").unwrap();
        let _second_guard = pool.try_acquire("1").unwrap();
        let _third_guard = pool.try_acquire("2").unwrap();

        let candidates = pool.healthy_candidates();

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].backend.backend_id, "1");
        assert_eq!(candidates[0].active_connections, 2);
        assert_eq!(candidates[1].backend.backend_id, "2");
        assert_eq!(candidates[1].active_connections, 1);
    }

    #[test]
    fn try_acquire_increments_active_connections_until_guard_is_dropped() {
        let pool = BackendPool::new(vec![backend(1, 8081)]);

        assert_eq!(pool.active_connections("1"), Some(0));

        {
            let _guard = pool.try_acquire("1").unwrap();
            assert_eq!(pool.active_connections("1"), Some(1));
        }

        assert_eq!(pool.active_connections("1"), Some(0));
    }

    #[test]
    fn try_acquire_returns_none_for_unhealthy_backend() {
        let pool = BackendPool::new(vec![backend(1, 8081)]);

        pool.mark_unhealthy("1".to_string());
        let guard = pool.try_acquire("1");
        assert!(guard.is_none());
        assert_eq!(pool.active_connections("1"), Some(0));
    }

    #[test]
    fn try_acquire_returns_none_for_unknown_backend() {
        let pool = BackendPool::new(vec![backend(1, 8081)]);

        assert!(pool.try_acquire("missing").is_none());
    }
}
