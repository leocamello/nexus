//! Routing configuration

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::agent::PrivacyZone;
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

/// Privacy constraint for traffic policies (FR-013)
///
/// Determines which privacy zones are acceptable for routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PrivacyConstraint {
    /// Any agent privacy zone is acceptable
    #[default]
    Unrestricted,
    /// Only agents with PrivacyZone::Restricted are acceptable
    Restricted,
}

impl PrivacyConstraint {
    /// Check if an agent's privacy zone is allowed under this constraint (FR-013).
    ///
    /// - Unrestricted: allows any privacy zone
    /// - Restricted: only allows PrivacyZone::Restricted agents
    pub fn allows(&self, zone: PrivacyZone) -> bool {
        match self {
            PrivacyConstraint::Unrestricted => true,
            PrivacyConstraint::Restricted => zone == PrivacyZone::Restricted,
        }
    }
}

/// Traffic policy for model-pattern-based routing constraints (FR-035)
///
/// Each policy matches a glob pattern against model names and applies
/// routing constraints (privacy, cost, tier). Policies are evaluated
/// in TOML declaration order (first match wins).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficPolicy {
    /// Glob pattern to match model names (e.g., "gpt-4*", "llama*")
    pub model_pattern: String,

    /// Privacy constraint for matched models
    #[serde(default)]
    pub privacy: PrivacyConstraint,

    /// Maximum cost per request in USD (FR-018, reserved for BudgetReconciler)
    #[serde(default)]
    pub max_cost_per_request: Option<f64>,

    /// Minimum capability tier required (FR-024, reserved for TierReconciler)
    #[serde(default)]
    pub min_tier: Option<u8>,

    /// Whether fallback to other models is allowed
    #[serde(default = "default_fallback_allowed")]
    pub fallback_allowed: bool,
}

fn default_fallback_allowed() -> bool {
    true
}

/// Pre-compiled policy matcher using globset for efficient pattern matching (FR-011)
#[derive(Debug, Clone, Default)]
pub struct PolicyMatcher {
    policies: Vec<TrafficPolicy>,
    matchers: Vec<globset::GlobMatcher>,
}

impl PolicyMatcher {
    /// Compile traffic policies into glob matchers.
    ///
    /// Returns an error if any pattern fails to compile.
    pub fn compile(policies: Vec<TrafficPolicy>) -> Result<Self, ConfigError> {
        let mut matchers = Vec::with_capacity(policies.len());
        for policy in &policies {
            let glob =
                globset::Glob::new(&policy.model_pattern).map_err(|e| ConfigError::Validation {
                    field: format!("routing.policies.model_pattern({})", policy.model_pattern),
                    message: format!("Invalid glob pattern: {}", e),
                })?;
            matchers.push(glob.compile_matcher());
        }
        Ok(Self { policies, matchers })
    }

    /// Find the first matching policy for a model name (TOML order precedence).
    ///
    /// Returns None if no policy matches (zero-config default: unrestricted).
    pub fn find_policy(&self, model: &str) -> Option<&TrafficPolicy> {
        for (i, matcher) in self.matchers.iter().enumerate() {
            if matcher.is_match(model) {
                return Some(&self.policies[i]);
            }
        }
        None
    }

