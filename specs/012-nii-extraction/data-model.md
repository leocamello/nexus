# Data Model: NII Extraction

**Feature**: F12 NII Extraction (RFC-001 Phase 1)  
**Date**: 2026-02-15  
**Status**: Complete

This document defines the core data structures for the Nexus Inference Interface abstraction layer.

---

## Core Trait: InferenceAgent

The `InferenceAgent` trait defines the contract between Nexus core and any LLM backend. All backend-specific logic is encapsulated behind this interface.

```rust
use async_trait::async_trait;
use futures_util::stream::BoxStream;
use axum::http::HeaderMap;

/// Unified interface for all LLM inference backends.
///
/// Encapsulates backend-specific HTTP protocols, response parsing, and
/// capability detection. Enables uniform routing without type branching.
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
    /// Returns:
    /// - Ok(HealthStatus::Healthy) if backend is reachable and functional
    /// - Ok(HealthStatus::Unhealthy) if backend returned error
    /// - Err(AgentError::Network) if network unreachable
    /// - Err(AgentError::Timeout) if request timed out
    async fn health_check(&self) -> Result<HealthStatus, AgentError>;
    
    /// List all available models with capabilities.
    ///
    /// Implementations:
    /// - OllamaAgent: GET /api/tags, then POST /api/show per model for enrichment
    /// - GenericOpenAIAgent: GET /v1/models, apply name heuristics for capabilities
    ///
    /// Returns:
    /// - Ok(Vec<ModelCapability>) with discovered models
    /// - Err(AgentError::Network) if backend unreachable
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
    /// Returns:
    /// - Ok(ChatCompletionResponse) on success
    /// - Err(AgentError::Upstream) if backend returned error (4xx, 5xx)
    /// - Err(AgentError::Network) if connection failed
    /// - Err(AgentError::Timeout) if request exceeded deadline
    /// - Err(AgentError::InvalidResponse) if response doesn't match OpenAI format
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
    /// Returns:
    /// - Ok(BoxStream) on successful connection
    /// - Err(AgentError::*) on connection/auth failures (before streaming starts)
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
    async fn embeddings(
        &self,
        _request: EmbeddingsRequest,
    ) -> Result<EmbeddingsResponse, AgentError> {
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
    /// Returns TokenCount::Exact if using real tokenizer, Heuristic otherwise.
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
```

---

## Supporting Types

### AgentProfile

Agent metadata for routing and observability.

```rust
/// Metadata describing an agent's type, version, and capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProfile {
    /// Backend type string (e.g., "ollama", "openai", "generic").
    pub backend_type: String,
    
    /// Optional version string from backend (e.g., "0.1.29" for Ollama).
    pub version: Option<String>,
    
    /// Privacy zone classification.
    pub privacy_zone: PrivacyZone,
    
    /// Capability flags for this agent type.
    pub capabilities: AgentCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivacyZone {
    /// Restricted: Must not receive cloud overflow. Local-only backends.
    Restricted,
    
    /// Open: Can receive cloud overflow from restricted zones (if policy allows).
    Open,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCapabilities {
    /// Supports /v1/embeddings endpoint.
    pub embeddings: bool,
    
    /// Supports model load/unload lifecycle operations.
    pub model_lifecycle: bool,
    
    /// Supports token counting with backend-specific tokenizer.
    pub token_counting: bool,
    
    /// Supports resource usage queries (VRAM, pending requests).
    pub resource_monitoring: bool,
}
```

**Mapping to BackendType**:
- `Ollama` → `privacy_zone: Restricted`, `model_lifecycle: false` (Phase 1)
- `OpenAI` → `privacy_zone: Open`, `token_counting: true`
- `LMStudio`, `VLLM`, `Generic` → `privacy_zone: Restricted`

---

### HealthStatus

Agent health with structured state.

```rust
/// Backend health status with state-specific metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Backend is healthy and accepting requests.
    Healthy {
        /// Number of models discovered (informational).
        model_count: usize,
    },
    
    /// Backend is unhealthy (failed health check).
    Unhealthy,
    
    /// Backend is loading a model (F20: Model Lifecycle).
    Loading {
        /// Model currently being loaded.
        model_id: String,
        
        /// Load progress percentage (0-100).
        percent: u8,
        
        /// Estimated time to completion in milliseconds (optional).
        eta_ms: Option<u64>,
    },
    
    /// Backend is healthy but draining (rejecting new requests).
    Draining,
}
```

**Phase 1 usage**: Only `Healthy` and `Unhealthy` are used. `Loading` and `Draining` are reserved for future features.

---

### ModelCapability

Extends existing `Model` struct with capability tier.

