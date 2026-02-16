//! Control plane reconciler pipeline
//!
//! This module implements RFC-001 Phase 2: A pipeline architecture for routing decisions
//! that enables independent policy evaluation without O(n²) feature interaction complexity.
//!
//! # Architecture
//!
//! The reconciler pipeline uses a Chain of Responsibility pattern where each Reconciler
//! annotates a shared RoutingIntent state object:
//!
//! 1. **PrivacyReconciler**: Filters backends by privacy zone
//! 2. **BudgetReconciler**: Annotates with cost estimates and budget status
//! 3. **CapabilityReconciler**: Filters backends by capability tier
//! 4. **SelectionReconciler**: Selects final backend from remaining candidates
//!
//! # Performance
//!
//! Target: <500μs total pipeline execution
//! - Privacy filtering: <50μs
//! - Budget annotation: <100μs
//! - Capability matching: <50μs
//! - Backend selection: <200μs

pub mod budget;
pub mod capability;
pub mod decision;
pub mod intent;
pub mod privacy;
pub mod reconciler;
pub mod selection;

pub use decision::RoutingDecision;
pub use intent::{RoutingAnnotations, RoutingIntent};
pub use reconciler::{ReconcileError, ReconcileErrorPolicy, Reconciler, ReconcilerPipeline};

use crate::agent::types::PrivacyZone;
use std::sync::Arc;

/// Reason a backend was rejected during reconciliation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectionReason {
    /// Privacy zone mismatch
    PrivacyZoneMismatch {
        required_zone: PrivacyZone,
        backend_zone: PrivacyZone,
    },
    /// Insufficient reasoning capability
    TierInsufficientReasoning { required: u8, actual: u8 },
    /// Insufficient coding capability
    TierInsufficientCoding { required: u8, actual: u8 },
    /// Insufficient context window
    TierInsufficientContext { required: u32, actual: u32 },
    /// Missing vision capability
    MissingVision,
    /// Missing tools capability
    MissingTools,
    /// Cross-zone overflow blocked due to conversation history
    OverflowBlockedWithHistory {
        from_zone: PrivacyZone,
        to_zone: PrivacyZone,
    },
    /// Cross-zone overflow blocked by policy
    OverflowBlockedByPolicy {
        from_zone: PrivacyZone,
        to_zone: PrivacyZone,
    },
}

impl RejectionReason {
    /// Get human-readable error message
    pub fn message(&self) -> String {
        match self {
            RejectionReason::PrivacyZoneMismatch {
                required_zone,
                backend_zone,
            } => format!(
                "Backend zone {:?} does not match required zone {:?}",
                backend_zone, required_zone
            ),
            RejectionReason::TierInsufficientReasoning { required, actual } => {
                format!(
                    "Backend reasoning score {} is below required {}",
                    actual, required
                )
            }
            RejectionReason::TierInsufficientCoding { required, actual } => {
                format!(
                    "Backend coding score {} is below required {}",
                    actual, required
                )
            }
            RejectionReason::TierInsufficientContext { required, actual } => {
                format!(
                    "Backend context window {} is below required {}",
                    actual, required
                )
            }
            RejectionReason::MissingVision => {
                "Backend does not support vision capability".to_string()
            }
            RejectionReason::MissingTools => {
                "Backend does not support tools capability".to_string()
            }
            RejectionReason::OverflowBlockedWithHistory { from_zone, to_zone } => {
                format!(
                    "Cross-zone overflow from {:?} to {:?} blocked due to conversation history",
                    from_zone, to_zone
                )
            }
            RejectionReason::OverflowBlockedByPolicy { from_zone, to_zone } => {
                format!(
                    "Cross-zone overflow from {:?} to {:?} blocked by policy",
                    from_zone, to_zone
                )
            }
        }
    }
}

/// Builder for constructing reconciler pipelines
pub struct PipelineBuilder {
    reconcilers: Vec<Arc<dyn Reconciler>>,
}

impl PipelineBuilder {
    /// Create a new pipeline builder
    pub fn new() -> Self {
        Self {
            reconcilers: Vec::new(),
        }
    }

    /// Add a reconciler to the pipeline
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, reconciler: Arc<dyn Reconciler>) -> Self {
        self.reconcilers.push(reconciler);
        self
    }

    /// Build the pipeline
    pub fn build(self) -> ReconcilerPipeline {
        ReconcilerPipeline::new(self.reconcilers)
    }
}

impl Default for PipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}
