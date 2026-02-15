//! Core reconciler trait and pipeline execution

use crate::control::intent::RoutingIntent;
use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;

/// Errors during reconciler execution
#[derive(Debug, Error)]
pub enum ReconcileError {
    #[error("No backends available after filtering")]
    NoCandidates,

    #[error("Privacy policy violation: {0}")]
    PrivacyViolation(String),

    #[error("Budget service unavailable: {0}")]
    BudgetServiceError(String),

    #[error("Required capability not available: {0}")]
    CapabilityUnavailable(String),

    #[error("Selection failed: {0}")]
    SelectionFailed(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Error handling policy for reconcilers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileErrorPolicy {
    /// Log warning and continue pipeline (graceful degradation)
    FailOpen,

    /// Stop pipeline and return error (strict enforcement)
    FailClosed,
}

/// Policy reconciler that annotates routing intent
///
/// # Contract
///
/// Reconcilers must:
/// - Be Send + Sync (thread-safe)
/// - Not panic (return ReconcileError instead)
/// - Complete in <100μs for CPU-bound operations
/// - Not modify immutable fields of RoutingIntent
///
/// Reconcilers can:
/// - Filter candidate_backends
/// - Add annotations
/// - Set decision (SelectionReconciler only)
#[async_trait]
pub trait Reconciler: Send + Sync {
    /// Reconcile policy with routing intent
    ///
    /// # Errors
    ///
    /// Returns Err if policy evaluation fails critically
    async fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), ReconcileError>;

    /// Error handling policy for this reconciler
    fn error_policy(&self) -> ReconcileErrorPolicy {
        ReconcileErrorPolicy::FailOpen
    }

    /// Name for logging and debugging
    fn name(&self) -> &str;
}

/// Pipeline of reconcilers executed sequentially
pub struct ReconcilerPipeline {
    reconcilers: Vec<Arc<dyn Reconciler>>,
}

impl ReconcilerPipeline {
    /// Create new pipeline with reconcilers
    pub fn new(reconcilers: Vec<Arc<dyn Reconciler>>) -> Self {
        Self { reconcilers }
    }

    /// Execute pipeline on routing intent
    pub async fn execute(&self, intent: &mut RoutingIntent) -> Result<(), ReconcileError> {
        for reconciler in &self.reconcilers {
            // Execute reconciler
            let result = reconciler.reconcile(intent).await;

            // Handle errors based on policy
            if let Err(err) = result {
                match reconciler.error_policy() {
                    ReconcileErrorPolicy::FailOpen => {
                        tracing::warn!(
                            reconciler = reconciler.name(),
                            error = %err,
                            "Reconciler failed, continuing pipeline"
                        );
                        intent.trace(format!("⚠️  {} failed: {}", reconciler.name(), err));
                        continue;
                    }
                    ReconcileErrorPolicy::FailClosed => {
                        tracing::error!(
                            reconciler = reconciler.name(),
                            error = %err,
                            "Reconciler failed, stopping pipeline"
                        );
                        return Err(err);
                    }
                }
            }

            // Log success
            intent.trace(format!("✓ {}", reconciler.name()));
        }

        Ok(())
    }

    /// Get list of reconcilers in pipeline
    pub fn list_reconcilers(&self) -> Vec<&str> {
        self.reconcilers.iter().map(|r| r.name()).collect()
    }

    /// Get reconciler by name
    pub fn get_reconciler_by_name(&self, name: &str) -> Option<&Arc<dyn Reconciler>> {
        self.reconcilers.iter().find(|r| r.name() == name)
    }
}
