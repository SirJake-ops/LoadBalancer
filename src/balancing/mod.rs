mod round_robin;

use crate::config::BackendConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BalanceError {
    NoBackendsAvailable,
}

pub trait BalancingStrategy {
    fn select_backend(
        &mut self,
        candidates: &[BackendConfig],
    ) -> Result<BackendConfig, BalanceError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend(id: u64, port: u16) -> BackendConfig {
        BackendConfig::new(format!("127.0.0.1:{port}"), id, None)
    }

    struct FirstBackendStrategy;

    impl BalancingStrategy for FirstBackendStrategy {
        fn select_backend(
            &mut self,
            candidates: &[BackendConfig],
        ) -> Result<BackendConfig, BalanceError> {
            candidates
                .first()
                .cloned()
                .ok_or(BalanceError::NoBackendsAvailable)
        }
    }

    #[test]
    fn strategy_selects_backend_from_candidates() {
        let candidates = vec![backend(1, 8081), backend(2, 8082)];
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
        let candidates = vec![backend(1, 8081)];
        let mut strategy: Box<dyn BalancingStrategy> = Box::new(FirstBackendStrategy);

        let selected = strategy.select_backend(&candidates).unwrap();

        assert_eq!(selected.backend_id, "1");
    }
}
