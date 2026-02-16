//! Enhanced error types for Nexus API (F12: Cloud Backend Support)
//!
//! This module extends the API error handling with actionable error contexts
//! that help clients make intelligent retry decisions.

use super::types::ApiErrorBody;
use serde::{Deserialize, Serialize};

/// Actionable context for 503 Service Unavailable errors (T018).
///
/// Provides structured information to help clients understand why a request
/// failed and how they might retry successfully.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionableErrorContext {
    /// Tier required for the requested model (1-5)
    /// Present if failure was due to tier requirements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_tier: Option<u8>,

    /// Names of backends currently available (may be empty)
    /// Helps clients understand fleet capacity
    pub available_backends: Vec<String>,

    /// Estimated time to recovery in seconds
    /// Present if backend is expected to recover
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_seconds: Option<u64>,

    /// Privacy zone required but not met
    /// Present if failure was due to privacy constraints
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacy_zone_required: Option<String>,
}

/// 503 Service Unavailable error with actionable context (T019).
///
/// Wraps a standard OpenAI error with additional Nexus-specific context
/// to enable intelligent retry behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceUnavailableError {
    /// Standard OpenAI error envelope
    pub error: ApiErrorBody,

    /// Nexus-specific actionable context
    pub context: ActionableErrorContext,
}

impl ServiceUnavailableError {
    /// Create a new service unavailable error with context (T020).
    ///
    /// # Arguments
    ///
    /// * `message` - Human-readable error message
    /// * `context` - Actionable context for retry decisions
    pub fn new(message: String, context: ActionableErrorContext) -> Self {
        Self {
            error: ApiErrorBody {
                message,
                r#type: "service_unavailable".to_string(),
                param: None,
                code: Some("service_unavailable".to_string()),
            },
            context,
        }
    }

    /// Create error for missing required tier.
    pub fn tier_unavailable(required_tier: u8, available_backends: Vec<String>) -> Self {
        Self::new(
            format!(
                "No backend available for requested model (tier {} required)",
                required_tier
            ),
            ActionableErrorContext {
                required_tier: Some(required_tier),
                available_backends,
                eta_seconds: None,
                privacy_zone_required: None,
            },
        )
    }

    /// Create error for privacy zone mismatch.
    pub fn privacy_unavailable(zone: &str, available_backends: Vec<String>) -> Self {
        Self::new(
            format!(
                "No backend available that satisfies privacy zone requirement: {}",
                zone
            ),
            ActionableErrorContext {
                required_tier: None,
                available_backends,
                eta_seconds: None,
                privacy_zone_required: Some(zone.to_string()),
            },
        )
    }

    /// Create error for all backends down.
    pub fn all_backends_down() -> Self {
        Self::new(
            "All backends are currently unavailable".to_string(),
            ActionableErrorContext {
                required_tier: None,
                available_backends: Vec::new(),
                eta_seconds: None,
                privacy_zone_required: None,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_unavailable_error() {
        let error = ServiceUnavailableError::tier_unavailable(
            5,
            vec!["ollama-llama2".to_string(), "local-mistral".to_string()],
        );

        assert_eq!(error.error.r#type, "service_unavailable");
        assert!(error.error.message.contains("tier 5"));
        assert_eq!(error.context.required_tier, Some(5));
        assert_eq!(error.context.available_backends.len(), 2);
    }

    #[test]
    fn test_privacy_unavailable_error() {
        let error =
            ServiceUnavailableError::privacy_unavailable("restricted", vec!["openai-gpt4".to_string()]);

        assert!(error.error.message.contains("privacy zone"));
        assert_eq!(error.context.privacy_zone_required.as_deref(), Some("restricted"));
    }

    #[test]
    fn test_all_backends_down() {
        let error = ServiceUnavailableError::all_backends_down();

        assert!(error.error.message.contains("unavailable"));
        assert!(error.context.available_backends.is_empty());
    }

    #[test]
    fn test_serialization() {
        let error = ServiceUnavailableError::tier_unavailable(3, vec!["backend1".to_string()]);

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("service_unavailable"));
        assert!(json.contains("required_tier"));
        assert!(json.contains("available_backends"));
    }
}
