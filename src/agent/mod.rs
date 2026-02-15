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
pub mod lmstudio;
pub mod ollama;
pub mod openai;
pub mod types;

// Re-export key types for convenience
pub use error::AgentError;
pub use types::{
    AgentCapabilities, AgentProfile, HealthStatus, ModelCapability, PrivacyZone, ResourceUsage,
    StreamChunk, TokenCount,
};

use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};

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
