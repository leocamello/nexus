//! QualityReconciler - Reserved for future quality metrics
//!
//! Phase 1: pass-through only. This reconciler does not filter or exclude
//! any agents. It serves as a placeholder in the pipeline for future
//! quality-based routing decisions (e.g., response quality tracking,
//! model benchmark scores, user satisfaction metrics).

use super::intent::RoutingIntent;
use super::Reconciler;
use crate::routing::error::RoutingError;

/// QualityReconciler is a pass-through reconciler reserved for future quality metrics.
///
/// # Pipeline Position
/// RequestAnalyzer → PrivacyReconciler → BudgetReconciler → TierReconciler
/// → **QualityReconciler** → SchedulerReconciler
///
/// # Current Behavior (Phase 1)
/// No filtering — all candidates pass through unchanged.
///
/// # Future Behavior
/// - Track response quality per agent (latency, accuracy, user ratings)
/// - Exclude agents below quality thresholds
/// - Prefer agents with higher quality scores
pub struct QualityReconciler;

impl QualityReconciler {
    /// Create a new QualityReconciler (no state needed for pass-through).
    pub fn new() -> Self {
        Self
    }
}

impl Default for QualityReconciler {
    fn default() -> Self {
        Self::new()
    }
}

impl Reconciler for QualityReconciler {
    fn name(&self) -> &'static str {
        "QualityReconciler"
    }

    fn reconcile(&self, _intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        // Reserved for future quality metrics — Phase 1: pass-through only
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
            },
            candidates,
        )
    }

    #[test]
    fn pass_through_preserves_all_candidates() {
        let reconciler = QualityReconciler::new();
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
        let reconciler = QualityReconciler::new();
        let mut intent = create_intent("llama3:8b", vec![]);

        reconciler.reconcile(&mut intent).unwrap();

        assert!(intent.candidate_agents.is_empty());
        assert!(intent.excluded_agents.is_empty());
    }

    #[test]
    fn name_returns_quality_reconciler() {
        let reconciler = QualityReconciler::new();
        assert_eq!(reconciler.name(), "QualityReconciler");
    }

    #[test]
    fn default_creates_pass_through() {
        let reconciler = QualityReconciler;
        let mut intent = create_intent("test-model", vec!["a".into()]);

        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents, vec!["a"]);
    }
}
