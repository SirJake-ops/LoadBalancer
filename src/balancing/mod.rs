mod least_connections;
pub mod round_robin;

pub use least_connections::LeastConnections;
pub use round_robin::RoundRobin;

use crate::config::{BackendCandidate, BackendConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BalanceError {
    NoBackendsAvailable,
}

pub trait BalancingStrategy: Send {
    fn select_backend(
        &mut self,
        candidates: &[BackendCandidate],
    ) -> Result<BackendConfig, BalanceError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: u64, port: u16) -> BackendCandidate {
        BackendCandidate {
            backend: BackendConfig::new(format!("127.0.0.1:{port}"), id, None),
            active_connections: 0,
        }
    }

    struct FirstBackendStrategy;

    impl BalancingStrategy for FirstBackendStrategy {
        fn select_backend(
            &mut self,
            candidates: &[BackendCandidate],
        ) -> Result<BackendConfig, BalanceError> {
            candidates
                .first()
                .map(|candidate| candidate.backend.clone())
                .ok_or(BalanceError::NoBackendsAvailable)
        }
    }

    #[test]
    fn strategy_selects_backend_from_candidates() {
        let candidates = vec![candidate(1, 8081), candidate(2, 8082)];
        let mut strategy = FirstBackendStrategy;

        let selected = strategy.select_backend(&candidates).unwrap();

        assert_eq!(selected.backend_id, "1");
    }

    #[test]
    fn strategy_returns_error_when_no_candidates_exist() {
        let candidates = Vec::new();
        let mut strategy = FirstBackendStrategy;

        let result = strategy.select_backend(&candidates);

        assert!(matches!(result, Err(BalanceError::NoBackendsAvailable)));
    }

    #[test]
    fn strategy_can_be_used_as_trait_object() {
        let candidates = vec![candidate(1, 8081)];
        let mut strategy: Box<dyn BalancingStrategy> = Box::new(FirstBackendStrategy);

        let selected = strategy.select_backend(&candidates).unwrap();

        assert_eq!(selected.backend_id, "1");
    }
}
