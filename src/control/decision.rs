//! Final routing decision

use crate::registry::Backend;
use std::sync::Arc;

/// Final routing decision after pipeline evaluation
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    /// Selected backend
    pub backend: Arc<Backend>,

    /// Reason for selection (for observability)
    pub reason: String,

    /// Score used for selection (if applicable)
    pub score: Option<f64>,
}

impl RoutingDecision {
    /// Create decision from backend and reason
    pub fn new(backend: Arc<Backend>, reason: impl Into<String>) -> Self {
        Self {
            backend,
            reason: reason.into(),
            score: None,
        }
    }

    /// Create decision with score
    pub fn with_score(backend: Arc<Backend>, reason: impl Into<String>, score: f64) -> Self {
        Self {
            backend,
            reason: reason.into(),
            score: Some(score),
        }
    }
}
