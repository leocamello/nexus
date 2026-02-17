//! Nexus Inference Interface (NII) - Agent abstraction layer.
//!
//! This module provides the `InferenceAgent` trait and supporting types that abstract
//! backend-specific logic for health checking, model discovery, and inference.

use async_trait::async_trait;
use axum::http::HeaderMap;
use futures_util::stream::BoxStream;

pub mod anthropic;
pub mod error;
pub mod factory;
pub mod generic;
pub mod google;
pub mod lmstudio;
pub mod ollama;
pub mod openai;
pub mod pricing;
pub mod tokenizer;
pub mod translation;
pub mod types;

// Re-export key types for convenience
pub use error::AgentError;
pub use types::{
    AgentCapabilities, AgentProfile, HealthStatus, ModelCapability, PrivacyZone, ResourceUsage,
    StreamChunk, TokenCount,
};

use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};
use std::time::Instant;

/// Quality metrics for an agent tracked over rolling time windows.
///
/// Used by QualityReconciler to filter out unreliable backends and by
/// SchedulerReconciler to penalize slow backends in scoring.
///
/// # Thread Safety
///
/// Wrapped in `Arc<parking_lot::RwLock<AgentQualityMetrics>>` to allow
/// concurrent reads (request path) and periodic writes (quality loop).
///
/// # Rolling Windows
///
/// - error_rate_1h: Computed from requests in the last hour
/// - success_rate_24h: Computed from requests in the last 24 hours
/// - avg_ttft_ms: Average time to first token over the last hour
#[derive(Debug, Clone)]
pub struct AgentQualityMetrics {
    /// Error rate over the last 1 hour (0.0 = no errors, 1.0 = all errors)
    pub error_rate_1h: f32,

    /// Average time to first token in milliseconds (last 1 hour)
    pub avg_ttft_ms: u32,

    /// Success rate over the last 24 hours (0.0 = all failures, 1.0 = all success)
    pub success_rate_24h: f32,

    /// Timestamp of the last request failure (None if no failures yet)
    pub last_failure_ts: Option<Instant>,

    /// Number of requests in the last 1 hour
    pub request_count_1h: u32,
}

impl Default for AgentQualityMetrics {
    /// Default metrics assume a healthy agent with no history.
    fn default() -> Self {
        Self {
            error_rate_1h: 0.0,
            avg_ttft_ms: 0,
            success_rate_24h: 1.0,
            last_failure_ts: None,
            request_count_1h: 0,
        }
    }
}

impl AgentQualityMetrics {
    /// Create new metrics with all-healthy default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if the agent is considered healthy based on current metrics.
    ///
    /// An agent is healthy if:
    /// - Error rate is below 50% (or no requests yet)
    /// - Has processed at least 1 request OR has no failure history
    pub fn is_healthy(&self) -> bool {
        self.error_rate_1h < 0.5 && (self.request_count_1h > 0 || self.last_failure_ts.is_none())
    }
}

