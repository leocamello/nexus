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

use std::sync::Arc;

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
