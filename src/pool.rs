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
