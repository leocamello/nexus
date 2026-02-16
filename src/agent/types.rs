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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Model;

    #[test]
    fn model_capability_from_model() {
        let model = Model {
            id: "llama3:8b".to_string(),
            name: "Llama 3 8B".to_string(),
            context_length: 8192,
            supports_vision: true,
            supports_tools: false,
            supports_json_mode: true,
            max_output_tokens: Some(4096),
        };

        let cap: ModelCapability = model.into();
        assert_eq!(cap.id, "llama3:8b");
        assert_eq!(cap.context_length, 8192);
        assert!(cap.supports_vision);
        assert!(!cap.supports_tools);
        assert!(cap.supports_json_mode);
        assert_eq!(cap.max_output_tokens, Some(4096));
        assert!(cap.capability_tier.is_none()); // Phase 1: always None
    }

    #[test]
    fn model_from_model_capability() {
        let cap = ModelCapability {
            id: "gpt-4".to_string(),
            name: "GPT-4".to_string(),
            context_length: 128000,
            supports_vision: true,
            supports_tools: true,
            supports_json_mode: true,
            max_output_tokens: Some(4096),
            capability_tier: Some(3),
        };

        let model: Model = cap.into();
        assert_eq!(model.id, "gpt-4");
        assert_eq!(model.context_length, 128000);
        assert!(model.supports_vision);
        assert!(model.supports_tools);
    }

    #[test]
    fn token_count_exact_value() {
        let tc = TokenCount::Exact(42);
        assert_eq!(tc.value(), 42);
        assert!(tc.is_exact());
    }

    #[test]
    fn token_count_heuristic_value() {
        let tc = TokenCount::Heuristic(100);
        assert_eq!(tc.value(), 100);
        assert!(!tc.is_exact());
    }

    #[test]
    fn health_status_variants() {
        let healthy = HealthStatus::Healthy { model_count: 5 };
        assert!(matches!(healthy, HealthStatus::Healthy { model_count: 5 }));

        let unhealthy = HealthStatus::Unhealthy;
        assert!(matches!(unhealthy, HealthStatus::Unhealthy));

        let loading = HealthStatus::Loading {
            model_id: "llama3".to_string(),
            percent: 50,
            eta_ms: Some(5000),
        };
        assert!(matches!(loading, HealthStatus::Loading { percent: 50, .. }));

        let draining = HealthStatus::Draining;
        assert!(matches!(draining, HealthStatus::Draining));
    }

    #[test]
    fn privacy_zone_equality() {
        assert_eq!(PrivacyZone::Restricted, PrivacyZone::Restricted);
        assert_eq!(PrivacyZone::Open, PrivacyZone::Open);
        assert_ne!(PrivacyZone::Restricted, PrivacyZone::Open);
    }

    #[test]
    fn agent_profile_serialization() {
        let profile = AgentProfile {
            backend_type: "ollama".to_string(),
            version: Some("0.5.0".to_string()),
            privacy_zone: PrivacyZone::Restricted,
            capabilities: AgentCapabilities {
                embeddings: false,
                model_lifecycle: true,
                token_counting: false,
                resource_monitoring: true,
            },
            capability_tier: Some(2),
        };

        let json = serde_json::to_string(&profile).unwrap();
        let deserialized: AgentProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, profile);
    }

    #[test]
    fn resource_usage_default() {
        let usage = ResourceUsage::default();
        assert!(usage.vram_used_bytes.is_none());
        assert!(usage.vram_total_bytes.is_none());
        assert!(usage.pending_requests.is_none());
        assert!(usage.avg_latency_ms.is_none());
        assert!(usage.loaded_models.is_empty());
    }

    #[test]
    fn agent_capabilities_default() {
        let caps = AgentCapabilities::default();
        assert!(!caps.embeddings);
        assert!(!caps.model_lifecycle);
        assert!(!caps.token_counting);
        assert!(!caps.resource_monitoring);
    }

    #[test]
    fn stream_chunk_serialization() {
        let chunk = StreamChunk {
            data: "hello world".to_string(),
        };
        let json = serde_json::to_string(&chunk).unwrap();
        let deserialized: StreamChunk = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.data, "hello world");
    }
}