/// Unified interface for all LLM inference backends.
///
/// Encapsulates backend-specific HTTP protocols, response parsing, and
/// capability detection. Enables uniform routing without type branching.
///
/// # Object Safety
///
/// This trait is object-safe and designed to be used as `Arc<dyn InferenceAgent>`.
/// All async methods use `async_trait` for compatibility with trait objects.
///
/// # Cancellation Safety
///
/// All async methods are cancellation-safe. Dropping a future will abort any
/// in-flight HTTP requests and clean up resources.
#[async_trait]
pub trait InferenceAgent: Send + Sync + 'static {
    // ========================================================================
    // Identity & Metadata (synchronous)
    // ========================================================================

    /// Unique identifier for this agent instance (e.g., "backend-uuid").
    fn id(&self) -> &str;

    /// Human-readable name for logging and UI (e.g., "Ollama on localhost").
    fn name(&self) -> &str;

    /// Agent profile with type, version, capabilities, and privacy zone.
    fn profile(&self) -> AgentProfile;

    // ========================================================================
    // Discovery & Health (required)
    // ========================================================================

    /// Check backend health and return current status.
    ///
    /// Implementations:
    /// - OllamaAgent: GET /api/tags, count models
    /// - GenericOpenAIAgent: GET /v1/models
    /// - LMStudioAgent: GET /v1/models with LM Studio-specific handling
    ///
    /// # Returns
    ///
    /// - `Ok(HealthStatus::Healthy)` if backend is reachable and functional
    /// - `Ok(HealthStatus::Unhealthy)` if backend returned error
    /// - `Err(AgentError::Network)` if network unreachable
    /// - `Err(AgentError::Timeout)` if request timed out
    async fn health_check(&self) -> Result<HealthStatus, AgentError>;

    /// List all available models with capabilities.
    ///
    /// Implementations:
    /// - OllamaAgent: GET /api/tags, then POST /api/show per model for enrichment
    /// - GenericOpenAIAgent: GET /v1/models, apply name heuristics for capabilities
    ///
    /// # Returns
    ///
    /// - `Ok(Vec<ModelCapability>)` with discovered models
    /// - `Err(AgentError::Network)` if backend unreachable
    async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError>;

    // ========================================================================
    // Inference (required)
    // ========================================================================

    /// Execute non-streaming chat completion request.
    ///
    /// Request must be OpenAI-compatible. Agent handles:
    /// - HTTP request construction (URL, headers, body)
    /// - Authorization header forwarding (if present)
    /// - Response parsing and error mapping
    ///
    /// # Arguments
    ///
    /// * `request` - OpenAI-compatible chat completion request
    /// * `headers` - Optional headers from original request (for Authorization forwarding)
    ///
    /// # Returns
    ///
    /// - `Ok(ChatCompletionResponse)` on success
    /// - `Err(AgentError::Upstream)` if backend returned error (4xx, 5xx)
    /// - `Err(AgentError::Network)` if connection failed
    /// - `Err(AgentError::Timeout)` if request exceeded deadline
    /// - `Err(AgentError::InvalidResponse)` if response doesn't match OpenAI format
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
        headers: Option<&HeaderMap>,
    ) -> Result<ChatCompletionResponse, AgentError>;

    /// Execute streaming chat completion request.
    ///
    /// Returns a stream of SSE chunks in OpenAI format. Stream is cancellation-safe:
    /// dropping the future aborts the in-flight HTTP request and cleans up resources.
    ///
    /// # Arguments
    ///
    /// * `request` - OpenAI-compatible chat completion request with stream=true
    /// * `headers` - Optional headers from original request (for Authorization forwarding)
    ///
    /// # Returns
    ///
    /// - `Ok(BoxStream)` on successful connection
    /// - `Err(AgentError::*)` on connection/auth failures (before streaming starts)
    async fn chat_completion_stream(
        &self,
        request: ChatCompletionRequest,
        headers: Option<&HeaderMap>,
    ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError>;

    // ========================================================================
    // Optional Capabilities (with defaults)
    // ========================================================================

    /// Generate embeddings for input text (F17: Embeddings, v0.4).
    ///
    /// Default implementation returns `Unsupported`. Override in OpenAIAgent
    /// and backends that support /v1/embeddings endpoint.
    async fn embeddings(&self, _input: Vec<String>) -> Result<Vec<Vec<f32>>, AgentError> {
        Err(AgentError::Unsupported("embeddings"))
    }

    /// Load a model into backend memory (F20: Model Lifecycle, v0.5).
    ///
    /// Default implementation returns `Unsupported`. Override in OllamaAgent
    /// (POST /api/pull) and vLLM (if lifecycle API available).
    async fn load_model(&self, _model_id: &str) -> Result<(), AgentError> {
        Err(AgentError::Unsupported("load_model"))
    }

    /// Unload a model from backend memory (F20: Model Lifecycle, v0.5).
    ///
    /// Default implementation returns `Unsupported`. Override in agents that
    /// support explicit model unloading.
    async fn unload_model(&self, _model_id: &str) -> Result<(), AgentError> {
        Err(AgentError::Unsupported("unload_model"))
    }

    /// Count tokens in text using backend-specific tokenizer (F14: Budget, v0.3).
    ///
    /// Default implementation returns heuristic (chars / 4). Override in:
    /// - OpenAIAgent: Use tiktoken-rs with o200k_base encoding
    /// - Anthropic (future): Use Claude tokenizer
    ///
    /// Returns `TokenCount::Exact` if using real tokenizer, `Heuristic` otherwise.
    async fn count_tokens(&self, _model_id: &str, text: &str) -> TokenCount {
        TokenCount::Heuristic((text.len() / 4) as u32)
    }

    /// Query backend resource usage (F19: Fleet Intelligence, v0.5).
    ///
    /// Default implementation returns empty ResourceUsage. Override in:
    /// - OllamaAgent: Parse /api/ps for VRAM usage
    /// - vLLM: Query metrics endpoint
    async fn resource_usage(&self) -> ResourceUsage {
        ResourceUsage::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};
    use futures_util::stream::BoxStream;

    // ========================================================================
    // T004: Unit tests for AgentQualityMetrics
    // ========================================================================

    #[test]
    fn quality_metrics_default_values() {
        let metrics = AgentQualityMetrics::default();
        assert_eq!(metrics.error_rate_1h, 0.0);
        assert_eq!(metrics.avg_ttft_ms, 0);
        assert_eq!(metrics.success_rate_24h, 1.0);
        assert_eq!(metrics.request_count_1h, 0);
        assert!(metrics.last_failure_ts.is_none());
    }

    #[test]
    fn quality_metrics_new_creates_healthy_defaults() {
        let metrics = AgentQualityMetrics::new();
        assert!(metrics.is_healthy());
        assert_eq!(metrics.error_rate_1h, 0.0);
        assert_eq!(metrics.success_rate_24h, 1.0);
    }

    #[test]
    fn quality_metrics_is_healthy_with_no_history() {
        let metrics = AgentQualityMetrics {
            error_rate_1h: 0.0,
            avg_ttft_ms: 0,
            success_rate_24h: 1.0,
            last_failure_ts: None,
            request_count_1h: 0,
        };
        assert!(metrics.is_healthy());
    }

    #[test]
    fn quality_metrics_is_healthy_with_low_error_rate() {
        let metrics = AgentQualityMetrics {
            error_rate_1h: 0.25, // 25% errors - below threshold
            avg_ttft_ms: 100,
            success_rate_24h: 0.75,
            last_failure_ts: Some(Instant::now()),
            request_count_1h: 10,
        };
        assert!(metrics.is_healthy());
    }

    #[test]
    fn quality_metrics_is_unhealthy_with_high_error_rate() {
        let metrics = AgentQualityMetrics {
            error_rate_1h: 0.75, // 75% errors - above threshold
            avg_ttft_ms: 100,
            success_rate_24h: 0.25,
            last_failure_ts: Some(Instant::now()),
            request_count_1h: 10,
        };
        assert!(!metrics.is_healthy());
    }

    #[test]
    fn quality_metrics_threshold_at_boundary() {
        let metrics = AgentQualityMetrics {
            error_rate_1h: 0.5, // Exactly at 50% threshold
            avg_ttft_ms: 100,
            success_rate_24h: 0.5,
            last_failure_ts: Some(Instant::now()),
            request_count_1h: 10,
        };
        // At exactly 0.5, should not be healthy (threshold is <0.5)
        assert!(!metrics.is_healthy());
    }

    // ========================================================================
    // Original InferenceAgent trait tests
    // ========================================================================

    struct MockAgent;

    #[async_trait]
    impl InferenceAgent for MockAgent {
        fn id(&self) -> &str {
            "mock"
        }
        fn name(&self) -> &str {
            "Mock Agent"
        }
        fn profile(&self) -> AgentProfile {
            AgentProfile {
                backend_type: "mock".to_string(),
                version: None,
                privacy_zone: PrivacyZone::Open,
                capabilities: AgentCapabilities::default(),
                capability_tier: None,
            }
        }
        async fn health_check(&self) -> Result<HealthStatus, AgentError> {
            Ok(HealthStatus::Healthy { model_count: 0 })
        }
        async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
            Ok(vec![])
        }
        async fn chat_completion(
            &self,
            _request: ChatCompletionRequest,
            _headers: Option<&HeaderMap>,
        ) -> Result<ChatCompletionResponse, AgentError> {
            Err(AgentError::Unsupported("chat_completion"))
        }
        async fn chat_completion_stream(
            &self,
            _request: ChatCompletionRequest,
            _headers: Option<&HeaderMap>,
        ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
            Err(AgentError::Unsupported("chat_completion_stream"))
        }
    }

    #[tokio::test]
    async fn count_tokens_empty_string() {
        let agent = MockAgent;
        assert_eq!(agent.count_tokens("m", "").await, TokenCount::Heuristic(0));
    }

    #[tokio::test]
    async fn count_tokens_short_string() {
        let agent = MockAgent;
        assert_eq!(
            agent.count_tokens("m", "hello").await,
            TokenCount::Heuristic(1)
        );
    }

    #[tokio::test]
    async fn count_tokens_100_chars() {
        let agent = MockAgent;
        let text = "a".repeat(100);
        assert_eq!(
            agent.count_tokens("m", &text).await,
            TokenCount::Heuristic(25)
        );
    }

    #[tokio::test]
    async fn embeddings_returns_unsupported() {
        let agent = MockAgent;
        let err = agent.embeddings(vec![]).await.unwrap_err();
        assert!(matches!(err, AgentError::Unsupported("embeddings")));
    }

    #[tokio::test]
    async fn load_model_returns_unsupported() {
        let agent = MockAgent;
        let err = agent.load_model("m").await.unwrap_err();
        assert!(matches!(err, AgentError::Unsupported("load_model")));
    }

    #[tokio::test]
    async fn unload_model_returns_unsupported() {
        let agent = MockAgent;
        let err = agent.unload_model("m").await.unwrap_err();
        assert!(matches!(err, AgentError::Unsupported("unload_model")));
    }

    #[tokio::test]
    async fn resource_usage_returns_default() {
        let agent = MockAgent;
        assert_eq!(agent.resource_usage().await, ResourceUsage::default());
    }
}
