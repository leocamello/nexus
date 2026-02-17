//! Reconciler pipeline module
//!
//! Implements the reconciler pipeline architecture for intelligent routing decisions.
//! Each reconciler reads and annotates RoutingIntent without removing constraints.

pub mod budget;
pub mod decision;
pub mod intent;
pub mod privacy;
pub mod quality;
pub mod request_analyzer;
pub mod scheduler;
pub mod scheduling;
pub mod tier;

use crate::routing::error::RoutingError;
use decision::RoutingDecision;
use intent::RoutingIntent;
use std::time::Instant;

/// Reconciler trait for pipeline stages.
/// Each reconciler annotates RoutingIntent without removing prior constraints.
/// Order-independent: reconcilers only add constraints, never remove.
pub trait Reconciler: Send + Sync {
    /// Returns reconciler identifier for logging and rejection reasons.
    fn name(&self) -> &'static str;

    /// Reconcile routing intent based on reconciler's domain.
    ///
    /// # Behavior
    /// - Read requirements, constraints, and candidate agents from intent
    /// - Add constraints to intent (privacy, budget, tier, etc.)
    /// - Move agents from candidates to excluded with RejectionReason
    /// - NEVER remove constraints or rejection reasons from prior reconcilers
    ///
    /// # Returns
    /// - Ok(()) if reconciliation succeeded (even if all agents excluded)
    /// - Err(RoutingError) only for catastrophic failures (e.g., config missing)
    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError>;
}

/// ReconcilerPipeline executes a sequence of reconcilers on routing intent.
/// Order is fixed: RequestAnalyzer → Privacy → Budget → Tier → Quality → Scheduler
pub struct ReconcilerPipeline {
    reconcilers: Vec<Box<dyn Reconciler>>,
}

impl ReconcilerPipeline {
    /// Create a new pipeline with the given reconcilers.
    /// Reconcilers will execute in the order provided.
    pub fn new(reconcilers: Vec<Box<dyn Reconciler>>) -> Self {
        Self { reconcilers }
    }

    /// Returns the number of reconcilers in the pipeline.
    pub fn len(&self) -> usize {
        self.reconcilers.len()
    }

    /// Returns true if the pipeline has no reconcilers.
    pub fn is_empty(&self) -> bool {
        self.reconcilers.is_empty()
    }

