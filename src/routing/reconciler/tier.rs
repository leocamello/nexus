//! TierReconciler - enforces capability tier constraints
//!
//! Matches request models against traffic policies and excludes agents
//! whose capability tier is below the policy's minimum tier. Supports
//! strict (default) and flexible enforcement modes via request headers.

use super::intent::{RoutingIntent, TierEnforcementMode};
use super::Reconciler;
use crate::config::PolicyMatcher;
use crate::registry::Registry;
use crate::routing::error::RoutingError;
use std::sync::Arc;

/// TierReconciler filters candidates by capability tier based on traffic policies.
///
/// # Pipeline Position
/// RequestAnalyzer → PrivacyReconciler → BudgetReconciler → **TierReconciler** → Scheduler
///
/// # Behavior
/// 1. Look up the resolved model in the PolicyMatcher
/// 2. If a policy with `min_tier` matches, set `intent.min_capability_tier`
/// 3. Check enforcement mode (strict by default, or flexible via X-Nexus-Flexible)
/// 4. In strict mode: exclude agents with capability_tier < min_tier
/// 5. In flexible mode: exclude only if higher-tier agents remain after filtering
///
/// # Zero-Config Default (FR-034)
/// If no policies are configured or no policy has `min_tier`, all agents pass through.
pub struct TierReconciler {
    registry: Arc<Registry>,
    policy_matcher: PolicyMatcher,
}

impl TierReconciler {
    /// Create a new TierReconciler with the given registry and compiled policies.
    pub fn new(registry: Arc<Registry>, policy_matcher: PolicyMatcher) -> Self {
        Self {
            registry,
            policy_matcher,
        }
    }

    /// Get the effective capability tier for an agent.
    ///
    /// Checks agent profile first, then defaults to tier 1 (FR-025).
    fn get_agent_capability_tier(&self, agent_id: &str) -> u8 {
        if let Some(agent) = self.registry.get_agent(agent_id) {
            return agent.profile().capability_tier.unwrap_or(1);
        }
        // Unknown agent defaults to tier 1
        1
    }
}

