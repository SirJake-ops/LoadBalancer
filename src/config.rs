use std::fs;
use std::net::SocketAddr;
use std::path::Path;

#[derive(Debug, serde::Deserialize, Clone)]
pub struct LoadBalancerConfig {
    pub listener_address: SocketAddr,
    pub backend_list: Vec<BackendConfig>,
    pub strategy: StrategyKind,
    pub health_check_interval: HealthCheckConfig,
}

impl LoadBalancerConfig {
    pub fn new() -> Self {
        Self {
            listener_address: "127.0.0.1:8080".parse().unwrap(),
            backend_list: vec![
                BackendConfig::new(
                    "127.0.0.1:8081".to_string(),
                    "backend-id-1".to_string(),
                    Some(1),
                ),
                BackendConfig::new(
                    "127.0.0.1:8082".to_string(),
                    "backend-id-2".to_string(),
                    Some(2),
                ),
            ],
            strategy: StrategyKind::RoundRobin,
            health_check_interval: HealthCheckConfig {
                interval_seconds: 10,
                timeout_seconds: 5,
            },
        }
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let config_contents = fs::read_to_string(path)?;
        let config = toml::from_str(&config_contents)?;
        Ok(config)
    }

    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        Self::from_file("config.toml")
    }
}

#[derive(Debug, serde::Deserialize, Clone)]
pub struct BackendConfig {
    pub backend_address: SocketAddr,
    pub backend_id: String,
    pub weight: Option<u32>,
}

impl BackendConfig {
    pub fn new(backend_address: String, backend_id: String, weight: Option<u32>) -> Self {
        Self {
            backend_address: backend_address.parse().unwrap(),
            backend_id,
            weight,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BackendCandidate {
    pub backend: BackendConfig,
    pub active_connections: usize,
}

#[derive(Debug, serde::Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StrategyKind {
    RoundRobin,
    LeastConnections,
}

#[derive(Debug, serde::Deserialize, Clone)]
pub struct HealthCheckConfig {
    pub interval_seconds: u64,
    pub timeout_seconds: u64,
}

#[test]
fn test_config() {
    let config = LoadBalancerConfig::new();
    assert_eq!(config.listener_address, "127.0.0.1:8080".parse().unwrap());
    assert_eq!(config.backend_list.len(), 2);
    assert_eq!(config.strategy, StrategyKind::RoundRobin);
    assert_eq!(config.health_check_interval.interval_seconds, 10);
    assert_eq!(config.health_check_interval.timeout_seconds, 5);

    let backend_config = BackendConfig::new(
        "127.0.0.1:8081".to_string(),
        "backend-id-1".to_string(),
        None,
    );
    assert_eq!(
        backend_config.backend_address,
        "127.0.0.1:8081".parse().unwrap()
    );
    assert_eq!(backend_config.backend_id, "backend-id-1");
    assert_eq!(backend_config.weight, None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_deserialization() {
        let config_str: LoadBalancerConfig = toml::from_str(
            r#"
        listener_address = "127.0.0.1:8080"
        strategy = "round_robin"

        [[backend_list]]
            backend_address = "127.0.0.1:8081"
            backend_id = "1"
            weight = 1

        [[backend_list]]
            backend_address = "127.0.0.1:8082"
            backend_id = "2"
            weight = 2

        [health_check_interval]
            interval_seconds = 10
            timeout_seconds = 5
            "#,
        )
        .unwrap();
        assert_eq!(
            config_str.listener_address,
            "127.0.0.1:8080".parse().unwrap()
        );
        assert_eq!(config_str.backend_list.len(), 2);
        assert_eq!(config_str.strategy, StrategyKind::RoundRobin);
        assert_eq!(config_str.health_check_interval.interval_seconds, 10);
        assert_eq!(config_str.health_check_interval.timeout_seconds, 5);
        assert_eq!(config_str.backend_list[0].backend_id, "1");
        assert_eq!(config_str.backend_list[1].backend_id, "2");
        assert_eq!(config_str.backend_list[0].weight, Some(1));
        assert_eq!(config_str.backend_list[1].weight, Some(2));
    }

    #[test]
    fn test_config_file_loading() {
        let config = LoadBalancerConfig::load().unwrap();

        assert_eq!(config.listener_address, "127.0.0.1:8080".parse().unwrap());
        assert_eq!(config.backend_list.len(), 2);
        assert_eq!(config.strategy, StrategyKind::RoundRobin);
        assert_eq!(config.health_check_interval.interval_seconds, 10);
        assert_eq!(config.health_check_interval.timeout_seconds, 5);
    }
}