    /// Execute the pipeline on the given routing intent.
    /// Returns a RoutingDecision based on the final state of the intent.
    ///
    /// # Algorithm (FR-005)
    /// 1. Execute each reconciler in order
    /// 2. If any reconciler fails, return error immediately
    /// 3. After all reconcilers complete, convert intent to decision
    pub fn execute(&mut self, intent: &mut RoutingIntent) -> Result<RoutingDecision, RoutingError> {
        let pipeline_start = Instant::now();

        tracing::trace!(
            request_id = %intent.request_id,
            model = %intent.requested_model,
            reconciler_count = self.reconcilers.len(),
            "Pipeline execution started"
        );

        // Execute each reconciler in sequence with per-reconciler timing
        for reconciler in &self.reconcilers {
            let reconciler_start = Instant::now();
            let candidates_before = intent.candidate_agents.len();

            reconciler.reconcile(intent)?;

            let reconciler_elapsed = reconciler_start.elapsed();
            let excluded_count = candidates_before.saturating_sub(intent.candidate_agents.len());

            // T093: Per-reconciler latency histogram
            metrics::histogram!(
                "nexus_reconciler_duration_seconds",
                "reconciler" => reconciler.name().to_string(),
            )
            .record(reconciler_elapsed.as_secs_f64());

            // T093: Per-reconciler exclusion counter
            if excluded_count > 0 {
                metrics::counter!(
                    "nexus_reconciler_exclusions_total",
                    "reconciler" => reconciler.name().to_string(),
                )
                .increment(excluded_count as u64);
            }

            tracing::trace!(
                request_id = %intent.request_id,
                reconciler = reconciler.name(),
                elapsed_us = reconciler_elapsed.as_micros() as u64,
                candidates_remaining = intent.candidate_agents.len(),
                excluded = excluded_count,
                "Reconciler completed"
            );
        }

        let pipeline_elapsed = pipeline_start.elapsed();

        // T093: Pipeline total latency histogram
        metrics::histogram!("nexus_pipeline_duration_seconds")
            .record(pipeline_elapsed.as_secs_f64());

        // Convert intent to decision based on final state
        let decision = if intent.candidate_agents.is_empty() {
            RoutingDecision::Reject {
                rejection_reasons: intent.rejection_reasons.clone(),
            }
        } else {
            RoutingDecision::Route {
                agent_id: intent.candidate_agents[0].clone(),
                model: intent.resolved_model.clone(),
                reason: intent
                    .route_reason
                    .clone()
                    .unwrap_or_else(|| "Pipeline execution completed".to_string()),
                cost_estimate: intent.cost_estimate.clone(),
            }
        };

        tracing::trace!(
            request_id = %intent.request_id,
            elapsed_us = pipeline_elapsed.as_micros() as u64,
            decision = ?std::mem::discriminant(&decision),
            "Pipeline execution completed"
        );

        Ok(decision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::RequestRequirements;

    /// A mock reconciler that passes through without modifying intent.
    struct PassthroughReconciler;
    impl Reconciler for PassthroughReconciler {
        fn name(&self) -> &'static str {
            "PassthroughReconciler"
        }
        fn reconcile(&self, _intent: &mut RoutingIntent) -> Result<(), RoutingError> {
            Ok(())
        }
    }

    /// A mock reconciler that excludes a specific agent.
    struct ExcludeReconciler {
        agent_id: String,
    }
    impl Reconciler for ExcludeReconciler {
        fn name(&self) -> &'static str {
            "ExcludeReconciler"
        }
        fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
            intent.exclude_agent(
                self.agent_id.clone(),
                "ExcludeReconciler",
                "test exclusion".to_string(),
                "test action".to_string(),
            );
            Ok(())
        }
    }

    /// A mock reconciler that always fails.
    struct FailingReconciler;
    impl Reconciler for FailingReconciler {
        fn name(&self) -> &'static str {
            "FailingReconciler"
        }
        fn reconcile(&self, _intent: &mut RoutingIntent) -> Result<(), RoutingError> {
            Err(RoutingError::NoHealthyBackend {
                model: "test".to_string(),
            })
        }
    }

    fn create_intent(candidates: Vec<&str>) -> RoutingIntent {
        RoutingIntent::new(
            "req-1".to_string(),
            "llama3:8b".to_string(),
            "llama3:8b".to_string(),
            RequestRequirements {
                model: "llama3:8b".to_string(),
                estimated_tokens: 100,
                needs_vision: false,
                needs_tools: false,
                needs_json_mode: false,
                prefers_streaming: false,
            },
            candidates.into_iter().map(|s| s.to_string()).collect(),
        )
    }

    #[test]
    fn pipeline_new_creates_empty() {
        let pipeline = ReconcilerPipeline::new(vec![]);
        assert!(pipeline.is_empty());
        assert_eq!(pipeline.len(), 0);
    }

    #[test]
    fn pipeline_with_reconcilers_reports_length() {
        let pipeline = ReconcilerPipeline::new(vec![
            Box::new(PassthroughReconciler),
            Box::new(PassthroughReconciler),
        ]);
        assert_eq!(pipeline.len(), 2);
        assert!(!pipeline.is_empty());
    }

    #[test]
    fn empty_pipeline_routes_first_candidate() {
        let mut pipeline = ReconcilerPipeline::new(vec![]);
        let mut intent = create_intent(vec!["b1", "b2"]);
        let decision = pipeline.execute(&mut intent).unwrap();
        match decision {
            RoutingDecision::Route { agent_id, .. } => assert_eq!(agent_id, "b1"),
            _ => panic!("Expected Route decision"),
        }
    }

    #[test]
    fn empty_pipeline_no_candidates_rejects() {
        let mut pipeline = ReconcilerPipeline::new(vec![]);
        let mut intent = create_intent(vec![]);
        let decision = pipeline.execute(&mut intent).unwrap();
        assert!(matches!(decision, RoutingDecision::Reject { .. }));
    }

    #[test]
    fn passthrough_reconciler_preserves_candidates() {
        let mut pipeline = ReconcilerPipeline::new(vec![Box::new(PassthroughReconciler)]);
        let mut intent = create_intent(vec!["b1", "b2"]);
        let decision = pipeline.execute(&mut intent).unwrap();
        match decision {
            RoutingDecision::Route { agent_id, .. } => assert_eq!(agent_id, "b1"),
            _ => panic!("Expected Route decision"),
        }
    }

    #[test]
    fn exclude_reconciler_removes_agent() {
        let mut pipeline = ReconcilerPipeline::new(vec![Box::new(ExcludeReconciler {
            agent_id: "b1".to_string(),
        })]);
        let mut intent = create_intent(vec!["b1", "b2"]);
        let decision = pipeline.execute(&mut intent).unwrap();
        match decision {
            RoutingDecision::Route { agent_id, .. } => assert_eq!(agent_id, "b2"),
            _ => panic!("Expected Route decision"),
        }
    }

    #[test]
    fn exclude_all_candidates_produces_reject() {
        let mut pipeline = ReconcilerPipeline::new(vec![
            Box::new(ExcludeReconciler {
                agent_id: "b1".to_string(),
            }),
            Box::new(ExcludeReconciler {
                agent_id: "b2".to_string(),
            }),
        ]);
        let mut intent = create_intent(vec!["b1", "b2"]);
        let decision = pipeline.execute(&mut intent).unwrap();
        match decision {
            RoutingDecision::Reject {
                rejection_reasons, ..
            } => {
                assert_eq!(rejection_reasons.len(), 2);
                assert_eq!(rejection_reasons[0].agent_id, "b1");
                assert_eq!(rejection_reasons[1].agent_id, "b2");
            }
            _ => panic!("Expected Reject decision"),
        }
    }

    #[test]
    fn failing_reconciler_returns_error() {
        let mut pipeline = ReconcilerPipeline::new(vec![Box::new(FailingReconciler)]);
        let mut intent = create_intent(vec!["b1"]);
        let result = pipeline.execute(&mut intent);
        assert!(result.is_err());
    }

    #[test]
    fn failing_reconciler_stops_pipeline() {
        let mut pipeline = ReconcilerPipeline::new(vec![
            Box::new(FailingReconciler),
            Box::new(ExcludeReconciler {
                agent_id: "b1".to_string(),
            }),
        ]);
        let mut intent = create_intent(vec!["b1"]);
        let result = pipeline.execute(&mut intent);
        assert!(result.is_err());
        // Second reconciler should not have run
        assert!(intent.excluded_agents.is_empty());
    }

    #[test]
    fn route_reason_is_preserved() {
        struct ReasonReconciler;
        impl Reconciler for ReasonReconciler {
            fn name(&self) -> &'static str {
                "ReasonReconciler"
            }
            fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
                intent.route_reason = Some("custom_reason".to_string());
                Ok(())
            }
        }

        let mut pipeline = ReconcilerPipeline::new(vec![Box::new(ReasonReconciler)]);
        let mut intent = create_intent(vec!["b1"]);
        let decision = pipeline.execute(&mut intent).unwrap();
        match decision {
            RoutingDecision::Route { reason, .. } => assert_eq!(reason, "custom_reason"),
            _ => panic!("Expected Route decision"),
        }
    }

    #[test]
    fn default_route_reason_when_none_set() {
        let mut pipeline = ReconcilerPipeline::new(vec![Box::new(PassthroughReconciler)]);
        let mut intent = create_intent(vec!["b1"]);
        let decision = pipeline.execute(&mut intent).unwrap();
        match decision {
            RoutingDecision::Route { reason, .. } => {
                assert_eq!(reason, "Pipeline execution completed")
            }
            _ => panic!("Expected Route decision"),
        }
    }

    #[test]
    fn reconcilers_execute_in_order() {
        use std::sync::{Arc, Mutex};

        let order = Arc::new(Mutex::new(Vec::new()));

        struct OrderReconciler {
            id: String,
            order: Arc<Mutex<Vec<String>>>,
        }
        impl Reconciler for OrderReconciler {
            fn name(&self) -> &'static str {
                "OrderReconciler"
            }
            fn reconcile(&self, _intent: &mut RoutingIntent) -> Result<(), RoutingError> {
                self.order.lock().unwrap().push(self.id.clone());
                Ok(())
            }
        }

        let mut pipeline = ReconcilerPipeline::new(vec![
            Box::new(OrderReconciler {
                id: "first".to_string(),
                order: order.clone(),
            }),
            Box::new(OrderReconciler {
                id: "second".to_string(),
                order: order.clone(),
            }),
            Box::new(OrderReconciler {
                id: "third".to_string(),
                order: order.clone(),
            }),
        ]);

        let mut intent = create_intent(vec!["b1"]);
        pipeline.execute(&mut intent).unwrap();

        let executed = order.lock().unwrap();
        assert_eq!(*executed, vec!["first", "second", "third"]);
    }
}
