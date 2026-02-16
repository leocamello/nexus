//! Agent scheduling profile for reconciler pipeline
//!
//! Combines agent metadata, backend metrics, and quality metrics.

use crate::agent::PrivacyZone;
use crate::registry::{Backend, BackendStatus};
use std::sync::atomic::Ordering;

/// Scheduling profile for an agent (combines AgentProfile + Backend metrics)
#[derive(Debug, Clone)]
pub struct AgentSchedulingProfile {
    /// Agent ID
    pub agent_id: String,

    /// Privacy zone
    pub privacy_zone: PrivacyZone,

    /// Backend type string
    pub backend_type: String,

    /// Current health status
    pub is_healthy: bool,

    /// Average latency (milliseconds)
    pub avg_latency_ms: u32,

    /// Current pending requests
    pub pending_requests: u32,

    /// Capability tier (if specified)
    pub capability_tier: Option<u8>,
}

impl AgentSchedulingProfile {
    /// Create scheduling profile from backend
    pub fn from_backend(
        backend: &Backend,
        privacy_zone: PrivacyZone,
        capability_tier: Option<u8>,
    ) -> Self {
        let is_healthy = matches!(backend.status, BackendStatus::Healthy);
        let avg_latency_ms = backend.avg_latency_ms.load(Ordering::Relaxed);
        let pending_requests = backend.pending_requests.load(Ordering::Relaxed);

        Self {
            agent_id: backend.id.clone(),
            privacy_zone,
            backend_type: format!("{:?}", backend.backend_type),
            is_healthy,
            avg_latency_ms,
            pending_requests,
            capability_tier,
        }
    }

    /// Get capability tier (FR-025)
    pub fn capability_tier(&self) -> u8 {
        self.capability_tier.unwrap_or(1)
    }

    /// Calculate error rate (stub for now - will need metrics tracking)
    pub fn error_rate(&self) -> f64 {
        0.0 // TODO: Implement with metrics tracking
    }

    /// Calculate success rate (stub for now - will need metrics tracking)
    pub fn success_rate(&self) -> f64 {
        1.0 // TODO: Implement with metrics tracking
    }

    /// Calculate average TTFT (stub for now - will need metrics tracking)
    pub fn avg_ttft_ms(&self) -> f64 {
        self.avg_latency_ms as f64 // Approximate with latency for now
    }
}
