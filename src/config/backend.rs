//! Backend configuration

use serde::{Deserialize, Serialize};

// Re-export BackendType from registry
pub use crate::registry::BackendType;
// Re-export PrivacyZone from agent types
pub use crate::agent::types::PrivacyZone;

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
    /// Privacy zone classification (default: Restricted for local, Open for cloud)
    #[serde(default = "default_zone")]
    pub zone: PrivacyZone,
    /// Capability tier (0-4, None = auto-detect)
    #[serde(default)]
    pub tier: Option<u8>,
}

fn default_priority() -> i32 {
    50
}

fn default_zone() -> PrivacyZone {
    PrivacyZone::Restricted
}