impl Reconciler for TierReconciler {
    fn name(&self) -> &'static str {
        "TierReconciler"
    }

    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        // FR-034: No policies configured → pass through
        if self.policy_matcher.is_empty() {
            return Ok(());
        }

        // Find matching policy for the resolved model
        let policy = match self.policy_matcher.find_policy(&intent.resolved_model) {
            Some(p) => p,
            None => return Ok(()), // No matching policy → unrestricted
        };

        // FR-024: Check if policy specifies a minimum tier
        let min_tier = match policy.min_tier {
            Some(t) => t,
            None => return Ok(()), // No tier requirement in policy
        };

        // Set the tier constraint on the intent for downstream reconcilers
        intent.min_capability_tier = Some(min_tier);

        tracing::debug!(
            model = %intent.resolved_model,
            min_tier = min_tier,
            mode = ?intent.tier_enforcement_mode,
            candidates = intent.candidate_agents.len(),
            "TierReconciler: applying tier constraint"
        );

        let candidate_ids: Vec<String> = intent.candidate_agents.clone();

        match intent.tier_enforcement_mode {
            TierEnforcementMode::Strict => {
                // FR-026/FR-027: Exclude all agents below min_tier
                for agent_id in &candidate_ids {
                    let tier = self.get_agent_capability_tier(agent_id);
                    if tier < min_tier {
                        intent.exclude_agent(
                            agent_id.clone(),
                            "TierReconciler",
                            format!(
                                "Agent capability tier {} is below minimum tier {} \
                                 required by policy for model '{}'",
                                tier, min_tier, intent.resolved_model
                            ),
                            format!(
                                "Use a backend with capability tier >= {} or set \
                                 X-Nexus-Flexible header to allow lower-tier fallback",
                                min_tier
                            ),
                        );
                    }
                }
            }
            TierEnforcementMode::Flexible => {
                // FR-028: Only exclude if higher-tier agents remain
                let has_capable = candidate_ids
                    .iter()
                    .any(|id| self.get_agent_capability_tier(id) >= min_tier);

                if has_capable {
                    // Filter out lower-tier agents since capable ones exist
                    for agent_id in &candidate_ids {
                        let tier = self.get_agent_capability_tier(agent_id);
                        if tier < min_tier {
                            intent.exclude_agent(
                                agent_id.clone(),
                                "TierReconciler",
                                format!(
                                    "Agent capability tier {} is below minimum tier {} \
                                     (higher-tier agents available)",
                                    tier, min_tier
                                ),
                                format!("Use a backend with capability tier >= {}", min_tier),
                            );
                        }
                    }
                } else {
                    // No capable agents — allow all (flexible fallback)
                    tracing::warn!(
                        model = %intent.resolved_model,
                        min_tier = min_tier,
                        "TierReconciler: no agents meet min_tier, allowing flexible fallback"
                    );
                }
            }
        }

        tracing::debug!(
            remaining = intent.candidate_agents.len(),
            excluded = intent.excluded_agents.len(),
            "TierReconciler: filtering complete"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{AgentCapabilities, AgentProfile, ModelCapability};
    use crate::agent::{
        AgentError, HealthStatus, InferenceAgent, PrivacyZone, StreamChunk, TokenCount,
    };
    use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};
    use crate::config::{PrivacyConstraint, TrafficPolicy};
    use crate::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model};
    use crate::routing::reconciler::intent::RoutingIntent;
    use crate::routing::RequestRequirements;
    use async_trait::async_trait;
    use axum::http::HeaderMap;
    use chrono::Utc;
    use futures_util::stream::BoxStream;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, AtomicU64};

    /// Mock agent with configurable capability tier
    struct MockTierAgent {
        id: String,
        profile: AgentProfile,
    }

    impl MockTierAgent {
        fn new(id: &str, tier: Option<u8>) -> Self {
            Self {
                id: id.to_string(),
                profile: AgentProfile {
                    backend_type: "mock".to_string(),
                    version: None,
                    privacy_zone: PrivacyZone::Restricted,
                    capabilities: AgentCapabilities::default(),
                    capability_tier: tier,
                },
            }
        }
    }

    #[async_trait]
    impl InferenceAgent for MockTierAgent {
        fn id(&self) -> &str {
            &self.id
        }

        fn name(&self) -> &str {
            &self.id
        }

        fn profile(&self) -> AgentProfile {
            self.profile.clone()
        }

        async fn health_check(&self) -> Result<HealthStatus, AgentError> {
            Ok(HealthStatus::Healthy { model_count: 0 })
        }

        async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
            Ok(vec![])
        }

        async fn chat_completion(
            &self,
            _request: ChatCompletionRequest,
            _headers: Option<&HeaderMap>,
        ) -> Result<ChatCompletionResponse, AgentError> {
            Err(AgentError::Unsupported("mock"))
        }

        async fn chat_completion_stream(
            &self,
            _request: ChatCompletionRequest,
            _headers: Option<&HeaderMap>,
        ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
            Err(AgentError::Unsupported("mock"))
        }

        async fn count_tokens(&self, _model: &str, _text: &str) -> TokenCount {
            TokenCount::Heuristic(0)
        }
    }

    fn create_backend(id: &str, model_id: &str) -> Backend {
        Backend {
            id: id.to_string(),
            name: id.to_string(),
            url: format!("http://{}", id),
            backend_type: BackendType::Ollama,
            status: BackendStatus::Healthy,
            last_health_check: Utc::now(),
            last_error: None,
            models: vec![Model {
                id: model_id.to_string(),
                name: model_id.to_string(),
                context_length: 4096,
                supports_vision: false,
                supports_tools: false,
                supports_json_mode: false,
                max_output_tokens: None,
            }],
            priority: 1,
            pending_requests: AtomicU32::new(0),
            total_requests: AtomicU64::new(0),
            avg_latency_ms: AtomicU32::new(50),
            discovery_source: DiscoverySource::Static,
            metadata: HashMap::new(),
        }
    }

    fn create_intent(model: &str, candidates: Vec<String>) -> RoutingIntent {
        RoutingIntent::new(
            "req-1".to_string(),
            model.to_string(),
            model.to_string(),
            RequestRequirements {
                model: model.to_string(),
                estimated_tokens: 100,
                needs_vision: false,
                needs_tools: false,
                needs_json_mode: false,
                prefers_streaming: false,
            },
            candidates,
        )
    }

    fn tier_policy(pattern: &str, min_tier: u8) -> TrafficPolicy {
        TrafficPolicy {
            model_pattern: pattern.to_string(),
            privacy: PrivacyConstraint::Unrestricted,
            max_cost_per_request: None,
            min_tier: Some(min_tier),
            fallback_allowed: true,
        }
    }

    // === T069: TierReconciler struct creation ===

    #[test]
    fn no_policies_passes_all_through() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("b1", "llama3:8b"))
            .unwrap();
        registry
            .add_backend(create_backend("b2", "llama3:8b"))
            .unwrap();

        let matcher = PolicyMatcher::default();
        let reconciler = TierReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("llama3:8b", vec!["b1".into(), "b2".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents.len(), 2);
        assert!(intent.excluded_agents.is_empty());
        assert!(intent.min_capability_tier.is_none());
    }

    #[test]
    fn no_min_tier_in_policy_passes_through() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("b1", "llama3:8b"))
            .unwrap();

        let policy = TrafficPolicy {
            model_pattern: "llama*".to_string(),
            privacy: PrivacyConstraint::Unrestricted,
            max_cost_per_request: None,
            min_tier: None, // No tier requirement
            fallback_allowed: true,
        };
        let matcher = PolicyMatcher::compile(vec![policy]).unwrap();
        let reconciler = TierReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("llama3:8b", vec!["b1".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents, vec!["b1"]);
        assert!(intent.min_capability_tier.is_none());
    }

    #[test]
    fn non_matching_model_passes_through() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("b1", "llama3:8b"))
            .unwrap();

        let matcher = PolicyMatcher::compile(vec![tier_policy("gpt-4*", 3)]).unwrap();
        let reconciler = TierReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("llama3:8b", vec!["b1".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents, vec!["b1"]);
    }

    // === T071-T073: Strict tier enforcement (FR-026, FR-027) ===

    #[test]
    fn strict_mode_excludes_low_tier_agents() {
        let registry = Arc::new(Registry::new());

        // Tier 3 agent
        let b1 = create_backend("high-tier", "gpt-4");
        let agent1 = Arc::new(MockTierAgent::new("high-tier", Some(3)));
        registry.add_backend_with_agent(b1, agent1).unwrap();

        // Tier 1 agent (default)
        let b2 = create_backend("low-tier", "gpt-4");
        let agent2 = Arc::new(MockTierAgent::new("low-tier", Some(1)));
        registry.add_backend_with_agent(b2, agent2).unwrap();

        let matcher = PolicyMatcher::compile(vec![tier_policy("gpt-4*", 3)]).unwrap();
        let reconciler = TierReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("gpt-4", vec!["high-tier".into(), "low-tier".into()]);
        // Default is strict
        assert_eq!(intent.tier_enforcement_mode, TierEnforcementMode::Strict);

        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents, vec!["high-tier"]);
        assert_eq!(intent.excluded_agents, vec!["low-tier"]);
        assert_eq!(intent.min_capability_tier, Some(3));
    }

    #[test]
    fn strict_mode_rejects_all_when_none_meet_tier() {
        let registry = Arc::new(Registry::new());

        let b1 = create_backend("t1", "gpt-4");
        let agent1 = Arc::new(MockTierAgent::new("t1", Some(1)));
        registry.add_backend_with_agent(b1, agent1).unwrap();

        let b2 = create_backend("t2", "gpt-4");
        let agent2 = Arc::new(MockTierAgent::new("t2", Some(2)));
        registry.add_backend_with_agent(b2, agent2).unwrap();

        let matcher = PolicyMatcher::compile(vec![tier_policy("gpt-4*", 3)]).unwrap();
        let reconciler = TierReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("gpt-4", vec!["t1".into(), "t2".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert!(intent.candidate_agents.is_empty());
        assert_eq!(intent.rejection_reasons.len(), 2);
    }

    #[test]
    fn strict_mode_keeps_agents_at_exact_min_tier() {
        let registry = Arc::new(Registry::new());

        let b1 = create_backend("exact-tier", "gpt-4");
        let agent1 = Arc::new(MockTierAgent::new("exact-tier", Some(2)));
        registry.add_backend_with_agent(b1, agent1).unwrap();

        let matcher = PolicyMatcher::compile(vec![tier_policy("gpt-4*", 2)]).unwrap();
        let reconciler = TierReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("gpt-4", vec!["exact-tier".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents, vec!["exact-tier"]);
        assert!(intent.excluded_agents.is_empty());
    }

    // === T074: Flexible tier enforcement (FR-028) ===

    #[test]
    fn flexible_mode_allows_fallback_when_no_capable_agents() {
        let registry = Arc::new(Registry::new());

        let b1 = create_backend("t1", "gpt-4");
        let agent1 = Arc::new(MockTierAgent::new("t1", Some(1)));
        registry.add_backend_with_agent(b1, agent1).unwrap();

        let b2 = create_backend("t2", "gpt-4");
        let agent2 = Arc::new(MockTierAgent::new("t2", Some(2)));
        registry.add_backend_with_agent(b2, agent2).unwrap();

        let matcher = PolicyMatcher::compile(vec![tier_policy("gpt-4*", 3)]).unwrap();
        let reconciler = TierReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("gpt-4", vec!["t1".into(), "t2".into()]);
        intent.tier_enforcement_mode = TierEnforcementMode::Flexible;

        reconciler.reconcile(&mut intent).unwrap();

        // All agents kept since none meet min_tier (flexible fallback)
        assert_eq!(intent.candidate_agents.len(), 2);
        assert!(intent.excluded_agents.is_empty());
    }

    #[test]
    fn flexible_mode_still_filters_when_capable_agents_exist() {
        let registry = Arc::new(Registry::new());

        let b1 = create_backend("high-tier", "gpt-4");
        let agent1 = Arc::new(MockTierAgent::new("high-tier", Some(3)));
        registry.add_backend_with_agent(b1, agent1).unwrap();

        let b2 = create_backend("low-tier", "gpt-4");
        let agent2 = Arc::new(MockTierAgent::new("low-tier", Some(1)));
        registry.add_backend_with_agent(b2, agent2).unwrap();

        let matcher = PolicyMatcher::compile(vec![tier_policy("gpt-4*", 3)]).unwrap();
        let reconciler = TierReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("gpt-4", vec!["high-tier".into(), "low-tier".into()]);
        intent.tier_enforcement_mode = TierEnforcementMode::Flexible;

        reconciler.reconcile(&mut intent).unwrap();

        // Low-tier excluded since high-tier exists
        assert_eq!(intent.candidate_agents, vec!["high-tier"]);
        assert_eq!(intent.excluded_agents, vec!["low-tier"]);
    }

    // === T075: Rejection reasons ===

    #[test]
    fn rejection_reason_includes_tier_details() {
        let registry = Arc::new(Registry::new());

        let b1 = create_backend("low", "gpt-4");
        let agent1 = Arc::new(MockTierAgent::new("low", Some(1)));
        registry.add_backend_with_agent(b1, agent1).unwrap();

        let matcher = PolicyMatcher::compile(vec![tier_policy("gpt-4*", 3)]).unwrap();
        let reconciler = TierReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("gpt-4", vec!["low".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.rejection_reasons.len(), 1);
        let reason = &intent.rejection_reasons[0];
        assert_eq!(reason.agent_id, "low");
        assert_eq!(reason.reconciler, "TierReconciler");
        assert!(reason.reason.contains("tier 1"));
        assert!(reason.reason.contains("minimum tier 3"));
        assert!(reason.suggested_action.contains("tier >= 3"));
    }

    #[test]
    fn agents_without_tier_default_to_one() {
        let registry = Arc::new(Registry::new());

        // Agent with no explicit tier (defaults to 1)
        let b1 = create_backend("no-tier", "gpt-4");
        let agent1 = Arc::new(MockTierAgent::new("no-tier", None));
        registry.add_backend_with_agent(b1, agent1).unwrap();

        // Agent with tier 3
        let b2 = create_backend("has-tier", "gpt-4");
        let agent2 = Arc::new(MockTierAgent::new("has-tier", Some(3)));
        registry.add_backend_with_agent(b2, agent2).unwrap();

        let matcher = PolicyMatcher::compile(vec![tier_policy("gpt-4*", 2)]).unwrap();
        let reconciler = TierReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("gpt-4", vec!["no-tier".into(), "has-tier".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents, vec!["has-tier"]);
        assert_eq!(intent.excluded_agents, vec!["no-tier"]);
    }

    #[test]
    fn unknown_agent_defaults_to_tier_one() {
        let registry = Arc::new(Registry::new());
        // Don't add "ghost" to registry

        let matcher = PolicyMatcher::compile(vec![tier_policy("gpt-4*", 2)]).unwrap();
        let reconciler = TierReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("gpt-4", vec!["ghost".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert!(intent.candidate_agents.is_empty());
        assert_eq!(intent.excluded_agents, vec!["ghost"]);
    }

    #[test]
    fn glob_pattern_matching_works() {
        let registry = Arc::new(Registry::new());

        let b1 = create_backend("b1", "gpt-4-turbo");
        let agent1 = Arc::new(MockTierAgent::new("b1", Some(1)));
        registry.add_backend_with_agent(b1, agent1).unwrap();

        let matcher = PolicyMatcher::compile(vec![tier_policy("gpt-4*", 2)]).unwrap();
        let reconciler = TierReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("gpt-4-turbo", vec!["b1".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert!(intent.candidate_agents.is_empty());
        assert_eq!(intent.excluded_agents, vec!["b1"]);
    }

    #[test]
    fn sets_min_capability_tier_on_intent() {
        let registry = Arc::new(Registry::new());

        let b1 = create_backend("b1", "gpt-4");
        let agent1 = Arc::new(MockTierAgent::new("b1", Some(3)));
        registry.add_backend_with_agent(b1, agent1).unwrap();

        let matcher = PolicyMatcher::compile(vec![tier_policy("gpt-4*", 2)]).unwrap();
        let reconciler = TierReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("gpt-4", vec!["b1".into()]);
        assert!(intent.min_capability_tier.is_none());

        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.min_capability_tier, Some(2));
    }
}
