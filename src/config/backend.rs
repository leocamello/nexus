//! Backend configuration

use crate::agent::types::PrivacyZone;
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
    
    /// Privacy zone classification (F12: Cloud Backend Support)
    /// Defaults to backend type's default zone if not specified
    #[serde(default)]
    pub zone: Option<PrivacyZone>,
    
    /// Capability tier for routing (F12: Cloud Backend Support)
    /// Range: 1-5, where 5 is highest capability
    /// Defaults to 3 if not specified
    #[serde(default)]
    pub tier: Option<u8>,
}

fn default_priority() -> i32 {
    50
}

impl BackendConfig {
    /// Get the effective privacy zone, using backend type default if not specified
    pub fn effective_privacy_zone(&self) -> PrivacyZone {
        self.zone
            .unwrap_or_else(|| self.backend_type.default_privacy_zone())
    }

    /// Get the effective tier, defaulting to 3 if not specified
    pub fn effective_tier(&self) -> u8 {
        self.tier.unwrap_or(3)
    }

    /// Validate configuration fields
    pub fn validate(&self) -> Result<(), String> {
        // Cloud backends require api_key_env
        if matches!(
            self.backend_type,
            BackendType::OpenAI | BackendType::Anthropic | BackendType::Google
        ) && self.api_key_env.is_none()
        {
            return Err(format!(
                "Backend '{}' of type {:?} requires 'api_key_env' field",
                self.name, self.backend_type
            ));
        }

        // Validate tier range if specified
        if let Some(tier) = self.tier {
            if !(1..=5).contains(&tier) {
                return Err(format!(
                    "Backend '{}' has invalid tier {}, must be 1-5",
                    self.name, tier
                ));
            }
        }

        Ok(())
    }
}
