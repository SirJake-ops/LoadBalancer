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
