use crate::balancing::{BalanceError, BalancingStrategy};
use crate::config::{BackendCandidate, BackendConfig};

#[derive(Debug, Default)]
pub struct LeastConnections;

impl LeastConnections {
    pub fn new() -> Self {
        Self
    }
}

impl BalancingStrategy for LeastConnections {
    fn select_backend(
        &mut self,
        candidates: &[BackendCandidate],
    ) -> Result<BackendConfig, BalanceError> {
        if candidates.is_empty() {
            return Err(BalanceError::NoBackendsAvailable);
        }

        candidates
            .iter()
            .min_by_key(|candidate| candidate.active_connections)
            .map(|candidate| candidate.backend.clone())
            .ok_or(BalanceError::NoBackendsAvailable)
    }
}
