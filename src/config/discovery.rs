//! Discovery configuration

use serde::{Deserialize, Serialize};

/// mDNS discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscoveryConfig {
    pub enabled: bool,
    pub service_types: Vec<String>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            service_types: vec![
                "_ollama._tcp.local".to_string(),
                "_llm._tcp.local".to_string(),
            ],
        }
    }
}
