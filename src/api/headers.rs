//! Nexus Transparent Protocol - X-Nexus-* response headers.
//!
//! This module implements the Nexus-Transparent Protocol (F12) which exposes
//! routing decisions, backend classification, privacy zones, and cost estimates
//! through standardized HTTP response headers WITHOUT modifying the OpenAI-compatible
//! JSON response body.
//!
//! ## Header Format
//!
//! All responses from Nexus include these headers:
//! - `X-Nexus-Backend`: Backend name (e.g., "openai-gpt4", "ollama-llama2")
//! - `X-Nexus-Backend-Type`: "local" or "cloud"
//! - `X-Nexus-Route-Reason`: Why this backend was selected
//! - `X-Nexus-Privacy-Zone`: "restricted" or "open"
//! - `X-Nexus-Cost-Estimated`: USD cost (optional, cloud only)
//!
//! ## Example
//!
//! ```http
//! HTTP/1.1 200 OK
//! X-Nexus-Backend: openai-gpt4
//! X-Nexus-Backend-Type: cloud
//! X-Nexus-Route-Reason: capability-match
//! X-Nexus-Privacy-Zone: open
//! X-Nexus-Cost-Estimated: 0.0042
//! Content-Type: application/json
//!
//! {"id":"chatcmpl-123","object":"chat.completion",...}
//! ```

use crate::agent::types::PrivacyZone;
use crate::registry::BackendType;
use axum::http::{HeaderName, HeaderValue, Response};
use serde::{Deserialize, Serialize};

/// Standard X-Nexus-* header names (lowercase for HTTP/2 compatibility).
pub const HEADER_BACKEND: &str = "x-nexus-backend";
pub const HEADER_BACKEND_TYPE: &str = "x-nexus-backend-type";
pub const HEADER_ROUTE_REASON: &str = "x-nexus-route-reason";
pub const HEADER_PRIVACY_ZONE: &str = "x-nexus-privacy-zone";
pub const HEADER_COST_ESTIMATED: &str = "x-nexus-cost-estimated";

/// Reason why a particular backend was selected during routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouteReason {
    /// Backend has required model/capabilities.
    CapabilityMatch,

    /// Primary backend at capacity, overflowed to this one.
    CapacityOverflow,

    /// Privacy zone filtering eliminated other candidates.
    PrivacyRequirement,

    /// Previous backend failed, failed over to this one.
    Failover,
}

impl RouteReason {
    /// Convert to HTTP header value string.
    pub fn as_str(&self) -> &'static str {
        match self {
            RouteReason::CapabilityMatch => "capability-match",
            RouteReason::CapacityOverflow => "capacity-overflow",
            RouteReason::PrivacyRequirement => "privacy-requirement",
            RouteReason::Failover => "failover",
        }
    }
}

impl std::fmt::Display for RouteReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Complete set of Nexus-Transparent Protocol headers.
#[derive(Debug, Clone)]
pub struct NexusTransparentHeaders {
    /// Backend name (e.g., "openai-gpt4").
    pub backend: String,

    /// Backend type classification.
    pub backend_type: BackendType,

    /// Routing decision reason.
    pub route_reason: RouteReason,

    /// Privacy zone classification.
    pub privacy_zone: PrivacyZone,

    /// Estimated cost in USD (optional, cloud backends only).
    pub cost_estimated: Option<f64>,
}

impl NexusTransparentHeaders {
    /// Create a new header set.
    pub fn new(
        backend: String,
        backend_type: BackendType,
        route_reason: RouteReason,
        privacy_zone: PrivacyZone,
        cost_estimated: Option<f64>,
    ) -> Self {
        Self {
            backend,
            backend_type,
            route_reason,
            privacy_zone,
            cost_estimated,
        }
    }

