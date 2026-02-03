//! Backend configuration

use serde::{Deserialize, Serialize};

// Re-export BackendType from registry
pub use crate::registry::BackendType;

/// Backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    pub name: String,
    pub url: String,
    #[serde(rename = "type")]
    pub backend_type: BackendType,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default)]
    pub api_key_env: Option<String>,
}

fn default_priority() -> i32 {
    50
}
