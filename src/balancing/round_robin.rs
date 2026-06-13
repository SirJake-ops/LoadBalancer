use crate::balancing::{BalanceError, BalancingStrategy};
use crate::config::{BackendCandidate, BackendConfig};

#[derive(Debug, Default)]
pub struct RoundRobin {
    next_index: usize,
}

impl RoundRobin {
    pub fn new() -> Self {
        Self { next_index: 0 }
    }
}

impl BalancingStrategy for RoundRobin {
    fn select_backend(
        &mut self,
        candidates: &[BackendCandidate],
    ) -> Result<BackendConfig, BalanceError> {
        if candidates.is_empty() {
            return Err(BalanceError::NoBackendsAvailable);
        }

        let index = self.next_index % candidates.len();
        self.next_index += 1;
        Ok(candidates[index].backend.clone())
    }
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

    #[test]
    pub fn selects_backends_in_round_robin_order() {
        let mut round_robin = RoundRobin::new();
        let backends = vec![candidate(1, 8081), candidate(2, 8082)];

        assert_eq!(
            round_robin.select_backend(&backends).unwrap().backend_id,
            "1",
        );

        assert_eq!(
            round_robin.select_backend(&backends).unwrap().backend_id,
            "2",
        );

        assert_eq!(
            round_robin.select_backend(&backends).unwrap().backend_id,
            "1",
        );
    }

    #[test]
    pub fn selects_only_from_provided_healthy_candidates() {
        let mut round_robin = RoundRobin::new();
        let backends = vec![candidate(1, 8081), candidate(3, 8083)];
        assert_eq!(
            round_robin.select_backend(&backends).unwrap().backend_id,
            "1"
        );
        assert_eq!(
            round_robin.select_backend(&backends).unwrap().backend_id,
            "3"
        );
    }

    #[test]
    pub fn returns_error_when_no_candidates_exist() {
        let mut round_robin = RoundRobin::new();
        let backends = Vec::new();

        let result = round_robin.select_backend(&backends);

        assert!(matches!(result, Err(BalanceError::NoBackendsAvailable)));
    }
}