    /// Inject all X-Nexus-* headers into an HTTP response.
    ///
    /// This method is the single point of header injection for all responses
    /// (streaming and non-streaming). It ensures 100% consistency across all
    /// code paths (SC-002).
    pub fn inject_into_response<B>(&self, response: &mut Response<B>) {
        let headers = response.headers_mut();

        // X-Nexus-Backend: backend name
        headers.insert(
            HeaderName::from_static(HEADER_BACKEND),
            HeaderValue::from_str(&self.backend).expect("backend name should be valid ASCII"),
        );

        // X-Nexus-Backend-Type: "local" or "cloud"
        let backend_type_str = match self.backend_type {
            BackendType::Ollama
            | BackendType::VLLM
            | BackendType::LlamaCpp
            | BackendType::Exo
            | BackendType::LMStudio
            | BackendType::Generic => "local",
            BackendType::OpenAI | BackendType::Anthropic | BackendType::Google => "cloud",
        };
        headers.insert(
            HeaderName::from_static(HEADER_BACKEND_TYPE),
            HeaderValue::from_static(backend_type_str),
        );

        // X-Nexus-Route-Reason: routing decision
        headers.insert(
            HeaderName::from_static(HEADER_ROUTE_REASON),
            HeaderValue::from_static(self.route_reason.as_str()),
        );

        // X-Nexus-Privacy-Zone: "restricted" or "open"
        let privacy_zone_str = match self.privacy_zone {
            PrivacyZone::Restricted => "restricted",
            PrivacyZone::Open => "open",
        };
        headers.insert(
            HeaderName::from_static(HEADER_PRIVACY_ZONE),
            HeaderValue::from_static(privacy_zone_str),
        );

        // X-Nexus-Cost-Estimated: optional USD cost (4 decimal places)
        if let Some(cost) = self.cost_estimated {
            headers.insert(
                HeaderName::from_static(HEADER_COST_ESTIMATED),
                HeaderValue::from_str(&format!("{:.4}", cost))
                    .expect("cost should format to valid ASCII"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_reason_serialization() {
        assert_eq!(RouteReason::CapabilityMatch.as_str(), "capability-match");
        assert_eq!(RouteReason::CapacityOverflow.as_str(), "capacity-overflow");
        assert_eq!(
            RouteReason::PrivacyRequirement.as_str(),
            "privacy-requirement"
        );
        assert_eq!(RouteReason::Failover.as_str(), "failover");
    }

    #[test]
    fn test_header_injection() {
        use axum::http::Response;

        let headers = NexusTransparentHeaders::new(
            "test-backend".to_string(),
            BackendType::OpenAI,
            RouteReason::CapabilityMatch,
            PrivacyZone::Open,
            Some(0.0042),
        );

        let mut response = Response::new("test body");
        headers.inject_into_response(&mut response);

        let h = response.headers();
        assert_eq!(h.get(HEADER_BACKEND).unwrap(), "test-backend");
        assert_eq!(h.get(HEADER_BACKEND_TYPE).unwrap(), "cloud");
        assert_eq!(h.get(HEADER_ROUTE_REASON).unwrap(), "capability-match");
        assert_eq!(h.get(HEADER_PRIVACY_ZONE).unwrap(), "open");
        assert_eq!(h.get(HEADER_COST_ESTIMATED).unwrap(), "0.0042");
    }

    #[test]
    fn test_inject_headers_no_cost() {
        use axum::http::Response;

        let headers = NexusTransparentHeaders::new(
            "local-backend".to_string(),
            BackendType::Ollama,
            RouteReason::CapabilityMatch,
            PrivacyZone::Restricted,
            None, // No cost
        );

        let mut response = Response::new("test body");
        headers.inject_into_response(&mut response);

        let h = response.headers();
        assert_eq!(h.get(HEADER_BACKEND).unwrap(), "local-backend");
        assert_eq!(h.get(HEADER_BACKEND_TYPE).unwrap(), "local");
        assert_eq!(h.get(HEADER_ROUTE_REASON).unwrap(), "capability-match");
        assert_eq!(h.get(HEADER_PRIVACY_ZONE).unwrap(), "restricted");
        // X-Nexus-Cost-Estimated should NOT be present when cost is None
        assert!(
            h.get(HEADER_COST_ESTIMATED).is_none(),
            "Cost header should not be present when cost_estimated is None"
        );
    }

    #[test]
    fn test_inject_headers_vllm_local() {
        use axum::http::Response;

        let headers = NexusTransparentHeaders::new(
            "vllm-backend".to_string(),
            BackendType::VLLM,
            RouteReason::CapacityOverflow,
            PrivacyZone::Restricted,
            None,
        );

        let mut response = Response::new("test body");
        headers.inject_into_response(&mut response);

        let h = response.headers();
        assert_eq!(h.get(HEADER_BACKEND_TYPE).unwrap(), "local");
        assert_eq!(h.get(HEADER_ROUTE_REASON).unwrap(), "capacity-overflow");
    }

    #[test]
    fn test_inject_headers_llamacpp_local() {
        use axum::http::Response;

        let headers = NexusTransparentHeaders::new(
            "llcpp-backend".to_string(),
            BackendType::LlamaCpp,
            RouteReason::Failover,
            PrivacyZone::Restricted,
            None,
        );

        let mut response = Response::new("test body");
        headers.inject_into_response(&mut response);

        let h = response.headers();
        assert_eq!(h.get(HEADER_BACKEND_TYPE).unwrap(), "local");
        assert_eq!(h.get(HEADER_ROUTE_REASON).unwrap(), "failover");
    }

    #[test]
    fn test_inject_headers_exo_local() {
        use axum::http::Response;

        let headers = NexusTransparentHeaders::new(
            "exo-backend".to_string(),
            BackendType::Exo,
            RouteReason::PrivacyRequirement,
            PrivacyZone::Restricted,
            None,
        );

        let mut response = Response::new("test body");
        headers.inject_into_response(&mut response);

        let h = response.headers();
        assert_eq!(h.get(HEADER_BACKEND_TYPE).unwrap(), "local");
        assert_eq!(h.get(HEADER_ROUTE_REASON).unwrap(), "privacy-requirement");
    }

    #[test]
    fn test_inject_headers_lmstudio_local() {
        use axum::http::Response;

        let headers = NexusTransparentHeaders::new(
            "lms-backend".to_string(),
            BackendType::LMStudio,
            RouteReason::CapabilityMatch,
            PrivacyZone::Open,
            None,
        );

        let mut response = Response::new("test body");
        headers.inject_into_response(&mut response);

        let h = response.headers();
        assert_eq!(h.get(HEADER_BACKEND_TYPE).unwrap(), "local");
        assert_eq!(h.get(HEADER_PRIVACY_ZONE).unwrap(), "open");
    }

    #[test]
    fn test_inject_headers_generic_local() {
        use axum::http::Response;

        let headers = NexusTransparentHeaders::new(
            "gen-backend".to_string(),
            BackendType::Generic,
            RouteReason::CapabilityMatch,
            PrivacyZone::Restricted,
            None,
        );

        let mut response = Response::new("test body");
        headers.inject_into_response(&mut response);

        assert_eq!(
            response.headers().get(HEADER_BACKEND_TYPE).unwrap(),
            "local"
        );
    }

    #[test]
    fn test_inject_headers_anthropic_cloud() {
        use axum::http::Response;

        let headers = NexusTransparentHeaders::new(
            "anthropic-backend".to_string(),
            BackendType::Anthropic,
            RouteReason::CapabilityMatch,
            PrivacyZone::Open,
            Some(0.015),
        );

        let mut response = Response::new("test body");
        headers.inject_into_response(&mut response);

        let h = response.headers();
        assert_eq!(h.get(HEADER_BACKEND_TYPE).unwrap(), "cloud");
        assert_eq!(h.get(HEADER_COST_ESTIMATED).unwrap(), "0.0150");
    }

    #[test]
    fn test_inject_headers_google_cloud() {
        use axum::http::Response;

        let headers = NexusTransparentHeaders::new(
            "google-backend".to_string(),
            BackendType::Google,
            RouteReason::CapabilityMatch,
            PrivacyZone::Open,
            Some(0.001),
        );

        let mut response = Response::new("test body");
        headers.inject_into_response(&mut response);

        let h = response.headers();
        assert_eq!(h.get(HEADER_BACKEND_TYPE).unwrap(), "cloud");
        assert_eq!(h.get(HEADER_COST_ESTIMATED).unwrap(), "0.0010");
    }

    #[test]
    fn test_route_reason_display() {
        assert_eq!(
            format!("{}", RouteReason::CapabilityMatch),
            "capability-match"
        );
        assert_eq!(
            format!("{}", RouteReason::CapacityOverflow),
            "capacity-overflow"
        );
        assert_eq!(
            format!("{}", RouteReason::PrivacyRequirement),
            "privacy-requirement"
        );
        assert_eq!(format!("{}", RouteReason::Failover), "failover");
    }
}
