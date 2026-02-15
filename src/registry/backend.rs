use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64};

/// Backend type indicating API compatibility.
///
/// Different backend types have different API contracts and capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendType {
    /// Ollama backend (<https://ollama.ai>)
    Ollama,
    /// vLLM backend (<https://vllm.ai>)
    VLLM,
    /// llama.cpp server
    LlamaCpp,
    /// Exo distributed inference
    Exo,
    /// OpenAI-compatible API
    OpenAI,
    /// Anthropic Claude API
    Anthropic,
    /// LM Studio backend (<https://lmstudio.ai>)
    LMStudio,
    /// Generic/unknown backend type
    Generic,
}

/// Backend health status.
///
/// Determines whether the backend should receive new requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendStatus {
    /// Backend is healthy and accepting requests
    Healthy,
    /// Backend is unhealthy (failed health check)
    Unhealthy,
    /// Health status is unknown (not yet checked)
    Unknown,
    /// Backend is healthy but not accepting new requests (draining)
    Draining,
}

/// How the backend was discovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiscoverySource {
    /// Configured statically in config file
    Static,
    /// Auto-discovered via mDNS
    MDNS,
    /// Added manually via CLI at runtime
    Manual,
}

/// An LLM model available on a backend.
///
/// Models define the capabilities and constraints of a particular LLM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Model {
    /// Unique model identifier (e.g., "llama3:70b")
    pub id: String,
    /// Human-readable model name
    pub name: String,
    /// Maximum context window size in tokens
    pub context_length: u32,
    /// Whether the model supports vision/image inputs
    pub supports_vision: bool,
    /// Whether the model supports function/tool calling
    pub supports_tools: bool,
    /// Whether the model supports JSON mode
    pub supports_json_mode: bool,
    /// Maximum output tokens (if limited)
    pub max_output_tokens: Option<u32>,
}

/// An LLM inference backend.
///
/// Represents a server that can process LLM requests. Contains both configuration
/// and runtime state (via atomic counters for thread-safe updates).
///
/// # Examples
///
/// ```
/// use nexus::registry::{Backend, BackendType, DiscoverySource};
/// use std::collections::HashMap;
///
/// let backend = Backend::new(
///     "backend-1".to_string(),
///     "My Backend".to_string(),
///     "http://localhost:11434".to_string(),
///     BackendType::Ollama,
///     vec![],
///     DiscoverySource::Static,
///     HashMap::new(),
/// );
/// assert_eq!(backend.id, "backend-1");
/// ```
#[derive(Debug)]
pub struct Backend {
    /// Unique identifier (typically a UUID)
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Base URL for API requests
    pub url: String,
    /// Backend type/API compatibility
    pub backend_type: BackendType,
    /// Current health status
    pub status: BackendStatus,
    /// Last time health was checked
    pub last_health_check: DateTime<Utc>,
    /// Last error message (if any)
    pub last_error: Option<String>,
    /// Available models on this backend
    pub models: Vec<Model>,
    /// Priority for routing (lower = prefer)
    pub priority: i32,
    /// Current in-flight requests (atomic)
    pub pending_requests: AtomicU32,
    /// Lifetime total requests served (atomic)
    pub total_requests: AtomicU64,
    /// Rolling average latency in milliseconds (atomic, EMA with α=0.2)
    pub avg_latency_ms: AtomicU32,
    /// How this backend was discovered
    pub discovery_source: DiscoverySource,
    /// Additional metadata key-value pairs
    pub metadata: HashMap<String, String>,
}

impl Backend {
    /// Create a new Backend with default atomic values.
    ///
    /// All atomic counters are initialized to 0, status is set to `Unknown`.
    pub fn new(
        id: String,
        name: String,
        url: String,
        backend_type: BackendType,
        models: Vec<Model>,
        discovery_source: DiscoverySource,
        metadata: HashMap<String, String>,
    ) -> Self {
        Self {
            id,
            name,
            url,
            backend_type,
            status: BackendStatus::Unknown,
            last_health_check: Utc::now(),
            last_error: None,
            models,
            priority: 0,
            pending_requests: AtomicU32::new(0),
            total_requests: AtomicU64::new(0),
            avg_latency_ms: AtomicU32::new(0),
            discovery_source,
            metadata,
        }
    }
}

/// Serializable view of Backend (atomic fields converted to regular values).
///
/// Use this for JSON serialization since atomic types cannot be serialized directly.
/// Convert a `Backend` to a `BackendView` using `Into`/`From`.
///
/// # Examples
///
/// ```
/// use nexus::registry::{Backend, BackendView, BackendType, DiscoverySource};
/// use std::collections::HashMap;
///
/// let backend = Backend::new(
///     "backend-1".to_string(),
///     "My Backend".to_string(),
///     "http://localhost:11434".to_string(),
///     BackendType::Ollama,
///     vec![],
///     DiscoverySource::Static,
///     HashMap::new(),
/// );
///
/// let view: BackendView = (&backend).into();
/// let json = serde_json::to_string(&view).unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub backend_type: BackendType,
    pub status: BackendStatus,
    pub last_health_check: DateTime<Utc>,
    pub last_error: Option<String>,
    pub models: Vec<Model>,
    pub priority: i32,
    pub pending_requests: u32,
    pub total_requests: u64,
    pub avg_latency_ms: u32,
    pub discovery_source: DiscoverySource,
    pub metadata: HashMap<String, String>,
}

impl From<&Backend> for BackendView {
    fn from(backend: &Backend) -> Self {
        Self {
            id: backend.id.clone(),
            name: backend.name.clone(),
            url: backend.url.clone(),
            backend_type: backend.backend_type,
            status: backend.status,
            last_health_check: backend.last_health_check,
            last_error: backend.last_error.clone(),
            models: backend.models.clone(),
            priority: backend.priority,
            pending_requests: backend
                .pending_requests
                .load(std::sync::atomic::Ordering::SeqCst),
            total_requests: backend
                .total_requests
                .load(std::sync::atomic::Ordering::SeqCst),
            avg_latency_ms: backend
                .avg_latency_ms
                .load(std::sync::atomic::Ordering::SeqCst),
            discovery_source: backend.discovery_source,
            metadata: backend.metadata.clone(),
        }
    }
}