    /// Returns true if no policies are configured
    pub fn is_empty(&self) -> bool {
        self.policies.is_empty()
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
    /// Traffic policies for model-pattern-based routing constraints (FR-011, FR-034)
    /// Optional: zero-config by default (no policies = unrestricted)
    #[serde(default)]
    pub policies: Vec<TrafficPolicy>,
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
            policies: Vec::new(),
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
        assert!(matches!(
            result,
            Err(ConfigError::CircularAlias { ref start, ref cycle }) if start == "a" && cycle == "a"
        ));
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
        assert!(matches!(
            result,
            Err(ConfigError::CircularAlias { ref start, ref cycle })
                if (start == "a" || start == "b") && (cycle == "a" || cycle == "b")
        ));
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
        assert!(matches!(result, Err(ConfigError::CircularAlias { .. })));
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

    #[test]
    fn test_routing_strategy_conversion() {
        let config_smart = RoutingStrategy::Smart;
        let config_rr = RoutingStrategy::RoundRobin;
        let config_prio = RoutingStrategy::PriorityOnly;
        let config_rand = RoutingStrategy::Random;

        let rt_smart: crate::routing::RoutingStrategy = config_smart.into();
        let rt_rr: crate::routing::RoutingStrategy = config_rr.into();
        let rt_prio: crate::routing::RoutingStrategy = config_prio.into();
        let rt_rand: crate::routing::RoutingStrategy = config_rand.into();

        assert!(matches!(rt_smart, crate::routing::RoutingStrategy::Smart));
        assert!(matches!(rt_rr, crate::routing::RoutingStrategy::RoundRobin));
        assert!(matches!(
            rt_prio,
            crate::routing::RoutingStrategy::PriorityOnly
        ));
        assert!(matches!(rt_rand, crate::routing::RoutingStrategy::Random));
    }

    // T033-T035: PrivacyConstraint tests
    #[test]
    fn privacy_constraint_unrestricted_allows_all() {
        assert!(PrivacyConstraint::Unrestricted.allows(PrivacyZone::Restricted));
        assert!(PrivacyConstraint::Unrestricted.allows(PrivacyZone::Open));
    }

    #[test]
    fn privacy_constraint_restricted_blocks_open() {
        assert!(PrivacyConstraint::Restricted.allows(PrivacyZone::Restricted));
        assert!(!PrivacyConstraint::Restricted.allows(PrivacyZone::Open));
    }

    #[test]
    fn privacy_constraint_default_is_unrestricted() {
        assert_eq!(
            PrivacyConstraint::default(),
            PrivacyConstraint::Unrestricted
        );
    }

    // T036-T039: PolicyMatcher tests
    #[test]
    fn policy_matcher_exact_match() {
        let policies = vec![TrafficPolicy {
            model_pattern: "gpt-4".to_string(),
            privacy: PrivacyConstraint::Restricted,
            max_cost_per_request: None,
            min_tier: None,
            fallback_allowed: true,
        }];
        let matcher = PolicyMatcher::compile(policies).unwrap();
        assert!(matcher.find_policy("gpt-4").is_some());
        assert!(matcher.find_policy("gpt-3.5").is_none());
    }

    #[test]
    fn policy_matcher_glob_wildcard() {
        let policies = vec![TrafficPolicy {
            model_pattern: "gpt-4*".to_string(),
            privacy: PrivacyConstraint::Restricted,
            max_cost_per_request: None,
            min_tier: None,
            fallback_allowed: true,
        }];
        let matcher = PolicyMatcher::compile(policies).unwrap();
        assert!(matcher.find_policy("gpt-4").is_some());
        assert!(matcher.find_policy("gpt-4-turbo").is_some());
        assert!(matcher.find_policy("gpt-4o").is_some());
        assert!(matcher.find_policy("gpt-3.5").is_none());
    }

    #[test]
    fn policy_matcher_first_match_wins() {
        let policies = vec![
            TrafficPolicy {
                model_pattern: "gpt-4*".to_string(),
                privacy: PrivacyConstraint::Restricted,
                max_cost_per_request: None,
                min_tier: None,
                fallback_allowed: true,
            },
            TrafficPolicy {
                model_pattern: "gpt-*".to_string(),
                privacy: PrivacyConstraint::Unrestricted,
                max_cost_per_request: None,
                min_tier: None,
                fallback_allowed: true,
            },
        ];
        let matcher = PolicyMatcher::compile(policies).unwrap();
        let policy = matcher.find_policy("gpt-4-turbo").unwrap();
        assert_eq!(policy.privacy, PrivacyConstraint::Restricted);
    }

    #[test]
    fn policy_matcher_no_policies_returns_none() {
        let matcher = PolicyMatcher::compile(vec![]).unwrap();
        assert!(matcher.find_policy("anything").is_none());
        assert!(matcher.is_empty());
    }

    #[test]
    fn policy_matcher_invalid_glob_returns_error() {
        let policies = vec![TrafficPolicy {
            model_pattern: "[invalid".to_string(),
            privacy: PrivacyConstraint::Unrestricted,
            max_cost_per_request: None,
            min_tier: None,
            fallback_allowed: true,
        }];
        assert!(PolicyMatcher::compile(policies).is_err());
    }

    #[test]
    fn traffic_policy_serde_roundtrip() {
        let toml_str = r#"
            model_pattern = "gpt-4*"
            privacy = "restricted"
            max_cost_per_request = 0.05
            min_tier = 2
            fallback_allowed = false
        "#;
        let policy: TrafficPolicy = toml::from_str(toml_str).unwrap();
        assert_eq!(policy.model_pattern, "gpt-4*");
        assert_eq!(policy.privacy, PrivacyConstraint::Restricted);
        assert_eq!(policy.max_cost_per_request, Some(0.05));
        assert_eq!(policy.min_tier, Some(2));
        assert!(!policy.fallback_allowed);
    }

    #[test]
    fn traffic_policy_defaults() {
        let toml_str = r#"
            model_pattern = "llama*"
        "#;
        let policy: TrafficPolicy = toml::from_str(toml_str).unwrap();
        assert_eq!(policy.privacy, PrivacyConstraint::Unrestricted);
        assert!(policy.max_cost_per_request.is_none());
        assert!(policy.min_tier.is_none());
        assert!(policy.fallback_allowed);
    }

    #[test]
    fn routing_config_with_policies_serde() {
        let toml_str = r#"
            strategy = "smart"
            max_retries = 2

            [[policies]]
            model_pattern = "gpt-4*"
            privacy = "restricted"

            [[policies]]
            model_pattern = "*"
            privacy = "unrestricted"
        "#;
        let config: RoutingConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.policies.len(), 2);
        assert_eq!(config.policies[0].privacy, PrivacyConstraint::Restricted);
        assert_eq!(config.policies[1].privacy, PrivacyConstraint::Unrestricted);
    }
}
