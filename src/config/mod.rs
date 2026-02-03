//! Configuration module for Nexus
//!
//! Provides layered configuration loading from files, environment variables, and defaults.

pub mod backend;
pub mod discovery;
pub mod error;
pub mod logging;
pub mod routing;
pub mod server;

pub use backend::{BackendConfig, BackendType};
pub use discovery::DiscoveryConfig;
pub use error::ConfigError;
pub use logging::{LogFormat, LoggingConfig};
pub use routing::{RoutingConfig, RoutingStrategy, RoutingWeights};
pub use server::ServerConfig;

// Re-export HealthCheckConfig from health module
pub use crate::health::HealthCheckConfig;

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main configuration struct that holds all sub-configurations
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NexusConfig {
    pub server: ServerConfig,
    pub discovery: DiscoveryConfig,
    pub health_check: HealthCheckConfig,
    pub routing: RoutingConfig,
    pub backends: Vec<BackendConfig>,
    pub logging: LoggingConfig,
}

impl NexusConfig {
    /// Load configuration from a TOML file
    ///
    /// If path is None, returns default configuration.
    /// If path doesn't exist, returns NotFound error.
    pub fn load(path: Option<&Path>) -> Result<Self, ConfigError> {
        match path {
            Some(p) => {
                if !p.exists() {
                    return Err(ConfigError::NotFound(p.to_path_buf()));
                }
                let content = std::fs::read_to_string(p)?;
                toml::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))
            }
            None => Ok(Self::default()),
        }
    }

    /// Apply environment variable overrides
    ///
    /// Supports NEXUS_* environment variables for common settings.
    /// Invalid values are silently ignored (defaults are kept).
    pub fn with_env_overrides(mut self) -> Self {
        // Server settings
        if let Ok(port) = std::env::var("NEXUS_PORT") {
            if let Ok(p) = port.parse() {
                self.server.port = p;
            }
        }
        if let Ok(host) = std::env::var("NEXUS_HOST") {
            self.server.host = host;
        }

        // Logging settings
        if let Ok(level) = std::env::var("NEXUS_LOG_LEVEL") {
            self.logging.level = level;
        }
        if let Ok(format) = std::env::var("NEXUS_LOG_FORMAT") {
            if let Ok(f) = format.parse() {
                self.logging.format = f;
            }
        }

        // Discovery and health check
        if let Ok(discovery) = std::env::var("NEXUS_DISCOVERY") {
            self.discovery.enabled = discovery.to_lowercase() == "true";
        }
        if let Ok(health) = std::env::var("NEXUS_HEALTH_CHECK") {
            self.health_check.enabled = health.to_lowercase() == "true";
        }

        self
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate server config
        if self.server.port == 0 {
            return Err(ConfigError::Validation {
                field: "server.port".to_string(),
                message: "port must be non-zero".to_string(),
            });
        }

        // Validate backends
        for (i, backend) in self.backends.iter().enumerate() {
            if backend.url.is_empty() {
                return Err(ConfigError::Validation {
                    field: format!("backends[{}].url", i),
                    message: "URL cannot be empty".to_string(),
                });
            }
            if backend.name.is_empty() {
                return Err(ConfigError::Validation {
                    field: format!("backends[{}].name", i),
                    message: "name cannot be empty".to_string(),
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_nexus_config_defaults() {
        let config = NexusConfig::default();
        assert_eq!(config.server.port, 8000);
        assert!(config.discovery.enabled);
        assert!(config.health_check.enabled);
        assert!(config.backends.is_empty());
    }

    #[test]
    fn test_config_parse_minimal_toml() {
        let toml = r#"
        [server]
        port = 9000
        "#;

        let config: NexusConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.server.host, "0.0.0.0"); // Default
    }

    #[test]
    fn test_config_parse_full_toml() {
        let toml = include_str!("../../nexus.example.toml");
        let config: NexusConfig = toml::from_str(toml).unwrap();
        assert!(config.server.port > 0);
    }

    #[test]
    fn test_config_parse_backends_array() {
        let toml = r#"
        [[backends]]
        name = "local"
        url = "http://localhost:11434"
        type = "ollama"

        [[backends]]
        name = "remote"
        url = "http://192.168.1.100:8000"
        type = "vllm"
        "#;

        let config: NexusConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.backends.len(), 2);
    }

    #[test]
    fn test_config_load_from_file() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "[server]\nport = 8080").unwrap();

        let config = NexusConfig::load(Some(temp.path())).unwrap();
        assert_eq!(config.server.port, 8080);
    }

    #[test]
    fn test_config_missing_file_error() {
        let result = NexusConfig::load(Some(Path::new("/nonexistent/config.toml")));
        assert!(matches!(result, Err(ConfigError::NotFound(_))));
    }

    #[test]
    fn test_config_env_override_port() {
        std::env::set_var("NEXUS_PORT", "9999");
        let config = NexusConfig::default().with_env_overrides();
        std::env::remove_var("NEXUS_PORT");

        assert_eq!(config.server.port, 9999);
    }

    #[test]
    fn test_config_env_override_host() {
        std::env::set_var("NEXUS_HOST", "127.0.0.1");
        let config = NexusConfig::default().with_env_overrides();
        std::env::remove_var("NEXUS_HOST");

        assert_eq!(config.server.host, "127.0.0.1");
    }

    #[test]
    fn test_config_env_override_log_level() {
        std::env::set_var("NEXUS_LOG_LEVEL", "debug");
        let config = NexusConfig::default().with_env_overrides();
        std::env::remove_var("NEXUS_LOG_LEVEL");

        assert_eq!(config.logging.level, "debug");
    }

    #[test]
    fn test_config_env_invalid_value_ignored() {
        std::env::set_var("NEXUS_PORT", "not-a-number");
        let config = NexusConfig::default().with_env_overrides();
        std::env::remove_var("NEXUS_PORT");

        // Should keep default, not crash
        assert_eq!(config.server.port, 8000);
    }
}
