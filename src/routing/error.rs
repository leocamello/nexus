//! Error types for routing failures

use crate::routing::reconciler::intent::RejectionReason;
use thiserror::Error;

/// Errors that can occur during backend selection
#[derive(Debug, Error)]
pub enum RoutingError {
    /// The requested model was not found in any backend
    #[error("Model '{model}' not found")]
    ModelNotFound { model: String },

    /// No healthy backend is available for the requested model
    #[error("No healthy backend available for model '{model}'")]
    NoHealthyBackend { model: String },

    /// No backend supports the required capabilities
    #[error("No backend supports required capabilities for model '{model}': {missing:?}")]
    CapabilityMismatch { model: String, missing: Vec<String> },

    /// All models in the fallback chain were exhausted
    #[error("All backends in fallback chain unavailable: {chain:?}")]
    FallbackChainExhausted { chain: Vec<String> },

    /// Request rejected with detailed reasons from reconcilers (FR-004)
    #[error("Request rejected by reconciliation pipeline")]
    Reject {
        rejection_reasons: Vec<RejectionReason>,
    },

    /// Request should be queued (all backends at capacity, queue enabled)
    #[error("All backends at capacity, request queued")]
    Queue {
        reason: String,
        estimated_wait_ms: u64,
    },
}
