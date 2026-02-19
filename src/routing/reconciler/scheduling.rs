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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{BackendType, DiscoverySource, Model};
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, AtomicU64};

    fn create_backend(
        id: &str,
        status: BackendStatus,
        backend_type: BackendType,
        pending: u32,
        latency: u32,
    ) -> Backend {
        Backend {
            id: id.to_string(),
            name: id.to_string(),
            url: format!("http://{}", id),
            backend_type,
            status,
            last_health_check: Utc::now(),
            last_error: None,
            models: vec![Model {
                id: "test-model".to_string(),
                name: "test-model".to_string(),
                context_length: 4096,
                supports_vision: false,
                supports_tools: false,
                supports_json_mode: false,
                max_output_tokens: None,
            }],
            priority: 1,
            pending_requests: AtomicU32::new(pending),
            total_requests: AtomicU64::new(100),
            avg_latency_ms: AtomicU32::new(latency),
            discovery_source: DiscoverySource::Static,
            metadata: HashMap::new(),
            current_operation: None,
        }
    }

    #[test]
    fn from_backend_healthy() {
        let backend = create_backend("b1", BackendStatus::Healthy, BackendType::Ollama, 5, 50);
        let profile =
            AgentSchedulingProfile::from_backend(&backend, PrivacyZone::Restricted, Some(3));

        assert_eq!(profile.agent_id, "b1");
        assert_eq!(profile.privacy_zone, PrivacyZone::Restricted);
        assert!(profile.is_healthy);
        assert_eq!(profile.avg_latency_ms, 50);
        assert_eq!(profile.pending_requests, 5);
        assert_eq!(profile.capability_tier, Some(3));
    }

    #[test]
    fn from_backend_unhealthy() {
        let backend = create_backend("b2", BackendStatus::Unhealthy, BackendType::OpenAI, 0, 100);
        let profile = AgentSchedulingProfile::from_backend(&backend, PrivacyZone::Open, None);

        assert!(!profile.is_healthy);
        assert_eq!(profile.privacy_zone, PrivacyZone::Open);
        assert_eq!(profile.capability_tier, None);
    }

    #[test]
    fn capability_tier_returns_value_or_default() {
        let backend = create_backend("b1", BackendStatus::Healthy, BackendType::Ollama, 0, 0);

        let with_tier =
            AgentSchedulingProfile::from_backend(&backend, PrivacyZone::Restricted, Some(5));
        assert_eq!(with_tier.capability_tier(), 5);

        let without_tier =
            AgentSchedulingProfile::from_backend(&backend, PrivacyZone::Restricted, None);
        assert_eq!(without_tier.capability_tier(), 1); // Default
    }

    #[test]
    fn error_rate_returns_stub_zero() {
        let backend = create_backend("b1", BackendStatus::Healthy, BackendType::Ollama, 0, 0);
        let profile = AgentSchedulingProfile::from_backend(&backend, PrivacyZone::Restricted, None);
        assert_eq!(profile.error_rate(), 0.0);
    }

    #[test]
    fn success_rate_returns_stub_one() {
        let backend = create_backend("b1", BackendStatus::Healthy, BackendType::Ollama, 0, 0);
        let profile = AgentSchedulingProfile::from_backend(&backend, PrivacyZone::Restricted, None);
        assert_eq!(profile.success_rate(), 1.0);
    }

    #[test]
    fn avg_ttft_approximates_latency() {
        let backend = create_backend("b1", BackendStatus::Healthy, BackendType::Ollama, 0, 42);
        let profile = AgentSchedulingProfile::from_backend(&backend, PrivacyZone::Restricted, None);
        assert_eq!(profile.avg_ttft_ms(), 42.0);
    }

    #[test]
    fn backend_type_formatted_correctly() {
        let backend = create_backend("b1", BackendStatus::Healthy, BackendType::VLLM, 0, 0);
        let profile = AgentSchedulingProfile::from_backend(&backend, PrivacyZone::Restricted, None);
        assert_eq!(profile.backend_type, "VLLM");
    }
}
