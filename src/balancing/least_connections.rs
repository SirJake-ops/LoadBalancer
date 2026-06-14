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

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: u64, port: u16, active_connections: usize) -> BackendCandidate {
        BackendCandidate {
            backend: BackendConfig::new(format!("127.0.0.1:{port}"), id, None),
            active_connections,
        }
    }

    #[test]
    fn selects_backend_with_fewest_active_connections() {
        let mut strategy = LeastConnections::new();
        let candidates = vec![
            candidate(1, 8081, 4),
            candidate(2, 8082, 1),
            candidate(3, 8083, 7),
        ];

        let selected = strategy.select_backend(&candidates).unwrap();

        assert_eq!(selected.backend_id, "2");
    }

    #[test]
    fn tie_selects_first_candidate_with_lowest_connection_count() {
        let mut strategy = LeastConnections::new();
        let candidates = vec![
            candidate(1, 8081, 2),
            candidate(2, 8082, 2),
            candidate(3, 8083, 5),
        ];

        let selected = strategy.select_backend(&candidates).unwrap();

        assert_eq!(selected.backend_id, "1");
    }

    #[test]
    fn returns_error_when_no_candidates_exist() {
        let mut strategy = LeastConnections::new();
        let candidates = Vec::new();

        let result = strategy.select_backend(&candidates);

        assert!(matches!(result, Err(BalanceError::NoBackendsAvailable)));
    }
}
