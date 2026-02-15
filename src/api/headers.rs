//! X-Nexus-* response headers for routing transparency.
//!
//! This module implements the Nexus-Transparent Protocol: response headers
//! that reveal routing decisions without modifying the OpenAI-compatible
//! JSON response body.

use axum::http::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};

use crate::agent::types::PrivacyZone;
use crate::registry::BackendType;

/// Route reason for routing transparency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouteReason {
    /// Backend matched requested model capability
    CapabilityMatch,
    /// Local backend at capacity, routed to cloud
    CapacityOverflow,
    /// Privacy requirements restricted routing options
    PrivacyRequirement,
    /// Primary backend failed, routed to fallback
    BackendFailover,
}

impl RouteReason {
    /// Convert to header value string
    pub fn as_str(&self) -> &str {
        match self {
            RouteReason::CapabilityMatch => "capability-match",
            RouteReason::CapacityOverflow => "capacity-overflow",
            RouteReason::PrivacyRequirement => "privacy-requirement",
            RouteReason::BackendFailover => "backend-failover",
        }
    }
}

/// Backend type for X-Nexus-Backend-Type header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendTypeHeader {
    Local,
    Cloud,
}

impl BackendTypeHeader {
    pub fn as_str(&self) -> &str {
        match self {
            BackendTypeHeader::Local => "local",
            BackendTypeHeader::Cloud => "cloud",
        }
    }

    /// Determine if backend type is cloud-based
    pub fn from_backend_type(backend_type: BackendType) -> Self {
        match backend_type {
            BackendType::OpenAI | BackendType::Anthropic => BackendTypeHeader::Cloud,
            _ => BackendTypeHeader::Local,
        }
    }
}

/// X-Nexus-* response headers for routing transparency.
#[derive(Debug, Clone)]
pub struct NexusHeaders {
    /// Name of backend that served the request
    pub backend: String,
    /// Backend type (local|cloud)
    pub backend_type: BackendTypeHeader,
    /// Reason for routing decision
    pub route_reason: RouteReason,
    /// Privacy zone of selected backend
    pub privacy_zone: PrivacyZone,
    /// Estimated cost for cloud backends (USD)
    pub cost_estimated: Option<f32>,
}

impl NexusHeaders {
    /// Create new Nexus headers
    pub fn new(
        backend: String,
        backend_type: BackendTypeHeader,
        route_reason: RouteReason,
        privacy_zone: PrivacyZone,
        cost_estimated: Option<f32>,
    ) -> Self {
        Self {
            backend,
            backend_type,
            route_reason,
            privacy_zone,
            cost_estimated,
        }
    }