```rust
/// Model with capabilities for routing decisions.
///
/// Phase 1: Reuses existing `Model` struct from registry/backend.rs
/// Phase 2: Adds `capability_tier` field for F13 tier routing
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCapability {
    /// Unique model identifier (e.g., "llama3:70b").
    pub id: String,
    
    /// Human-readable model name.
    pub name: String,
    
    /// Maximum context window size in tokens.
    pub context_length: u32,
    
    /// Supports vision/image inputs.
    pub supports_vision: bool,
    
    /// Supports function/tool calling.
    pub supports_tools: bool,
    
    /// Supports JSON mode.
    pub supports_json_mode: bool,
    
    /// Maximum output tokens (if limited).
    pub max_output_tokens: Option<u32>,
    
    /// Capability tier for tiered routing (F13, v0.3).
    /// Phase 1: Always None. Phase 2: Populated based on model name/metadata.
    pub capability_tier: Option<u8>,
}
```

**Conversion**: `Model` → `ModelCapability` is 1:1 in Phase 1 (`capability_tier: None`).

---

### AgentError

Error type for all agent operations.

```rust
use thiserror::Error;

/// Errors that can occur during agent operations.
#[derive(Error, Debug)]
pub enum AgentError {
    /// Network connectivity error (DNS, connection refused, etc.).
    #[error("Network error: {0}")]
    Network(String),
    
    /// Request exceeded deadline.
    #[error("Request timeout after {0}ms")]
    Timeout(u64),
    
    /// Backend returned an error response (4xx, 5xx).
    #[error("Backend error {status}: {message}")]
    Upstream {
        status: u16,
        message: String,
    },
    
    /// Method not supported by this agent implementation.
    #[error("Method '{0}' not supported by this agent")]
    Unsupported(&'static str),
    
    /// Backend response doesn't match expected format.
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    
    /// Agent configuration error.
    #[error("Configuration error: {0}")]
    Configuration(String),
}

impl From<AgentError> for ApiError {
    fn from(e: AgentError) -> Self {
        match e {
            AgentError::Network(msg) => ApiError::bad_gateway(&msg),
            AgentError::Timeout(_) => ApiError::gateway_timeout(),
            AgentError::Upstream { message, .. } => ApiError::bad_gateway(&message),
            AgentError::Unsupported(op) => {
                ApiError::not_implemented(&format!("{} not supported", op))
            }
            AgentError::InvalidResponse(msg) => ApiError::bad_gateway(&msg),
            AgentError::Configuration(msg) => ApiError::internal(&msg),
        }
    }
}
```

---

### TokenCount

Tiered token count result.

```rust
/// Token count with accuracy indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenCount {
    /// Exact count from backend-specific tokenizer.
    Exact(u32),
    
    /// Heuristic estimate (chars / 4).
    Heuristic(u32),
}

impl TokenCount {
    pub fn value(&self) -> u32 {
        match self {
            TokenCount::Exact(n) => *n,
            TokenCount::Heuristic(n) => *n,
        }
    }
    
    pub fn is_exact(&self) -> bool {
        matches!(self, TokenCount::Exact(_))
    }
}
```

---

### ResourceUsage

Backend resource metrics.

```rust
/// Backend resource usage for fleet intelligence (F19, v0.5).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// VRAM usage in bytes (GPU memory).
    pub vram_used_bytes: Option<u64>,
    
    /// VRAM total capacity in bytes.
    pub vram_total_bytes: Option<u64>,
    
    /// Number of pending inference requests.
    pub pending_requests: Option<u32>,
    
    /// Average request latency in milliseconds.
    pub avg_latency_ms: Option<u32>,
    
    /// Currently loaded models.
    pub loaded_models: Vec<String>,
}
```

**Phase 1**: All fields return `None` or empty vec. Phase 2+: Populated by agent implementations.

---

## Agent Implementations

### OllamaAgent

Handles Ollama-specific API (`/api/tags`, `/api/show`, `/api/generate`).

```rust
pub struct OllamaAgent {
    /// Unique agent ID (matches Backend.id).
    id: String,
    
    /// Human-readable name.
    name: String,
    
    /// Ollama base URL (e.g., "http://localhost:11434").
    base_url: String,
    
    /// Shared HTTP client with connection pooling.
    client: Arc<reqwest::Client>,
}

impl OllamaAgent {
    pub fn new(
        id: String,
        name: String,
        base_url: String,
        client: Arc<reqwest::Client>,
    ) -> Self {
        Self { id, name, base_url, client }
    }
}

#[async_trait]
impl InferenceAgent for OllamaAgent {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    
    fn profile(&self) -> AgentProfile {
        AgentProfile {
            backend_type: "ollama".to_string(),
            version: None,  // Could query /api/version
            privacy_zone: PrivacyZone::Restricted,
            capabilities: AgentCapabilities {
                embeddings: false,
                model_lifecycle: false,  // Future: /api/pull, /api/delete
                token_counting: false,
                resource_monitoring: false,  // Future: /api/ps
            },
        }
    }
    
    async fn health_check(&self) -> Result<HealthStatus, AgentError> {
        // GET /api/tags, count models
    }
    
    async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
        // GET /api/tags, then POST /api/show per model
    }
    
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
        headers: Option<&HeaderMap>,
    ) -> Result<ChatCompletionResponse, AgentError> {
        // POST /v1/chat/completions (Ollama supports OpenAI API)
    }
    
    async fn chat_completion_stream(
        &self,
        request: ChatCompletionRequest,
        headers: Option<&HeaderMap>,
    ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
        // POST /v1/chat/completions with stream=true
    }
}
```

