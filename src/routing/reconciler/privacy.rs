//! PrivacyReconciler - enforces privacy zone constraints
//!
//! Matches request models against traffic policies and excludes agents
//! whose privacy zone violates the policy constraint. Runs BEFORE
//! SchedulerReconciler in the pipeline.

use super::{intent::RoutingIntent, Reconciler};
use crate::config::PolicyMatcher;
use crate::registry::Registry;
use crate::routing::error::RoutingError;
use std::sync::Arc;

/// PrivacyReconciler filters candidates by privacy zone based on traffic policies.
///
/// # Pipeline Position
/// RequestAnalyzer → **PrivacyReconciler** → SchedulerReconciler
///
/// # Behavior
/// 1. Look up the resolved model in the PolicyMatcher
/// 2. If a policy matches, set `intent.privacy_constraint`
/// 3. For each candidate, check its backend's privacy zone against the constraint
/// 4. Exclude agents that violate the constraint with actionable rejection reasons
///
/// # Zero-Config Default (FR-034)
/// If no policies are configured, all agents pass through unchanged.
pub struct PrivacyReconciler {
    registry: Arc<Registry>,
    policy_matcher: PolicyMatcher,
}

impl PrivacyReconciler {
    /// Create a new PrivacyReconciler with the given registry and compiled policies.
    pub fn new(registry: Arc<Registry>, policy_matcher: PolicyMatcher) -> Self {
        Self {
            registry,
            policy_matcher,
        }
    }

    /// Determine the effective privacy zone for a backend.
    ///
    /// Checks for an agent profile first (has explicit privacy_zone),
    /// then falls back to the backend type's default zone.
    /// Unknown backends are treated as Open (cloud) per FR-015.
    fn get_backend_privacy_zone(&self, agent_id: &str) -> crate::agent::PrivacyZone {
        // Try agent profile first (most authoritative)
        if let Some(agent) = self.registry.get_agent(agent_id) {
            return agent.profile().privacy_zone;
        }

        // Fall back to backend type default
        if let Some(backend) = self.registry.get_backend(agent_id) {
            return backend.backend_type.default_privacy_zone();
        }

        // Unknown backend → treat as Open (cloud) per FR-015
        tracing::warn!(
            agent_id = %agent_id,
            "Unknown agent privacy zone, treating as Open (cloud)"
        );
        crate::agent::PrivacyZone::Open
    }
}