    /// Inject headers into Axum HeaderMap
    pub fn inject_into(&self, headers: &mut HeaderMap) {
        // X-Nexus-Backend
        if let Ok(value) = HeaderValue::from_str(&self.backend) {
            headers.insert(HeaderName::from_static("x-nexus-backend"), value);
        }

        // X-Nexus-Backend-Type
        if let Ok(value) = HeaderValue::from_str(self.backend_type.as_str()) {
            headers.insert(HeaderName::from_static("x-nexus-backend-type"), value);
        }

        // X-Nexus-Route-Reason
        if let Ok(value) = HeaderValue::from_str(self.route_reason.as_str()) {
            headers.insert(HeaderName::from_static("x-nexus-route-reason"), value);
        }

        // X-Nexus-Privacy-Zone
        let zone_str = match self.privacy_zone {
            PrivacyZone::Restricted => "restricted",
            PrivacyZone::Open => "open",
        };
        if let Ok(value) = HeaderValue::from_str(zone_str) {
            headers.insert(HeaderName::from_static("x-nexus-privacy-zone"), value);
        }

        // X-Nexus-Cost-Estimated (only for cloud backends with cost data)
        if let (BackendTypeHeader::Cloud, Some(cost)) = (self.backend_type, self.cost_estimated) {
            if let Ok(value) = HeaderValue::from_str(&format!("{:.6}", cost)) {
                headers.insert(HeaderName::from_static("x-nexus-cost-estimated"), value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_reason_as_str() {
        assert_eq!(RouteReason::CapabilityMatch.as_str(), "capability-match");
        assert_eq!(RouteReason::CapacityOverflow.as_str(), "capacity-overflow");
        assert_eq!(
            RouteReason::PrivacyRequirement.as_str(),
            "privacy-requirement"
        );
        assert_eq!(RouteReason::BackendFailover.as_str(), "backend-failover");
    }

    #[test]
    fn test_backend_type_header() {
        assert_eq!(BackendTypeHeader::Local.as_str(), "local");
        assert_eq!(BackendTypeHeader::Cloud.as_str(), "cloud");
    }

    #[test]
    fn test_backend_type_from_backend_type() {
        assert_eq!(
            BackendTypeHeader::from_backend_type(BackendType::Ollama),
            BackendTypeHeader::Local
        );
        assert_eq!(
            BackendTypeHeader::from_backend_type(BackendType::OpenAI),
            BackendTypeHeader::Cloud
        );
        assert_eq!(
            BackendTypeHeader::from_backend_type(BackendType::Anthropic),
            BackendTypeHeader::Cloud
        );
        assert_eq!(
            BackendTypeHeader::from_backend_type(BackendType::VLLM),
            BackendTypeHeader::Local
        );
    }

    #[test]
    fn test_nexus_headers_inject_local() {
        let headers_struct = NexusHeaders::new(
            "local-ollama".to_string(),
            BackendTypeHeader::Local,
            RouteReason::CapabilityMatch,
            PrivacyZone::Restricted,
            None,
        );

        let mut headers = HeaderMap::new();
        headers_struct.inject_into(&mut headers);

        assert_eq!(headers.get("x-nexus-backend").unwrap(), "local-ollama");
        assert_eq!(headers.get("x-nexus-backend-type").unwrap(), "local");
        assert_eq!(
            headers.get("x-nexus-route-reason").unwrap(),
            "capability-match"
        );
        assert_eq!(headers.get("x-nexus-privacy-zone").unwrap(), "restricted");
        assert!(headers.get("x-nexus-cost-estimated").is_none()); // No cost for local
    }

    #[test]
    fn test_nexus_headers_inject_cloud_with_cost() {
        let headers_struct = NexusHeaders::new(
            "openai-gpt4".to_string(),
            BackendTypeHeader::Cloud,
            RouteReason::CapacityOverflow,
            PrivacyZone::Open,
            Some(0.0015),
        );

        let mut headers = HeaderMap::new();
        headers_struct.inject_into(&mut headers);

        assert_eq!(headers.get("x-nexus-backend").unwrap(), "openai-gpt4");
        assert_eq!(headers.get("x-nexus-backend-type").unwrap(), "cloud");
        assert_eq!(
            headers.get("x-nexus-route-reason").unwrap(),
            "capacity-overflow"
        );
        assert_eq!(headers.get("x-nexus-privacy-zone").unwrap(), "open");
        assert_eq!(headers.get("x-nexus-cost-estimated").unwrap(), "0.001500");
    }

    #[test]
    fn test_nexus_headers_inject_cloud_no_cost() {
        let headers_struct = NexusHeaders::new(
            "openai-gpt4".to_string(),
            BackendTypeHeader::Cloud,
            RouteReason::BackendFailover,
            PrivacyZone::Open,
            None,
        );

        let mut headers = HeaderMap::new();
        headers_struct.inject_into(&mut headers);

        assert_eq!(headers.get("x-nexus-backend").unwrap(), "openai-gpt4");
        assert_eq!(headers.get("x-nexus-backend-type").unwrap(), "cloud");
        // Cost header not injected when cost is None
        assert!(headers.get("x-nexus-cost-estimated").is_none());
    }
}
