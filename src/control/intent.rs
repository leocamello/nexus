//! Routing intent and annotations

use crate::control::budget::{BudgetStatus, BudgetViolation};
use crate::control::capability::CapabilityMismatch;
use crate::control::decision::RoutingDecision;
use crate::control::privacy::{PrivacyConstraint, PrivacyViolation};
use crate::registry::Backend;
use crate::routing::requirements::RequestRequirements;
use std::collections::HashMap;
use std::sync::Arc;

/// Shared routing state annotated by reconcilers
#[derive(Debug, Clone)]
pub struct RoutingIntent {
    /// Original request requirements (immutable)
    pub request_requirements: RequestRequirements,

    /// Candidate backends (filtered by reconcilers)
    pub candidate_backends: Vec<Arc<Backend>>,

    /// Policy annotations (written by reconcilers)
    pub annotations: RoutingAnnotations,

    /// Final routing decision (set by SelectionReconciler)
    pub decision: Option<RoutingDecision>,
}

impl RoutingIntent {
    /// Create new intent from request requirements and candidates
    pub fn new(
        request_requirements: RequestRequirements,
        candidate_backends: Vec<Arc<Backend>>,
    ) -> Self {
        Self {
            request_requirements,
            candidate_backends,
            annotations: RoutingAnnotations::default(),
            decision: None,
        }
    }

    /// Check if any backend is available after filtering
    pub fn has_candidates(&self) -> bool {
        !self.candidate_backends.is_empty()
    }

    /// Add trace information for observability
    pub fn trace(&mut self, message: impl Into<String>) {
        self.annotations.trace_info.push(message.into());
    }
}

/// Policy annotations added by reconcilers
#[derive(Debug, Clone, Default)]
pub struct RoutingAnnotations {
    // Privacy Policy
    /// Privacy constraints extracted from request or user profile
    pub privacy_constraints: Option<PrivacyConstraint>,

    /// Backends excluded due to privacy violations
    pub privacy_excluded: HashMap<String, PrivacyViolation>,

    // Budget Policy
    /// Estimated cost for this request (simple f64 for backwards compat)
    pub estimated_cost: Option<f64>,

    /// Detailed cost estimate with token counts and tier
    pub cost_estimate: Option<crate::control::budget::CostEstimate>,

    /// Current budget status (normal, soft limit, hard limit)
    pub budget_status: Option<BudgetStatus>,

    /// Backends excluded due to budget constraints
    pub budget_excluded: HashMap<String, BudgetViolation>,

    // Capability Policy
    /// Required capability tier (if specified)
    pub required_tier: Option<u8>,

    /// Backends excluded due to capability mismatches
    pub capability_excluded: HashMap<String, CapabilityMismatch>,

    // Observability
    /// Trace messages for debugging and audit
    pub trace_info: Vec<String>,

    /// Whether fallback model was used
    pub fallback_used: bool,
}
