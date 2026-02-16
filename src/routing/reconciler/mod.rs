//! Reconciler pipeline module
//!
//! Implements the reconciler pipeline architecture for intelligent routing decisions.
//! Each reconciler reads and annotates RoutingIntent without removing constraints.

pub mod budget;
pub mod decision;
pub mod intent;
pub mod privacy;
pub mod request_analyzer;
pub mod scheduler;
pub mod scheduling;

use crate::routing::error::RoutingError;
use decision::RoutingDecision;
use intent::RoutingIntent;

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
        // Execute each reconciler in sequence
        for reconciler in &self.reconcilers {
            reconciler.reconcile(intent)?;
        }

        // Convert intent to decision based on final state
        if intent.candidate_agents.is_empty() {
            Ok(RoutingDecision::Reject {
                rejection_reasons: intent.rejection_reasons.clone(),
            })
        } else {
            Ok(RoutingDecision::Route {
                agent_id: intent.candidate_agents[0].clone(),
                model: intent.resolved_model.clone(),
                reason: intent
                    .route_reason
                    .clone()
                    .unwrap_or_else(|| "Pipeline execution completed".to_string()),
                cost_estimate: intent.cost_estimate.clone(),
            })
        }
    }
}
