//! Shared test utilities for Nexus integration and unit tests.
//!
//! Provides reusable helpers for creating backends, models, registries,
//! and mock agents to reduce duplication across test files.

#![allow(dead_code)]

use nexus::api::{create_router, AppState};
use nexus::config::NexusConfig;
use nexus::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model, Registry};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::Arc;

// =============================================================================
// Well-Known Test Constants
// =============================================================================

/// UUID v4 string length: "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
pub const UUID_V4_STRING_LEN: usize = 36;

/// UUID v4 segment count when split by '-'
pub const UUID_V4_SEGMENT_COUNT: usize = 5;

/// UUID v4 segment lengths: [8, 4, 4, 4, 12]
pub const UUID_V4_SEGMENT_LENGTHS: [usize; 5] = [8, 4, 4, 4, 12];

// =============================================================================
// Model Builders
// =============================================================================

/// Create a minimal test model with sensible defaults.
pub fn make_model(id: &str) -> Model {
    Model {
        id: id.to_string(),
        name: id.to_string(),
        context_length: 4096,
        supports_vision: false,
        supports_tools: false,
        supports_json_mode: false,
        max_output_tokens: None,
    }
}

/// Create a test model with vision capability.
pub fn make_vision_model(id: &str, context_length: u32) -> Model {
    Model {
        id: id.to_string(),
        name: id.to_string(),
        context_length,
        supports_vision: true,
        supports_tools: false,
        supports_json_mode: false,
        max_output_tokens: None,
    }
}

/// Create a test model with tool-use capability.
pub fn make_tools_model(id: &str, context_length: u32) -> Model {
    Model {
        id: id.to_string(),
        name: id.to_string(),
        context_length,
        supports_vision: false,
        supports_tools: true,
        supports_json_mode: false,
        max_output_tokens: None,
    }
}

// =============================================================================
// Backend Builders
// =============================================================================

/// Create a healthy backend with one model (most common test pattern).
///
/// Uses `Backend` struct directly to set atomic counters and status,
/// which `Backend::new()` doesn't allow.
pub fn make_backend(id: &str, name: &str, model_id: &str, priority: i32) -> Backend {
    Backend {
        id: id.to_string(),
        name: name.to_string(),
        url: format!("http://{}", name),
        backend_type: BackendType::Ollama,
        status: BackendStatus::Healthy,
        last_health_check: chrono::Utc::now(),
        last_error: None,
        models: vec![make_model(model_id)],
        priority,
        pending_requests: AtomicU32::new(0),
        total_requests: AtomicU64::new(0),
        avg_latency_ms: AtomicU32::new(50),
        discovery_source: DiscoverySource::Static,
        metadata: HashMap::new(),
    }
}

/// Create a backend via `Backend::new()` (starts as Unknown status, no atomics preset).
pub fn make_new_backend(id: &str, url: &str, backend_type: BackendType) -> Backend {
    Backend::new(
        id.to_string(),
        format!("Test {}", id),
        url.to_string(),
        backend_type,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    )
}

// =============================================================================
// Registry Builders
// =============================================================================

/// Create a registry pre-loaded with backends.
pub fn make_registry(backends: Vec<Backend>) -> Arc<Registry> {
    let registry = Arc::new(Registry::new());
    for backend in backends {
        registry.add_backend(backend).unwrap();
    }
    registry
}

// =============================================================================
// App Builders
// =============================================================================

/// Create a test app backed by a mock HTTP server with a single healthy backend.
pub async fn make_app_with_mock(
    mock_server: &wiremock::MockServer,
) -> (axum::Router, Arc<Registry>) {
    let registry = Arc::new(Registry::new());
    let config = Arc::new(NexusConfig::default());

    let backend = Backend::new(
        "test-backend".to_string(),
        "Test Backend".to_string(),
        mock_server.uri(),
        BackendType::Generic,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    registry.add_backend(backend).unwrap();

    let _ = registry.update_status("test-backend", BackendStatus::Healthy, None);
    let _ = registry.update_models(
        "test-backend",
        vec![Model {
            id: "test-model".to_string(),
            name: "Test Model".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }],
    );

    let state = Arc::new(AppState::new(registry.clone(), config));
    (create_router(state), registry)
}
