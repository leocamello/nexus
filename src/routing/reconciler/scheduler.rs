//! SchedulerReconciler - final routing decision maker
//!
//! Filters candidates by health and capabilities, scores remaining candidates,
//! and annotates the intent for the pipeline to produce a Route/Queue/Reject decision.

use super::intent::{BudgetStatus, RoutingIntent};
use super::Reconciler;
use crate::agent::PrivacyZone;
use crate::registry::{Backend, BackendStatus, Registry};
use crate::routing::error::RoutingError;
use crate::routing::scoring::{score_backend, ScoringWeights};
use crate::routing::strategies::RoutingStrategy;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// SchedulerReconciler filters, scores, and selects the best candidate agent.
/// It stores the selected agent_id and score in the intent for the pipeline
/// to convert into a RoutingDecision.
pub struct SchedulerReconciler {
    /// Registry for looking up backend state
    registry: Arc<Registry>,

    /// Routing strategy
    strategy: RoutingStrategy,

    /// Scoring weights for smart strategy
    weights: ScoringWeights,

    /// Round-robin counter (shared with Router)
    round_robin_counter: Arc<AtomicU64>,
}

impl SchedulerReconciler {
    /// Create a new SchedulerReconciler
    pub fn new(
        registry: Arc<Registry>,
        strategy: RoutingStrategy,
        weights: ScoringWeights,
        round_robin_counter: Arc<AtomicU64>,
    ) -> Self {
        Self {
            registry,
            strategy,
            weights,
            round_robin_counter,
        }
    }

    /// Check if a backend meets the request's capability requirements
    fn meets_requirements(backend: &Backend, model: &str, intent: &RoutingIntent) -> bool {
        if let Some(model_info) = backend.models.iter().find(|m| m.id == model) {
            if intent.requirements.needs_vision && !model_info.supports_vision {
                return false;
            }
            if intent.requirements.needs_tools && !model_info.supports_tools {
                return false;
            }
            if intent.requirements.needs_json_mode && !model_info.supports_json_mode {
                return false;
            }
            if intent.requirements.estimated_tokens > model_info.context_length {
                return false;
            }
            true
        } else {
            false
        }
    }

    /// Determine the effective privacy zone for a backend (FR-020).
    fn get_backend_privacy_zone(&self, backend: &Backend) -> PrivacyZone {
        if let Some(agent) = self.registry.get_agent(&backend.id) {
            return agent.profile().privacy_zone;
        }
        backend.backend_type.default_privacy_zone()
    }

    /// Apply budget-aware score adjustment (FR-020).
    /// At SoftLimit, cloud agent scores are reduced by 50% to prefer local agents.
    fn apply_budget_adjustment(
        &self,
        score: u32,
        backend: &Backend,
        intent: &RoutingIntent,
    ) -> u32 {
        if intent.budget_status == BudgetStatus::SoftLimit {
            let zone = self.get_backend_privacy_zone(backend);
            if zone == PrivacyZone::Open {
                return score / 2;
            }
        }
        score
    }
}

