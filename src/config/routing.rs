//! Routing configuration

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::config::error::ConfigError;

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

impl From<RoutingStrategy> for crate::routing::RoutingStrategy {
    fn from(strategy: RoutingStrategy) -> Self {
        match strategy {
            RoutingStrategy::Smart => crate::routing::RoutingStrategy::Smart,
            RoutingStrategy::RoundRobin => crate::routing::RoutingStrategy::RoundRobin,
            RoutingStrategy::PriorityOnly => crate::routing::RoutingStrategy::PriorityOnly,
            RoutingStrategy::Random => crate::routing::RoutingStrategy::Random,
        }
    }
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
    pub priority: u32,
    pub load: u32,
    pub latency: u32,
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
            priority: 50,
            load: 30,
            latency: 20,
        }
    }
}

impl From<RoutingWeights> for crate::routing::ScoringWeights {
    fn from(weights: RoutingWeights) -> Self {
        crate::routing::ScoringWeights {
            priority: weights.priority,
            load: weights.load,
            latency: weights.latency,
        }
    }
}

/// Validate aliases for circular references
pub fn validate_aliases(aliases: &HashMap<String, String>) -> Result<(), ConfigError> {
    for start in aliases.keys() {
        let mut current = start;
        let mut visited = HashSet::new();
        visited.insert(start);

        while let Some(target) = aliases.get(current) {
            if visited.contains(target) {
                return Err(ConfigError::CircularAlias {
                    start: start.clone(),
                    cycle: target.clone(),
                });
            }
            visited.insert(target);
            current = target;
        }
    }
    Ok(())
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

    // T02: Circular Alias Detection Tests (TDD RED Phase)
    #[test]
    fn validates_circular_alias_direct() {
        // Given aliases: "a" → "a"
        let mut aliases = HashMap::new();
        aliases.insert("a".to_string(), "a".to_string());

        // When validating
        let result = validate_aliases(&aliases);

        // Then returns CircularAlias error
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::CircularAlias { start, cycle } => {
                assert_eq!(start, "a");
                assert_eq!(cycle, "a");
            }
            _ => panic!("Expected CircularAlias error"),
        }
    }

    #[test]
    fn validates_circular_alias_indirect() {
        // Given aliases: "a" → "b", "b" → "a"
        let mut aliases = HashMap::new();
        aliases.insert("a".to_string(), "b".to_string());
        aliases.insert("b".to_string(), "a".to_string());

        // When validating
        let result = validate_aliases(&aliases);

        // Then returns CircularAlias error
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::CircularAlias { start, cycle } => {
                assert!(start == "a" || start == "b");
                assert!(cycle == "a" || cycle == "b");
            }
            _ => panic!("Expected CircularAlias error"),
        }
    }

    #[test]
    fn validates_circular_alias_three_way() {
        // Given aliases: "a" → "b", "b" → "c", "c" → "a"
        let mut aliases = HashMap::new();
        aliases.insert("a".to_string(), "b".to_string());
        aliases.insert("b".to_string(), "c".to_string());
        aliases.insert("c".to_string(), "a".to_string());

        // When validating
        let result = validate_aliases(&aliases);

        // Then returns CircularAlias error
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::CircularAlias { .. } => {
                // Success - circular reference detected
            }
            _ => panic!("Expected CircularAlias error"),
        }
    }

    #[test]
    fn validates_non_circular_aliases() {
        // Given aliases: "a" → "b", "c" → "d"
        let mut aliases = HashMap::new();
        aliases.insert("a".to_string(), "b".to_string());
        aliases.insert("c".to_string(), "d".to_string());

        // When validating
        let result = validate_aliases(&aliases);

        // Then returns Ok
        assert!(result.is_ok());
    }

    #[test]
    fn validates_empty_aliases() {
        // Given empty aliases
        let aliases = HashMap::new();

        // When validating
        let result = validate_aliases(&aliases);

        // Then returns Ok
        assert!(result.is_ok());
    }

    #[test]
    fn validates_chained_aliases_no_cycle() {
        // Given aliases: "a" → "b", "b" → "c", "c" → "d" (no cycle)
        let mut aliases = HashMap::new();
        aliases.insert("a".to_string(), "b".to_string());
        aliases.insert("b".to_string(), "c".to_string());
        aliases.insert("c".to_string(), "d".to_string());

        // When validating
        let result = validate_aliases(&aliases);

        // Then returns Ok
        assert!(result.is_ok());
    }
}
