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