---

### OpenAIAgent

Handles cloud OpenAI API with API key authentication.

```rust
pub struct OpenAIAgent {
    id: String,
    name: String,
    base_url: String,  // "https://api.openai.com"
    api_key: Option<String>,  // From config metadata
    client: Arc<reqwest::Client>,
}

#[async_trait]
impl InferenceAgent for OpenAIAgent {
    fn profile(&self) -> AgentProfile {
        AgentProfile {
            backend_type: "openai".to_string(),
            version: None,
            privacy_zone: PrivacyZone::Open,  // Can receive cloud overflow
            capabilities: AgentCapabilities {
                embeddings: true,  // /v1/embeddings
                model_lifecycle: false,
                token_counting: true,  // tiktoken-rs
                resource_monitoring: false,
            },
        }
    }
    
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
        headers: Option<&HeaderMap>,
    ) -> Result<ChatCompletionResponse, AgentError> {
        let mut req_builder = self.client
            .post(&format!("{}/v1/chat/completions", self.base_url))
            .json(&request);
        
        // Add API key from config or forward from headers
        if let Some(key) = &self.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        } else if let Some(hdrs) = headers {
            if let Some(auth) = hdrs.get("authorization") {
                req_builder = req_builder.header("Authorization", auth);
            }
        }
        
        // ... send, parse response
    }
    
    async fn count_tokens(&self, model_id: &str, text: &str) -> TokenCount {
        // Use tiktoken-rs (deferred to F14: Budget implementation)
        // For now, return heuristic
        TokenCount::Heuristic((text.len() / 4) as u32)
    }
}
```

---

### LMStudioAgent

Handles LM Studio's OpenAI-compatible API with quirks.

```rust
pub struct LMStudioAgent {
    id: String,
    name: String,
    base_url: String,
    client: Arc<reqwest::Client>,
}

#[async_trait]
impl InferenceAgent for LMStudioAgent {
    fn profile(&self) -> AgentProfile {
        AgentProfile {
            backend_type: "lmstudio".to_string(),
            version: None,
            privacy_zone: PrivacyZone::Restricted,
            capabilities: AgentCapabilities::default(),  // OpenAI-compatible only
        }
    }
    
    // Implementation identical to GenericOpenAIAgent, but profiled as "lmstudio"
}
```

---

### GenericOpenAIAgent

Handles vLLM, exo, llama.cpp, and other OpenAI-compatible backends.

```rust
pub struct GenericOpenAIAgent {
    id: String,
    name: String,
    backend_type: BackendType,  // VLLM, LlamaCpp, Exo, Generic
    base_url: String,
    client: Arc<reqwest::Client>,
}

#[async_trait]
impl InferenceAgent for GenericOpenAIAgent {
    fn profile(&self) -> AgentProfile {
        AgentProfile {
            backend_type: format!("{:?}", self.backend_type).to_lowercase(),
            version: None,
            privacy_zone: PrivacyZone::Restricted,
            capabilities: AgentCapabilities::default(),  // Basic OpenAI API only
        }
    }
    
    async fn health_check(&self) -> Result<HealthStatus, AgentError> {
        // GET /v1/models
    }
    
    async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
        // GET /v1/models, apply name heuristics for capabilities
    }
    
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
        headers: Option<&HeaderMap>,
    ) -> Result<ChatCompletionResponse, AgentError> {
        // POST /v1/chat/completions
    }
    
    async fn chat_completion_stream(
        &self,
        request: ChatCompletionRequest,
        headers: Option<&HeaderMap>,
    ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
        // POST /v1/chat/completions with stream=true
    }
}
```

---

## Agent Factory

Factory function for creating agents from configuration.

