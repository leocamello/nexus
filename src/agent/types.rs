//! Supporting types for agent operations.

use crate::registry::Model;
use serde::{Deserialize, Serialize};

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
    
    /// Capability tier for quality-cost tradeoffs (FR-025)
    /// Higher tiers indicate more capable models (e.g., GPT-4 = tier 3, GPT-3.5 = tier 2)
    pub capability_tier: Option<u8>,
}

/// Privacy zone classification for routing decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivacyZone {
    /// Restricted: Must not receive cloud overflow. Local-only backends.
    Restricted,

    /// Open: Can receive cloud overflow from restricted zones (if policy allows).
    Open,
}

/// Capability flags for agent features.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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

impl From<Model> for ModelCapability {
    fn from(model: Model) -> Self {
        Self {
            id: model.id,
            name: model.name,
            context_length: model.context_length,
            supports_vision: model.supports_vision,
            supports_tools: model.supports_tools,
            supports_json_mode: model.supports_json_mode,
            max_output_tokens: model.max_output_tokens,
            capability_tier: None, // Phase 1: Always None
        }
    }
}

impl From<ModelCapability> for Model {
    fn from(cap: ModelCapability) -> Self {
        Self {
            id: cap.id,
            name: cap.name,
            context_length: cap.context_length,
            supports_vision: cap.supports_vision,
            supports_tools: cap.supports_tools,
            supports_json_mode: cap.supports_json_mode,
            max_output_tokens: cap.max_output_tokens,
        }
    }
}

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

    /// List of currently loaded model IDs.
    pub loaded_models: Vec<String>,
}

/// Streaming chunk error wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub data: String,
}
