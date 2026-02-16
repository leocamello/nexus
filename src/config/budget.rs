//! Budget management configuration

use serde::{Deserialize, Serialize};

/// Budget enforcement configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BudgetConfig {
    /// Whether budget management is enabled
    pub enabled: bool,

    /// Monthly spending limit in USD
    pub monthly_limit: f64,

    /// Soft limit percentage (0-100, triggers local-preferred routing)
    pub soft_limit_percent: u8,

    /// Action at hard limit
    pub hard_limit_action: HardLimitAction,

    /// Day of month when billing cycle resets (1-31)
    pub billing_cycle_start_day: u8,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            monthly_limit: 100.0,
            soft_limit_percent: 80,
            hard_limit_action: HardLimitAction::LocalOnly,
            billing_cycle_start_day: 1,
        }
    }
}

impl BudgetConfig {
    /// Validate configuration at startup
    pub fn validate(&self) -> Result<(), String> {
        // Monthly limit must be non-negative
        if self.monthly_limit < 0.0 {
            return Err("monthly_limit must be >= 0.0".to_string());
        }

        // Soft limit percent must be 0-100
        if self.soft_limit_percent > 100 {
            return Err("soft_limit_percent must be 0-100".to_string());
        }

        // Billing cycle day must be 1-31
        if !(1..=31).contains(&self.billing_cycle_start_day) {
            return Err("billing_cycle_start_day must be 1-31".to_string());
        }

        Ok(())
    }
}

/// Action to take when hard budget limit is reached
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HardLimitAction {
    /// Route only to local agents (block cloud)
    LocalOnly,

    /// Queue requests requiring cloud agents (future: task queue)
    Queue,

    /// Return 429 error for requests requiring cloud agents
    Reject,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_config_defaults() {
        let config = BudgetConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.monthly_limit, 100.0);
        assert_eq!(config.soft_limit_percent, 80);
        assert_eq!(config.hard_limit_action, HardLimitAction::LocalOnly);
        assert_eq!(config.billing_cycle_start_day, 1);
    }

    #[test]
    fn test_budget_config_validation_valid() {
        let config = BudgetConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_budget_config_validation_negative_limit() {
        let config = BudgetConfig {
            monthly_limit: -10.0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_budget_config_validation_soft_limit_percent() {
        let config = BudgetConfig {
            soft_limit_percent: 101,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_budget_config_validation_billing_cycle_day() {
        let config = BudgetConfig {
            billing_cycle_start_day: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = BudgetConfig {
            billing_cycle_start_day: 32,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = BudgetConfig {
            billing_cycle_start_day: 15,
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_budget_config_serialization() {
        let config = BudgetConfig::default();
        let toml = toml::to_string(&config).unwrap();
        let deserialized: BudgetConfig = toml::from_str(&toml).unwrap();
        assert_eq!(config.enabled, deserialized.enabled);
        assert_eq!(config.monthly_limit, deserialized.monthly_limit);
    }

    #[test]
    fn test_hard_limit_action_serialization() {
        #[derive(serde::Serialize)]
        struct TestConfig {
            action: HardLimitAction,
        }

        let config = TestConfig {
            action: HardLimitAction::LocalOnly,
        };
        let toml = toml::to_string(&config).unwrap();
        assert!(toml.contains("local-only"));

        let config = TestConfig {
            action: HardLimitAction::Queue,
        };
        let toml = toml::to_string(&config).unwrap();
        assert!(toml.contains("queue"));

        let config = TestConfig {
            action: HardLimitAction::Reject,
        };
        let toml = toml::to_string(&config).unwrap();
        assert!(toml.contains("reject"));
    }

    #[test]
    fn test_hard_limit_action_deserialization() {
        #[derive(serde::Deserialize)]
        struct TestConfig {
            action: HardLimitAction,
        }

        let toml = "action = \"local-only\"";
        let config: TestConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.action, HardLimitAction::LocalOnly);

        let toml = "action = \"queue\"";
        let config: TestConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.action, HardLimitAction::Queue);

        let toml = "action = \"reject\"";
        let config: TestConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.action, HardLimitAction::Reject);
    }
}
