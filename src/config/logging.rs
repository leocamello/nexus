//! Logging configuration

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

/// Log output format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    /// Pretty-printed logs for humans
    #[default]
    Pretty,
    /// JSON logs for machine parsing
    Json,
}

impl FromStr for LogFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pretty" => Ok(LogFormat::Pretty),
            "json" => Ok(LogFormat::Json),
            _ => Err(format!("Invalid log format: {}", s)),
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
    pub format: LogFormat,
    /// Component-specific log levels (e.g., {"routing": "debug", "api": "info"})
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_levels: Option<HashMap<String, String>>,
    /// Enable debug content logging (opt-in, defaults to false)
    /// WARNING: When true, request/response message content will be logged
    /// which may include sensitive data. Use only for debugging.
    #[serde(default)]
    pub enable_content_logging: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: LogFormat::Pretty,
            component_levels: None,
            enable_content_logging: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logging_config_defaults() {
        let config = LoggingConfig::default();
        assert_eq!(config.level, "info");
        assert_eq!(config.format, LogFormat::Pretty);
    }

    #[test]
    fn test_log_format_serde() {
        let format = LogFormat::Json;
        let json = serde_json::to_string(&format).unwrap();
        assert_eq!(json, "\"json\"");
    }

    #[test]
    fn test_log_format_from_str() {
        assert_eq!(LogFormat::from_str("pretty").unwrap(), LogFormat::Pretty);
        assert_eq!(LogFormat::from_str("json").unwrap(), LogFormat::Json);
        assert_eq!(LogFormat::from_str("PRETTY").unwrap(), LogFormat::Pretty);
        assert_eq!(LogFormat::from_str("JSON").unwrap(), LogFormat::Json);
    }

    #[test]
    fn test_log_format_from_str_invalid() {
        assert!(LogFormat::from_str("xml").is_err());
        assert!(LogFormat::from_str("").is_err());
    }
}
