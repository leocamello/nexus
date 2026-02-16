//! Routing decision types
//!
//! Final output from the reconciler pipeline.

use super::intent::{CostEstimate, RejectionReason};

/// Final routing decision from pipeline (FR-003)
#[derive(Debug)]
pub enum RoutingDecision {
    /// Successful routing to an agent
    Route {
        /// Selected agent ID
        agent_id: String,

        /// Resolved model name (after alias expansion)
        model: String,

        /// Explanation of routing decision
        reason: String,

        /// Estimated cost for request
        cost_estimate: CostEstimate,
    },

    /// Agent is busy, queue or wait required
    Queue {
        /// Reason for queueing
        reason: String,

        /// Estimated wait time in milliseconds
        estimated_wait_ms: u64,

        /// Fallback agent if available
        fallback_agent: Option<String>,
    },

    /// No viable agents, request rejected
    Reject {
        /// Detailed rejection reasons from all reconcilers
        rejection_reasons: Vec<RejectionReason>,
    },
}
