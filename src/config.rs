use std::fs;
use std::net::SocketAddr;
use std::path::Path;

#[derive(Debug, serde::Deserialize, Clone)]
pub struct LoadBalancerConfig {
    pub services: Vec<ServiceConfig>,
}

#[derive(Debug, serde::Deserialize, Clone)]
pub struct ServiceConfig {
    pub name: String,
    pub listener_address: SocketAddr,
    pub backend_list: Vec<BackendConfig>,
    pub strategy: StrategyKind,
    pub health_check_interval: HealthCheckConfig,
}

impl ServiceConfig {
    pub fn new(
        name: String,
        listener_address: String,
        backend_list: Option<Vec<BackendConfig>>,
    ) -> Self {
        Self {
            name,
            listener_address: listener_address.parse().unwrap(),
            backend_list: backend_list.unwrap_or_else(|| vec![]),
            strategy: StrategyKind::RoundRobin,
            health_check_interval: HealthCheckConfig {
                interval_seconds: 10,
                timeout_seconds: 5,
            },
        }
    }
}

impl LoadBalancerConfig {
    pub fn new(service_config: ServiceConfig) -> Self {
        Self {
            services: vec![service_config],
        }
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let config_contents = fs::read_to_string(path)?;
        let config = toml::from_str(&config_contents)?;
        Ok(config)
    }

    pub fn load() -> Result<Self, Box<dyn std::error::Error + Send + Sync + 'static>> {
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
    let service_config = ServiceConfig {
        name: "java-api".to_string(),
        listener_address: "127.0.0.1:9001".parse().unwrap(),
        backend_list: vec![BackendConfig::new(
            "127.0.0.1:8081".to_string(),
            "java-api-1".to_string(),
            Some(1),
        )],
        strategy: StrategyKind::RoundRobin,
        health_check_interval: HealthCheckConfig {
            interval_seconds: 10,
            timeout_seconds: 5,
        },
    };

    let config = LoadBalancerConfig::new(service_config);
    assert_eq!(config.services.len(), 1);

    let service = &config.services[0];
    assert_eq!(service.name, "java-api");
    assert_eq!(service.listener_address, "127.0.0.1:9001".parse().unwrap());
    assert_eq!(service.backend_list.len(), 1);
    assert_eq!(service.backend_list[0].backend_id, "java-api-1");
    assert_eq!(service.strategy, StrategyKind::RoundRobin);
    assert_eq!(service.health_check_interval.interval_seconds, 10);
    assert_eq!(service.health_check_interval.timeout_seconds, 5);

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
    use std::fs;

    #[test]
    fn test_config_deserialization() {
        let config_str: LoadBalancerConfig = toml::from_str(
            r#"
        [[services]]
        name = "java-api"
        listener_address = "127.0.0.1:9001"
        strategy = "round_robin"

        [[services.backend_list]]
            backend_address = "127.0.0.1:8081"
            backend_id = "java-api-1"
            weight = 1

        [services.health_check_interval]
            interval_seconds = 10
            timeout_seconds = 5

        [[services]]
        name = "cpp-engine"
        listener_address = "127.0.0.1:9002"
        strategy = "least_connections"

        [[services.backend_list]]
            backend_address = "127.0.0.1:8080"
            backend_id = "cpp-engine-1"
            weight = 2

        [services.health_check_interval]
            interval_seconds = 10
            timeout_seconds = 5
            "#,
        )
        .unwrap();

        assert_eq!(config_str.services.len(), 2);

        let java_service = &config_str.services[0];
        assert_eq!(
            java_service.listener_address,
            "127.0.0.1:9001".parse().unwrap()
        );
        assert_eq!(java_service.name, "java-api");
        assert_eq!(java_service.backend_list.len(), 1);
        assert_eq!(java_service.strategy, StrategyKind::RoundRobin);
        assert_eq!(java_service.health_check_interval.interval_seconds, 10);
        assert_eq!(java_service.health_check_interval.timeout_seconds, 5);
        assert_eq!(java_service.backend_list[0].backend_id, "java-api-1");
        assert_eq!(java_service.backend_list[0].weight, Some(1));

        let cpp_service = &config_str.services[1];
        assert_eq!(cpp_service.name, "cpp-engine");
        assert_eq!(
            cpp_service.listener_address,
            "127.0.0.1:9002".parse().unwrap()
        );
        assert_eq!(cpp_service.backend_list.len(), 1);
        assert_eq!(cpp_service.strategy, StrategyKind::LeastConnections);
        assert_eq!(cpp_service.backend_list[0].backend_id, "cpp-engine-1");
        assert_eq!(cpp_service.backend_list[0].weight, Some(2));
    }

    #[test]
    fn test_config_file_loading() {
        let config_path = std::env::temp_dir().join(format!(
            "load_balancer_config_test_{}.toml",
            std::process::id()
        ));

        fs::write(
            &config_path,
            r#"
        [[services]]
        name = "java-api"
        listener_address = "127.0.0.1:9001"
        strategy = "round_robin"

        [[services.backend_list]]
            backend_address = "127.0.0.1:8081"
            backend_id = "java-api-1"
            weight = 1

        [services.health_check_interval]
            interval_seconds = 10
            timeout_seconds = 5
            "#,
        )
        .unwrap();

        let config = LoadBalancerConfig::from_file(&config_path).unwrap();
        fs::remove_file(&config_path).unwrap();

        assert_eq!(config.services.len(), 1);

        let service = &config.services[0];
        assert_eq!(service.name, "java-api");
        assert_eq!(service.listener_address, "127.0.0.1:9001".parse().unwrap());
        assert_eq!(service.backend_list.len(), 1);
        assert_eq!(service.strategy, StrategyKind::RoundRobin);
        assert_eq!(service.health_check_interval.interval_seconds, 10);
        assert_eq!(service.health_check_interval.timeout_seconds, 5);
    }
}
