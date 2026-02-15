//! Configuration module for Nexus
//!
//! Provides layered configuration loading from files, environment variables, and defaults.
//!
//! # Configuration Precedence
//!
//! 1. CLI arguments (highest priority)
//! 2. Environment variables (`NEXUS_*`)
//! 3. Configuration file (TOML)
//! 4. Default values (lowest priority)
//!
//! # Example
//!
//! ```rust
//! use nexus::config::NexusConfig;
//!
//! // Load defaults
//! let config = NexusConfig::default();
//! assert_eq!(config.server.port, 8000);
//!
//! // Parse from TOML
//! let toml = r#"
//! [server]
//! port = 9000
//! "#;
//! let config: NexusConfig = toml::from_str(toml).unwrap();
//! assert_eq!(config.server.port, 9000);
//! ```

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

/// Unified configuration for the Nexus server.
///
/// This struct aggregates all configuration sections including server settings,
/// discovery, health checking, routing, backends, and logging.
///
/// # Example
///
/// ```rust
/// use nexus::config::NexusConfig;
///
/// let config = NexusConfig::default();
/// assert_eq!(config.server.port, 8000);
/// assert_eq!(config.server.host, "0.0.0.0");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NexusConfig {
    /// HTTP server configuration
    pub server: ServerConfig,
    /// mDNS discovery settings
    pub discovery: DiscoveryConfig,
    /// Health check configuration
    pub health_check: HealthCheckConfig,
    /// Request routing configuration
    pub routing: RoutingConfig,
    /// Static backend definitions
    pub backends: Vec<BackendConfig>,
    /// Logging configuration
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

        // Validate routing aliases for circular references
        routing::validate_aliases(&self.routing.aliases)?;

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

    #[test]
    fn test_config_validation_circular_alias() {
        let mut config = NexusConfig::default();

        // Add circular alias: a → b, b → a
        config
            .routing
            .aliases
            .insert("a".to_string(), "b".to_string());
        config
            .routing
            .aliases
            .insert("b".to_string(), "a".to_string());

        // Validation should fail
        let result = config.validate();
        assert!(matches!(result, Err(ConfigError::CircularAlias { .. })));
    }

    #[test]
    fn test_config_validation_valid_aliases() {
        let mut config = NexusConfig::default();

        // Add valid aliases
        config
            .routing
            .aliases
            .insert("gpt-4".to_string(), "llama3:70b".to_string());
        config
            .routing
            .aliases
            .insert("claude".to_string(), "mixtral".to_string());

        // Validation should pass
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_env_override_log_format() {
        // Test valid format
        std::env::set_var("NEXUS_LOG_FORMAT", "json");
        let config = NexusConfig::default().with_env_overrides();
        assert_eq!(
            config.logging.format,
            crate::config::logging::LogFormat::Json
        );

        // Test invalid format keeps default
        std::env::set_var("NEXUS_LOG_FORMAT", "xml");
        let config = NexusConfig::default().with_env_overrides();
        std::env::remove_var("NEXUS_LOG_FORMAT");
        assert_eq!(
            config.logging.format,
            crate::config::logging::LogFormat::Pretty
        );
    }

    #[test]
    fn test_config_env_override_discovery() {
        std::env::set_var("NEXUS_DISCOVERY", "false");
        let config = NexusConfig::default().with_env_overrides();
        std::env::remove_var("NEXUS_DISCOVERY");

        assert!(!config.discovery.enabled);
    }

    #[test]
    fn test_config_env_override_health_check() {
        std::env::set_var("NEXUS_HEALTH_CHECK", "false");
        let config = NexusConfig::default().with_env_overrides();
        std::env::remove_var("NEXUS_HEALTH_CHECK");

        assert!(!config.health_check.enabled);
    }

    #[test]
    fn test_config_validation_zero_port() {
        let mut config = NexusConfig::default();
        config.server.port = 0;

        let result = config.validate();
        assert!(matches!(
            result,
            Err(ConfigError::Validation { ref field, .. }) if field == "server.port"
        ));
    }

    #[test]
    fn test_config_validation_empty_backend_url() {
        let mut config = NexusConfig::default();
        config.backends.push(crate::config::backend::BackendConfig {
            name: "test".to_string(),
            url: "".to_string(),
            backend_type: crate::registry::BackendType::Ollama,
            priority: 1,
            api_key_env: None,
        });

        let result = config.validate();
        assert!(matches!(
            result,
            Err(ConfigError::Validation { ref field, .. }) if field.contains("url")
        ));
    }

    #[test]
    fn test_config_validation_empty_backend_name() {
        let mut config = NexusConfig::default();
        config.backends.push(crate::config::backend::BackendConfig {
            name: "".to_string(),
            url: "http://localhost:11434".to_string(),
            backend_type: crate::registry::BackendType::Ollama,
            priority: 1,
            api_key_env: None,
        });

        let result = config.validate();
        assert!(matches!(
            result,
            Err(ConfigError::Validation { ref field, .. }) if field.contains("name")
        ));
    }

    #[test]
    fn test_config_load_none_returns_defaults() {
        let config = NexusConfig::load(None).unwrap();
        assert_eq!(config.server.port, 8000);
        assert_eq!(config.server.host, "0.0.0.0");
    }
}
