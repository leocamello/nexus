//! Backend configuration

use serde::{Deserialize, Serialize};

// Re-export BackendType from registry
pub use crate::registry::BackendType;
// Re-export PrivacyZone from agent types
pub use crate::agent::types::PrivacyZone;

/// Multi-dimensional capability tier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapabilityTier {
    /// Reasoning score (0-10)
    #[serde(default)]
    pub reasoning: Option<u8>,
    /// Coding score (0-10)
    #[serde(default)]
    pub coding: Option<u8>,
    /// Context window size (tokens)
    #[serde(default)]
    pub context_window: Option<u32>,
    /// Vision capability
    #[serde(default)]
    pub vision: bool,
    /// Tools capability
    #[serde(default)]
    pub tools: bool,
}

impl CapabilityTier {
    /// Validate capability scores are in valid ranges
    pub fn validate(&self) -> Result<(), String> {
        if let Some(reasoning) = self.reasoning {
            if reasoning > 10 {
                return Err(format!("Reasoning score {} exceeds maximum 10", reasoning));
            }
        }
        if let Some(coding) = self.coding {
            if coding > 10 {
                return Err(format!("Coding score {} exceeds maximum 10", coding));
            }
        }
        if let Some(context) = self.context_window {
            if context == 0 {
                return Err("Context window must be > 0".to_string());
            }
        }
        Ok(())
    }
}

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
    /// Capability tier (0-4, None = auto-detect) - DEPRECATED: use capability_tier
    #[serde(default)]
    pub tier: Option<u8>,
    /// Multi-dimensional capability tier
    #[serde(default)]
    pub capability_tier: Option<CapabilityTier>,
}

impl BackendConfig {
    /// Validate configuration at load time
    pub fn validate(&self) -> Result<(), String> {
        if let Some(ref cap_tier) = self.capability_tier {
            cap_tier.validate()?;
        }
        Ok(())
    }
}

fn default_priority() -> i32 {
    50
}

fn default_zone() -> PrivacyZone {
    PrivacyZone::Restricted
}