```rust
/// Create an InferenceAgent from backend configuration.
///
/// # Arguments
///
/// * `config` - Backend configuration from TOML or mDNS discovery
/// * `client` - Shared HTTP client with connection pooling
///
/// # Returns
///
/// Arc-wrapped trait object for uniform storage in Registry.
///
/// # Errors
///
/// Returns `AgentError::Configuration` if required metadata is missing
/// (e.g., OpenAI agent without API key when required).
pub fn create_agent(
    config: &BackendConfig,
    client: Arc<reqwest::Client>,
) -> Result<Arc<dyn InferenceAgent>, AgentError> {
    match config.backend_type {
        BackendType::Ollama => Ok(Arc::new(OllamaAgent::new(
            config.id.clone(),
            config.name.clone(),
            config.url.clone(),
            client,
        ))),
        
        BackendType::OpenAI => {
            let api_key = config.metadata.get("api_key").cloned();
            // Phase 1: API key is optional (can be forwarded from request headers)
            // Phase 2: Validate required for cloud backends
            Ok(Arc::new(OpenAIAgent::new(
                config.id.clone(),
                config.name.clone(),
                config.url.clone(),
                api_key,
                client,
            )))
        }
        
        BackendType::LMStudio => Ok(Arc::new(LMStudioAgent::new(
            config.id.clone(),
            config.name.clone(),
            config.url.clone(),
            client,
        ))),
        
        BackendType::VLLM
        | BackendType::LlamaCpp
        | BackendType::Exo
        | BackendType::Generic => Ok(Arc::new(GenericOpenAIAgent::new(
            config.id.clone(),
            config.name.clone(),
            config.backend_type,
            config.url.clone(),
            client,
        ))),
    }
}
```

**Placement**: `src/agent/factory.rs`

---

## Registry Extension

Registry stores both `Backend` and `Arc<dyn InferenceAgent>` (dual storage).

```rust
pub struct Registry {
    backends: DashMap<String, Backend>,  // Existing
    agents: DashMap<String, Arc<dyn InferenceAgent>>,  // New
    model_index: DashMap<String, Vec<String>>,  // Existing
}

impl Registry {
    /// Add backend with associated agent.
    pub fn add_backend_with_agent(
        &self,
        backend: Backend,
        agent: Arc<dyn InferenceAgent>,
    ) -> Result<(), RegistryError> {
        let id = backend.id.clone();
        
        if self.backends.contains_key(&id) {
            return Err(RegistryError::DuplicateBackend(id));
        }
        
        // Update model index
        for model in &backend.models {
            self.model_index
                .entry(model.id.clone())
                .or_default()
                .push(id.clone());
        }
        
        // Store both representations
        self.backends.insert(id.clone(), backend);
        self.agents.insert(id, agent);
        
        Ok(())
    }
    
    /// Get agent by backend ID.
    pub fn get_agent(&self, id: &str) -> Option<Arc<dyn InferenceAgent>> {
        self.agents.get(id).map(|a| a.clone())
    }
    
    /// Get all agents (for iteration).
    pub fn get_all_agents(&self) -> Vec<Arc<dyn InferenceAgent>> {
        self.agents.iter().map(|entry| entry.value().clone()).collect()
    }
}
```

---

## Module Structure

```
src/agent/
├── mod.rs              # Trait definition, exports
├── error.rs            # AgentError enum
├── types.rs            # AgentProfile, HealthStatus, TokenCount, ResourceUsage
├── factory.rs          # create_agent() function
├── ollama.rs           # OllamaAgent implementation
├── openai.rs           # OpenAIAgent implementation
├── lmstudio.rs         # LMStudioAgent implementation
├── generic.rs          # GenericOpenAIAgent implementation
└── tests/
    ├── mod.rs
    ├── ollama_tests.rs
    ├── openai_tests.rs
    └── mock.rs         # Mock HTTP backend helpers
```

---

## Summary

| Entity | Purpose | Storage |
|--------|---------|---------|
| `InferenceAgent` | Trait defining backend contract | N/A (trait) |
| `AgentProfile` | Agent metadata for routing | Per-agent instance |
| `HealthStatus` | Backend health state | Returned by `health_check()` |
| `ModelCapability` | Model with capabilities | Returned by `list_models()` |
| `AgentError` | Typed error for agent ops | Returned by all methods |
| `TokenCount` | Token count with accuracy | Returned by `count_tokens()` |
| `ResourceUsage` | VRAM/load metrics | Returned by `resource_usage()` |
| `OllamaAgent` | Ollama implementation | Registry (Arc<dyn>) |
| `OpenAIAgent` | OpenAI cloud implementation | Registry (Arc<dyn>) |
| `LMStudioAgent` | LM Studio implementation | Registry (Arc<dyn>) |
| `GenericOpenAIAgent` | vLLM/exo/llama.cpp impl | Registry (Arc<dyn>) |
| Agent factory | `create_agent()` function | N/A (factory) |

**Next step**: Generate contracts for agent method signatures and error handling.