impl Reconciler for PrivacyReconciler {
    fn name(&self) -> &'static str {
        "PrivacyReconciler"
    }

    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        // FR-034: No policies configured → pass through (zero-config default)
        if self.policy_matcher.is_empty() {
            return Ok(());
        }

        // Find matching policy for the resolved model
        let policy = match self.policy_matcher.find_policy(&intent.resolved_model) {
            Some(p) => p,
            None => return Ok(()), // No matching policy → unrestricted
        };

        // Set the privacy constraint on the intent for downstream reconcilers
        let constraint = policy.privacy;
        intent.privacy_constraint = Some(match constraint {
            crate::config::PrivacyConstraint::Restricted => crate::agent::PrivacyZone::Restricted,
            crate::config::PrivacyConstraint::Unrestricted => {
                return Ok(()); // Unrestricted → no filtering needed
            }
        });

        tracing::debug!(
            model = %intent.resolved_model,
            privacy = ?constraint,
            candidates = intent.candidate_agents.len(),
            "PrivacyReconciler: applying privacy constraint"
        );

        // Filter candidates by privacy zone
        let candidate_ids: Vec<String> = intent.candidate_agents.clone();
        for agent_id in &candidate_ids {
            let zone = self.get_backend_privacy_zone(agent_id);

            if !constraint.allows(zone) {
                intent.exclude_agent(
                    agent_id.clone(),
                    "PrivacyReconciler",
                    format!(
                        "Agent privacy zone {:?} violates {:?} policy for model '{}'",
                        zone, constraint, intent.resolved_model
                    ),
                    "Use a local backend or change the traffic policy to unrestricted".to_string(),
                );
            }
        }

        tracing::debug!(
            remaining = intent.candidate_agents.len(),
            excluded = intent.excluded_agents.len(),
            "PrivacyReconciler: filtering complete"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::PrivacyZone;
    use crate::config::{PrivacyConstraint, TrafficPolicy};
    use crate::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model};
    use crate::routing::reconciler::intent::RoutingIntent;
    use crate::routing::RequestRequirements;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, AtomicU64};

    fn create_backend(id: &str, model_id: &str, backend_type: BackendType) -> Backend {
        Backend {
            id: id.to_string(),
            name: id.to_string(),
            url: format!("http://{}", id),
            backend_type,
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
            },
            candidates,
        )
    }

    fn restricted_policy(pattern: &str) -> TrafficPolicy {
        TrafficPolicy {
            model_pattern: pattern.to_string(),
            privacy: PrivacyConstraint::Restricted,
            max_cost_per_request: None,
            min_tier: None,
            fallback_allowed: true,
        }
    }

    #[test]
    fn no_policies_passes_all_through() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("local", "llama3:8b", BackendType::Ollama))
            .unwrap();
        registry
            .add_backend(create_backend("cloud", "llama3:8b", BackendType::OpenAI))
            .unwrap();

        let matcher = PolicyMatcher::default();
        let reconciler = PrivacyReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("llama3:8b", vec!["local".into(), "cloud".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents.len(), 2);
        assert!(intent.excluded_agents.is_empty());
    }

    #[test]
    fn restricted_policy_excludes_cloud_backends() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("local", "gpt-4", BackendType::Ollama))
            .unwrap();
        registry
            .add_backend(create_backend("cloud", "gpt-4", BackendType::OpenAI))
            .unwrap();

        let matcher = PolicyMatcher::compile(vec![restricted_policy("gpt-4*")]).unwrap();
        let reconciler = PrivacyReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("gpt-4", vec!["local".into(), "cloud".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents, vec!["local"]);
        assert_eq!(intent.excluded_agents, vec!["cloud"]);
    }

    #[test]
    fn restricted_policy_keeps_local_backends() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("local1", "gpt-4", BackendType::Ollama))
            .unwrap();
        registry
            .add_backend(create_backend("local2", "gpt-4", BackendType::LlamaCpp))
            .unwrap();

        let matcher = PolicyMatcher::compile(vec![restricted_policy("gpt-4*")]).unwrap();
        let reconciler = PrivacyReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("gpt-4", vec!["local1".into(), "local2".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents.len(), 2);
        assert!(intent.excluded_agents.is_empty());
    }

    #[test]
    fn non_matching_model_passes_through() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("cloud", "llama3:8b", BackendType::OpenAI))
            .unwrap();

        let matcher = PolicyMatcher::compile(vec![restricted_policy("gpt-4*")]).unwrap();
        let reconciler = PrivacyReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("llama3:8b", vec!["cloud".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents, vec!["cloud"]);
    }

    #[test]
    fn rejection_reason_includes_required_fields() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("cloud", "gpt-4", BackendType::OpenAI))
            .unwrap();

        let matcher = PolicyMatcher::compile(vec![restricted_policy("gpt-4*")]).unwrap();
        let reconciler = PrivacyReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("gpt-4", vec!["cloud".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.rejection_reasons.len(), 1);
        let reason = &intent.rejection_reasons[0];
        assert_eq!(reason.agent_id, "cloud");
        assert_eq!(reason.reconciler, "PrivacyReconciler");
        assert!(reason.reason.contains("Open"));
        assert!(reason.reason.contains("Restricted"));
        assert!(!reason.suggested_action.is_empty());
    }

    #[test]
    fn sets_privacy_constraint_on_intent() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("local", "gpt-4", BackendType::Ollama))
            .unwrap();

        let matcher = PolicyMatcher::compile(vec![restricted_policy("gpt-4*")]).unwrap();
        let reconciler = PrivacyReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("gpt-4", vec!["local".into()]);
        assert!(intent.privacy_constraint.is_none());

        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.privacy_constraint, Some(PrivacyZone::Restricted));
    }

    #[test]
    fn unknown_backend_treated_as_cloud() {
        let registry = Arc::new(Registry::new());
        // Don't add "ghost" to registry — simulates unknown agent

        let matcher = PolicyMatcher::compile(vec![restricted_policy("gpt-4*")]).unwrap();
        let reconciler = PrivacyReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("gpt-4", vec!["ghost".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        // Unknown agent should be excluded (treated as Open/cloud)
        assert!(intent.candidate_agents.is_empty());
        assert_eq!(intent.excluded_agents, vec!["ghost"]);
    }

    #[test]
    fn glob_pattern_matching_works() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("cloud", "gpt-4-turbo", BackendType::OpenAI))
            .unwrap();
        registry
            .add_backend(create_backend("local", "gpt-4-turbo", BackendType::Ollama))
            .unwrap();

        let matcher = PolicyMatcher::compile(vec![restricted_policy("gpt-4*")]).unwrap();
        let reconciler = PrivacyReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("gpt-4-turbo", vec!["cloud".into(), "local".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents, vec!["local"]);
        assert_eq!(intent.excluded_agents, vec!["cloud"]);
    }

    #[test]
    fn all_candidates_excluded_produces_rejection() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("cloud1", "gpt-4", BackendType::OpenAI))
            .unwrap();
        registry
            .add_backend(create_backend("cloud2", "gpt-4", BackendType::Anthropic))
            .unwrap();

        let matcher = PolicyMatcher::compile(vec![restricted_policy("gpt-4*")]).unwrap();
        let reconciler = PrivacyReconciler::new(Arc::clone(&registry), matcher);

        let mut intent = create_intent("gpt-4", vec!["cloud1".into(), "cloud2".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert!(intent.candidate_agents.is_empty());
        assert_eq!(intent.rejection_reasons.len(), 2);
    }
}
