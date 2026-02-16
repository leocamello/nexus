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

    /// Get the effective tier, defaulting to 1 if not specified (FR-022)
    pub fn effective_tier(&self) -> u8 {
        self.tier.unwrap_or(1)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cloud_backend(backend_type: BackendType) -> BackendConfig {
        BackendConfig {
            name: "test-cloud".to_string(),
            url: "https://api.example.com".to_string(),
            backend_type,
            priority: 50,
            api_key_env: Some("TEST_API_KEY".to_string()),
            zone: None,
            tier: None,
        }
    }

    fn local_backend() -> BackendConfig {
        BackendConfig {
            name: "test-local".to_string(),
            url: "http://localhost:11434".to_string(),
            backend_type: BackendType::Ollama,
            priority: 50,
            api_key_env: None,
            zone: None,
            tier: None,
        }
    }

    #[test]
    fn test_validate_cloud_requires_api_key_env() {
        for bt in [
            BackendType::OpenAI,
            BackendType::Anthropic,
            BackendType::Google,
        ] {
            let mut cfg = cloud_backend(bt);
            cfg.api_key_env = None;
            let result = cfg.validate();
            assert!(result.is_err(), "{bt:?} should require api_key_env");
            assert!(result.unwrap_err().contains("api_key_env"));
        }
    }

    #[test]
    fn test_validate_cloud_with_api_key_passes() {
        for bt in [
            BackendType::OpenAI,
            BackendType::Anthropic,
            BackendType::Google,
        ] {
            let cfg = cloud_backend(bt);
            assert!(
                cfg.validate().is_ok(),
                "{bt:?} should pass with api_key_env"
            );
        }
    }

    #[test]
    fn test_validate_local_no_api_key_required() {
        let cfg = local_backend();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_validate_tier_range_valid() {
        for tier in 1..=5 {
            let mut cfg = local_backend();
            cfg.tier = Some(tier);
            assert!(cfg.validate().is_ok(), "tier {tier} should be valid");
        }
    }

    #[test]
    fn test_validate_tier_range_invalid() {
        for tier in [0, 6, 10, 255] {
            let mut cfg = local_backend();
            cfg.tier = Some(tier);
            let result = cfg.validate();
            assert!(result.is_err(), "tier {tier} should be invalid");
            assert!(result.unwrap_err().contains("invalid tier"));
        }
    }

    #[test]
    fn test_effective_privacy_zone_override() {
        let mut cfg = local_backend();
        cfg.zone = Some(PrivacyZone::Open);
        assert_eq!(cfg.effective_privacy_zone(), PrivacyZone::Open);

        let mut cfg = cloud_backend(BackendType::OpenAI);
        cfg.zone = Some(PrivacyZone::Restricted);
        assert_eq!(cfg.effective_privacy_zone(), PrivacyZone::Restricted);
    }

    #[test]
    fn test_effective_privacy_zone_default() {
        let cfg = local_backend();
        assert_eq!(cfg.effective_privacy_zone(), PrivacyZone::Restricted);

        let cfg = cloud_backend(BackendType::OpenAI);
        assert_eq!(cfg.effective_privacy_zone(), PrivacyZone::Open);
    }

    #[test]
    fn test_effective_tier_default() {
        let cfg = local_backend();
        assert_eq!(cfg.effective_tier(), 1); // FR-022: default tier is 1
    }

    #[test]
    fn test_effective_tier_override() {
        let mut cfg = local_backend();
        cfg.tier = Some(5);
        assert_eq!(cfg.effective_tier(), 5);
    }
}