impl Reconciler for SchedulerReconciler {
    fn name(&self) -> &'static str {
        "SchedulerReconciler"
    }

    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        let model = intent.resolved_model.clone();

        // T024-T025: Filter candidates by health and capabilities, with rejection reasons
        let candidate_ids: Vec<String> = intent.candidate_agents.clone();
        for agent_id in &candidate_ids {
            if let Some(backend) = self.registry.get_backend(agent_id) {
                // Check health status (FR-030: error_rate via health)
                if backend.status != BackendStatus::Healthy {
                    intent.exclude_agent(
                        agent_id.clone(),
                        "SchedulerReconciler",
                        format!("Backend unhealthy: {:?}", backend.status),
                        "Wait for backend to become healthy".to_string(),
                    );
                    continue;
                }

                // Check capability requirements
                if !Self::meets_requirements(&backend, &model, intent) {
                    let mut missing = Vec::new();
                    if let Some(m) = backend.models.iter().find(|m| m.id == model) {
                        if intent.requirements.needs_vision && !m.supports_vision {
                            missing.push("vision");
                        }
                        if intent.requirements.needs_tools && !m.supports_tools {
                            missing.push("tools");
                        }
                        if intent.requirements.needs_json_mode && !m.supports_json_mode {
                            missing.push("json_mode");
                        }
                        if intent.requirements.estimated_tokens > m.context_length {
                            missing.push("context_length");
                        }
                    } else {
                        missing.push("model_not_found");
                    }
                    intent.exclude_agent(
                        agent_id.clone(),
                        "SchedulerReconciler",
                        format!("Missing capabilities: {:?}", missing),
                        "Use a backend that supports the required capabilities".to_string(),
                    );
                }
            } else {
                // Backend no longer in registry
                intent.exclude_agent(
                    agent_id.clone(),
                    "SchedulerReconciler",
                    "Backend not found in registry".to_string(),
                    "Backend may have been removed".to_string(),
                );
            }
        }

        // T026-T028: Score remaining candidates and select best using routing strategy
        // Scoring and selection happen here but the result is stored in intent
        // for the pipeline's execute() to read.
        // We store the selected agent and route_reason in intent fields.
        if !intent.candidate_agents.is_empty() {
            let candidates: Vec<Backend> = intent
                .candidate_agents
                .iter()
                .filter_map(|id| self.registry.get_backend(id))
                .collect();

            if !candidates.is_empty() {
                let (selected_id, route_reason) = match self.strategy {
                    RoutingStrategy::Smart => {
                        let best = candidates
                            .iter()
                            .max_by_key(|b| {
                                let raw_score = score_backend(
                                    b.priority as u32,
                                    b.pending_requests.load(Ordering::Relaxed),
                                    b.avg_latency_ms.load(Ordering::Relaxed),
                                    &self.weights,
                                );
                                self.apply_budget_adjustment(raw_score, b, intent)
                            })
                            .unwrap();
                        let raw_score = score_backend(
                            best.priority as u32,
                            best.pending_requests.load(Ordering::Relaxed),
                            best.avg_latency_ms.load(Ordering::Relaxed),
                            &self.weights,
                        );
                        let adjusted_score = self.apply_budget_adjustment(raw_score, best, intent);
                        let reason = if candidates.len() == 1 {
                            "only_healthy_backend".to_string()
                        } else {
                            format!("highest_score:{}:{:.2}", best.name, adjusted_score)
                        };
                        (best.id.clone(), reason)
                    }
                    RoutingStrategy::RoundRobin => {
                        let counter = self.round_robin_counter.fetch_add(1, Ordering::Relaxed);
                        let index = (counter as usize) % candidates.len();
                        let selected = &candidates[index];
                        let reason = if candidates.len() == 1 {
                            "only_healthy_backend".to_string()
                        } else {
                            format!("round_robin:index_{}", index)
                        };
                        (selected.id.clone(), reason)
                    }
                    RoutingStrategy::PriorityOnly => {
                        let best = candidates.iter().min_by_key(|b| b.priority).unwrap();
                        let reason = if candidates.len() == 1 {
                            "only_healthy_backend".to_string()
                        } else {
                            format!("priority:{}:{}", best.name, best.priority)
                        };
                        (best.id.clone(), reason)
                    }
                    RoutingStrategy::Random => {
                        use std::collections::hash_map::RandomState;
                        use std::hash::BuildHasher;
                        let random_state = RandomState::new();
                        let random_value = random_state.hash_one(std::time::SystemTime::now());
                        let index = (random_value as usize) % candidates.len();
                        let selected = &candidates[index];
                        let reason = if candidates.len() == 1 {
                            "only_healthy_backend".to_string()
                        } else {
                            format!("random:{}", selected.name)
                        };
                        (selected.id.clone(), reason)
                    }
                };

                // Store the selected agent as the only candidate
                // and preserve route_reason in intent for the pipeline
                intent.candidate_agents = vec![selected_id];
                intent.route_reason = Some(route_reason);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model};
    use crate::routing::reconciler::intent::RoutingIntent;
    use crate::routing::RequestRequirements;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, AtomicU64};

    fn create_test_backend(
        id: &str,
        status: BackendStatus,
        model_id: &str,
        priority: i32,
        pending: u32,
        latency: u32,
    ) -> Backend {
        Backend {
            id: id.to_string(),
            name: id.to_string(),
            url: format!("http://{}", id),
            backend_type: BackendType::Ollama,
            status,
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
            priority,
            pending_requests: AtomicU32::new(pending),
            total_requests: AtomicU64::new(0),
            avg_latency_ms: AtomicU32::new(latency),
            discovery_source: DiscoverySource::Static,
            metadata: HashMap::new(),
        }
    }

    fn create_requirements(model: &str) -> RequestRequirements {
        RequestRequirements {
            model: model.to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        }
    }

    fn create_intent(model: &str, candidates: Vec<String>) -> RoutingIntent {
        RoutingIntent::new(
            "req-1".to_string(),
            model.to_string(),
            model.to_string(),
            create_requirements(model),
            candidates,
        )
    }

    #[test]
    fn excludes_unhealthy_backends() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_test_backend(
                "b1",
                BackendStatus::Healthy,
                "llama3:8b",
                1,
                0,
                50,
            ))
            .unwrap();
        registry
            .add_backend(create_test_backend(
                "b2",
                BackendStatus::Unhealthy,
                "llama3:8b",
                1,
                0,
                50,
            ))
            .unwrap();

        let scheduler = SchedulerReconciler::new(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            Arc::new(AtomicU64::new(0)),
        );

        let mut intent = create_intent("llama3:8b", vec!["b1".into(), "b2".into()]);
        scheduler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents, vec!["b1"]);
        assert_eq!(intent.excluded_agents.len(), 1);
        assert_eq!(intent.excluded_agents[0], "b2");
    }

    #[test]
    fn selects_best_scoring_backend_smart() {
        let registry = Arc::new(Registry::new());
        // b1: high priority (1), low latency
        registry
            .add_backend(create_test_backend(
                "b1",
                BackendStatus::Healthy,
                "llama3:8b",
                1,
                0,
                50,
            ))
            .unwrap();
        // b2: low priority (10), high latency
        registry
            .add_backend(create_test_backend(
                "b2",
                BackendStatus::Healthy,
                "llama3:8b",
                10,
                50,
                500,
            ))
            .unwrap();

        let scheduler = SchedulerReconciler::new(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            Arc::new(AtomicU64::new(0)),
        );

        let mut intent = create_intent("llama3:8b", vec!["b1".into(), "b2".into()]);
        scheduler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents, vec!["b1"]);
    }

    #[test]
    fn rejects_when_no_candidates_remain() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_test_backend(
                "b1",
                BackendStatus::Unhealthy,
                "llama3:8b",
                1,
                0,
                50,
            ))
            .unwrap();

        let scheduler = SchedulerReconciler::new(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            Arc::new(AtomicU64::new(0)),
        );

        let mut intent = create_intent("llama3:8b", vec!["b1".into()]);
        scheduler.reconcile(&mut intent).unwrap();

        assert!(intent.candidate_agents.is_empty());
        assert!(!intent.rejection_reasons.is_empty());
    }

    #[test]
    fn excludes_capability_mismatch() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_test_backend(
                "b1",
                BackendStatus::Healthy,
                "llama3:8b",
                1,
                0,
                50,
            ))
            .unwrap();

        let scheduler = SchedulerReconciler::new(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            Arc::new(AtomicU64::new(0)),
        );

        let mut intent = RoutingIntent::new(
            "req-1".to_string(),
            "llama3:8b".to_string(),
            "llama3:8b".to_string(),
            RequestRequirements {
                model: "llama3:8b".to_string(),
                estimated_tokens: 100,
                needs_vision: true, // requires vision, but backend doesn't support it
                needs_tools: false,
                needs_json_mode: false,
            prefers_streaming: false,
            },
            vec!["b1".into()],
        );

        scheduler.reconcile(&mut intent).unwrap();
        assert!(intent.candidate_agents.is_empty());
        assert!(intent.rejection_reasons[0].reason.contains("vision"));
    }

    #[test]
    fn round_robin_strategy() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_test_backend(
                "b1",
                BackendStatus::Healthy,
                "llama3:8b",
                1,
                0,
                50,
            ))
            .unwrap();
        registry
            .add_backend(create_test_backend(
                "b2",
                BackendStatus::Healthy,
                "llama3:8b",
                1,
                0,
                50,
            ))
            .unwrap();

        let counter = Arc::new(AtomicU64::new(0));
        let scheduler = SchedulerReconciler::new(
            registry.clone(),
            RoutingStrategy::RoundRobin,
            ScoringWeights::default(),
            counter,
        );

        // First call → b1 (index 0)
        let mut intent1 = create_intent("llama3:8b", vec!["b1".into(), "b2".into()]);
        scheduler.reconcile(&mut intent1).unwrap();
        let first = intent1.candidate_agents[0].clone();

        // Second call → b2 (index 1)
        let mut intent2 = create_intent("llama3:8b", vec!["b1".into(), "b2".into()]);
        scheduler.reconcile(&mut intent2).unwrap();
        let second = intent2.candidate_agents[0].clone();

        assert_ne!(first, second);
    }

    #[test]
    fn meets_requirements_no_model_found() {
        let backend = create_test_backend("b1", BackendStatus::Healthy, "llama3:8b", 1, 0, 50);
        let intent = create_intent("nonexistent-model", vec!["b1".into()]);
        assert!(!SchedulerReconciler::meets_requirements(
            &backend,
            "nonexistent-model",
            &intent
        ));
    }

    #[test]
    fn meets_requirements_all_satisfied() {
        let backend = create_test_backend("b1", BackendStatus::Healthy, "llama3:8b", 1, 0, 50);
        let intent = create_intent("llama3:8b", vec!["b1".into()]);
        assert!(SchedulerReconciler::meets_requirements(
            &backend,
            "llama3:8b",
            &intent
        ));
    }

    #[test]
    fn meets_requirements_vision_required_not_supported() {
        let backend = create_test_backend("b1", BackendStatus::Healthy, "llama3:8b", 1, 0, 50);
        let intent = RoutingIntent::new(
            "req-1".to_string(),
            "llama3:8b".to_string(),
            "llama3:8b".to_string(),
            RequestRequirements {
                model: "llama3:8b".to_string(),
                estimated_tokens: 100,
                needs_vision: true,
                needs_tools: false,
                needs_json_mode: false,
            prefers_streaming: false,
            },
            vec!["b1".into()],
        );
        assert!(!SchedulerReconciler::meets_requirements(
            &backend,
            "llama3:8b",
            &intent
        ));
    }

    #[test]
    fn meets_requirements_context_length_exceeded() {
        let backend = create_test_backend("b1", BackendStatus::Healthy, "llama3:8b", 1, 0, 50);
        let intent = RoutingIntent::new(
            "req-1".to_string(),
            "llama3:8b".to_string(),
            "llama3:8b".to_string(),
            RequestRequirements {
                model: "llama3:8b".to_string(),
                estimated_tokens: 999999, // Exceeds 4096
                needs_vision: false,
                needs_tools: false,
                needs_json_mode: false,
            prefers_streaming: false,
            },
            vec!["b1".into()],
        );
        assert!(!SchedulerReconciler::meets_requirements(
            &backend,
            "llama3:8b",
            &intent
        ));
    }

    #[test]
    fn priority_only_strategy() {
        let registry = Arc::new(Registry::new());
        // b1: priority 10 (lower is better)
        registry
            .add_backend(create_test_backend(
                "b1",
                BackendStatus::Healthy,
                "llama3:8b",
                10,
                0,
                50,
            ))
            .unwrap();
        // b2: priority 1 (best)
        registry
            .add_backend(create_test_backend(
                "b2",
                BackendStatus::Healthy,
                "llama3:8b",
                1,
                0,
                50,
            ))
            .unwrap();

        let scheduler = SchedulerReconciler::new(
            registry,
            RoutingStrategy::PriorityOnly,
            ScoringWeights::default(),
            Arc::new(AtomicU64::new(0)),
        );

        let mut intent = create_intent("llama3:8b", vec!["b1".into(), "b2".into()]);
        scheduler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents, vec!["b2"]);
    }

    #[test]
    fn excludes_backend_not_in_registry() {
        let registry = Arc::new(Registry::new());

        let scheduler = SchedulerReconciler::new(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            Arc::new(AtomicU64::new(0)),
        );

        let mut intent = create_intent("llama3:8b", vec!["ghost".into()]);
        scheduler.reconcile(&mut intent).unwrap();

        assert!(intent.candidate_agents.is_empty());
        assert!(intent.rejection_reasons[0]
            .reason
            .contains("not found in registry"));
    }

    #[test]
    fn single_candidate_reason_is_only_healthy() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_test_backend(
                "b1",
                BackendStatus::Healthy,
                "llama3:8b",
                1,
                0,
                50,
            ))
            .unwrap();

        let scheduler = SchedulerReconciler::new(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            Arc::new(AtomicU64::new(0)),
        );

        let mut intent = create_intent("llama3:8b", vec!["b1".into()]);
        scheduler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.route_reason.as_deref(), Some("only_healthy_backend"));
    }
}
