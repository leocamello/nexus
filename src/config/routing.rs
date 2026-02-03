//! Routing configuration

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Routing strategy for backend selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RoutingStrategy {
    /// Smart routing based on multiple factors
    #[default]
    Smart,
    /// Round-robin across backends
    RoundRobin,
    /// Priority-only routing
    PriorityOnly,
    /// Random selection
    Random,
}

/// Routing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RoutingConfig {
    pub strategy: RoutingStrategy,
    pub max_retries: u32,
    pub weights: RoutingWeights,
    #[serde(default)]
    pub aliases: HashMap<String, String>,
    #[serde(default)]
    pub fallbacks: HashMap<String, Vec<String>>,
}

/// Routing weights for backend selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingWeights {
    pub priority: f64,
    pub load: f64,
    pub latency: f64,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            strategy: RoutingStrategy::Smart,
            max_retries: 2,
            weights: RoutingWeights::default(),
            aliases: HashMap::new(),
            fallbacks: HashMap::new(),
        }
    }
}

impl Default for RoutingWeights {
    fn default() -> Self {
        Self {
            priority: 50.0,
            load: 30.0,
            latency: 20.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routing_config_defaults() {
        let config = RoutingConfig::default();
        assert_eq!(config.strategy, RoutingStrategy::Smart);
        assert_eq!(config.max_retries, 2);
    }

    #[test]
    fn test_routing_strategy_serde() {
        let strategy = RoutingStrategy::RoundRobin;
        let json = serde_json::to_string(&strategy).unwrap();
        assert_eq!(json, "\"round_robin\"");
    }
}
