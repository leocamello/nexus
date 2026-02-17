//! QualityReconciler - filters agents by quality metrics
//!
//! Reads each candidate's AgentQualityMetrics and excludes agents with
//! error_rate_1h above the configured threshold.

use super::intent::RoutingIntent;
use super::Reconciler;
use crate::agent::quality::QualityMetricsStore;
use crate::config::QualityConfig;
use crate::routing::error::RoutingError;
use std::sync::Arc;

/// QualityReconciler filters agents by error rate and quality metrics.
///
/// # Pipeline Position
/// RequestAnalyzer → PrivacyReconciler → BudgetReconciler → TierReconciler
/// → **QualityReconciler** → SchedulerReconciler
///
/// # Behavior
/// - Agents with error_rate_1h > threshold are excluded
/// - Agents with no history (default metrics) pass through
/// - Rejection reasons populated for excluded agents
pub struct QualityReconciler {
    store: Arc<QualityMetricsStore>,
    config: QualityConfig,
}

impl QualityReconciler {
    /// Create a new QualityReconciler with quality metrics store.
    pub fn new(store: Arc<QualityMetricsStore>, config: QualityConfig) -> Self {
        Self { store, config }
    }
}

impl Reconciler for QualityReconciler {
    fn name(&self) -> &'static str {
        "QualityReconciler"
    }

    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        let candidates: Vec<String> = intent.candidate_agents.clone();

        for agent_id in &candidates {
            let metrics = self.store.get_metrics(agent_id);

            // Skip agents with no request history (default metrics)
            if metrics.request_count_1h == 0 && metrics.last_failure_ts.is_none() {
                continue;
            }

            if metrics.error_rate_1h >= self.config.error_rate_threshold {
                intent.exclude_agent(
                    agent_id.clone(),
                    "QualityReconciler",
                    format!(
                        "Error rate {:.1}% exceeds threshold \
                         {:.1}%",
                        metrics.error_rate_1h * 100.0,
                        self.config.error_rate_threshold * 100.0
                    ),
                    "Wait for agent error rate to decrease".to_string(),
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::reconciler::intent::RoutingIntent;
    use crate::routing::RequestRequirements;

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

    fn default_config() -> QualityConfig {
        QualityConfig::default()
    }

    fn make_store() -> Arc<QualityMetricsStore> {
        Arc::new(QualityMetricsStore::new(default_config()))
    }

    // ================================================================
    // T005: Unit tests for QualityReconciler with real filtering
    // ================================================================

    #[test]
    fn excludes_high_error_agents_above_threshold() {
        let store = make_store();
        // agent-healthy: 10 successes
        for _ in 0..10 {
            store.record_outcome("agent-healthy", true, 100);
        }
        // agent-failing: 75% errors (3 fail, 1 success)
        store.record_outcome("agent-failing", false, 100);
        store.record_outcome("agent-failing", false, 100);
        store.record_outcome("agent-failing", false, 100);
        store.record_outcome("agent-failing", true, 100);
        store.recompute_all();

        let reconciler = QualityReconciler::new(store, default_config());
        let mut intent = create_intent(
            "llama3:8b",
            vec!["agent-healthy".into(), "agent-failing".into()],
        );

        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents.len(), 1);
        assert_eq!(intent.candidate_agents[0], "agent-healthy");
        assert_eq!(intent.excluded_agents.len(), 1);
        assert!(!intent.rejection_reasons.is_empty());
    }

    #[test]
    fn preserves_healthy_agents_below_threshold() {
        let store = make_store();
        // Both agents: 20% error rate (below 50% threshold)
        for _ in 0..8 {
            store.record_outcome("agent-1", true, 100);
        }
        for _ in 0..2 {
            store.record_outcome("agent-1", false, 100);
        }
        for _ in 0..9 {
            store.record_outcome("agent-2", true, 100);
        }
        store.record_outcome("agent-2", false, 100);
        store.recompute_all();

        let reconciler = QualityReconciler::new(store, default_config());
        let mut intent = create_intent("llama3:8b", vec!["agent-1".into(), "agent-2".into()]);

        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents.len(), 2);
        assert!(intent.excluded_agents.is_empty());
    }

    #[test]
    fn all_excluded_produces_rejection_reasons() {
        let store = make_store();
        // Both agents: 100% errors
        for _ in 0..5 {
            store.record_outcome("agent-bad-1", false, 100);
            store.record_outcome("agent-bad-2", false, 100);
        }
        store.recompute_all();

        let reconciler = QualityReconciler::new(store, default_config());
        let mut intent = create_intent(
            "llama3:8b",
            vec!["agent-bad-1".into(), "agent-bad-2".into()],
        );

        reconciler.reconcile(&mut intent).unwrap();

        assert!(intent.candidate_agents.is_empty());
        assert_eq!(intent.excluded_agents.len(), 2);
        assert!(intent.rejection_reasons.len() >= 2);
    }

    #[test]
    fn fresh_start_no_history_all_pass() {
        let store = make_store();
        // No outcomes recorded — default metrics
        let reconciler = QualityReconciler::new(store, default_config());
        let mut intent = create_intent(
            "llama3:8b",
            vec!["new-agent-1".into(), "new-agent-2".into()],
        );

        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents.len(), 2);
        assert!(intent.excluded_agents.is_empty());
    }

    // ================================================================
    // Original pass-through tests (adapted for new constructor)
    // ================================================================

    #[test]
    fn pass_through_preserves_all_candidates() {
        let store = make_store();
        let reconciler = QualityReconciler::new(store, default_config());
        let mut intent = create_intent(
            "llama3:8b",
            vec!["agent-1".into(), "agent-2".into(), "agent-3".into()],
        );

        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents.len(), 3);
        assert!(intent.excluded_agents.is_empty());
        assert!(intent.rejection_reasons.is_empty());
    }

    #[test]
    fn pass_through_with_empty_candidates() {
        let store = make_store();
        let reconciler = QualityReconciler::new(store, default_config());
        let mut intent = create_intent("llama3:8b", vec![]);

        reconciler.reconcile(&mut intent).unwrap();

        assert!(intent.candidate_agents.is_empty());
        assert!(intent.excluded_agents.is_empty());
    }

    #[test]
    fn name_returns_quality_reconciler() {
        let store = make_store();
        let reconciler = QualityReconciler::new(store, default_config());
        assert_eq!(reconciler.name(), "QualityReconciler");
    }

    #[test]
    fn default_creates_pass_through() {
        let store = make_store();
        let reconciler = QualityReconciler::new(store, default_config());
        let mut intent = create_intent("test-model", vec!["a".into()]);

        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents, vec!["a"]);
    }
}
