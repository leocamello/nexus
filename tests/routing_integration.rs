//! Integration tests for intelligent routing

use nexus::api::{types::*, AppState};
use nexus::config::NexusConfig;
use nexus::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model, Registry};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::Arc;

fn create_test_backend(id: &str, name: &str, model_id: &str, priority: i32) -> Backend {
    Backend {
        id: id.to_string(),
        name: name.to_string(),
        url: format!("http://{}", name),
        backend_type: BackendType::Ollama,
        status: BackendStatus::Healthy,
        last_health_check: chrono::Utc::now(),
        last_error: None,
        models: vec![Model {
            id: model_id.to_string(),
            name: model_id.to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }],
        priority,
        pending_requests: AtomicU32::new(0),
        total_requests: AtomicU64::new(0),
        avg_latency_ms: AtomicU32::new(50),
        discovery_source: DiscoverySource::Static,
        metadata: HashMap::new(),
    }
}

#[test]
fn test_routing_with_multiple_backends() {
    // Setup registry with multiple backends
    let registry = Arc::new(Registry::new());
    registry
        .add_backend(create_test_backend(
            "backend1",
            "Backend 1",
            "llama3:8b",
            1,
        ))
        .unwrap();
    registry
        .add_backend(create_test_backend(
            "backend2",
            "Backend 2",
            "llama3:8b",
            2,
        ))
        .unwrap();
    registry
        .add_backend(create_test_backend(
            "backend3",
            "Backend 3",
            "mistral:7b",
            1,
        ))
        .unwrap();

    // Create config with smart routing
    let config = Arc::new(NexusConfig::default());

    // Create app state (which creates the router)
    let state = AppState::new(registry, config);

    // Create a simple request
    let request = ChatCompletionRequest {
        model: "llama3:8b".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text {
                content: "Hello".to_string(),
            },
            name: None,
        }],
        stream: false,
        temperature: None,
        max_tokens: None,
        top_p: None,
        stop: None,
        presence_penalty: None,
        frequency_penalty: None,
        user: None,
        extra: HashMap::new(),
    };

    // Extract requirements
    let requirements = nexus::routing::RequestRequirements::from_request(&request);

    // Select backend
    let backend = state.router.select_backend(&requirements).unwrap();

    // Should select one of the llama3:8b backends (Backend 1 or 2)
    assert!(backend.name == "Backend 1" || backend.name == "Backend 2");
    assert_eq!(backend.models[0].id, "llama3:8b");
}

#[test]
fn test_routing_with_aliases() {
    // Setup registry
    let registry = Arc::new(Registry::new());
    registry
        .add_backend(create_test_backend(
            "backend1",
            "Backend 1",
            "llama3:70b",
            1,
        ))
        .unwrap();

    // Create config with aliases
    let mut config = NexusConfig::default();
    config
        .routing
        .aliases
        .insert("gpt-4".to_string(), "llama3:70b".to_string());
    let config = Arc::new(config);

    let state = AppState::new(registry, config);

    let request = ChatCompletionRequest {
        model: "gpt-4".to_string(), // Alias!
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text {
                content: "Hello".to_string(),
            },
            name: None,
        }],
        stream: false,
        temperature: None,
        max_tokens: None,
        top_p: None,
        stop: None,
        presence_penalty: None,
        frequency_penalty: None,
        user: None,
        extra: HashMap::new(),
    };

    let requirements = nexus::routing::RequestRequirements::from_request(&request);
    let backend = state.router.select_backend(&requirements).unwrap();

    // Should resolve alias and select backend
    assert_eq!(backend.name, "Backend 1");
    assert_eq!(backend.models[0].id, "llama3:70b");
}

#[test]
fn test_routing_with_fallbacks() {
    // Setup registry with only fallback model
    let registry = Arc::new(Registry::new());
    registry
        .add_backend(create_test_backend(
            "backend1",
            "Backend 1",
            "mistral:7b",
            1,
        ))
        .unwrap();

    // Create config with fallbacks
    let mut config = NexusConfig::default();
    config.routing.fallbacks.insert(
        "llama3:70b".to_string(),
        vec!["llama3:8b".to_string(), "mistral:7b".to_string()],
    );
    let config = Arc::new(config);

    let state = AppState::new(registry, config);

    let request = ChatCompletionRequest {
        model: "llama3:70b".to_string(), // Not available, will fallback
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text {
                content: "Hello".to_string(),
            },
            name: None,
        }],
        stream: false,
        temperature: None,
        max_tokens: None,
        top_p: None,
        stop: None,
        presence_penalty: None,
        frequency_penalty: None,
        user: None,
        extra: HashMap::new(),
    };

    let requirements = nexus::routing::RequestRequirements::from_request(&request);
    let backend = state.router.select_backend(&requirements).unwrap();

    // Should fallback to mistral
    assert_eq!(backend.name, "Backend 1");
    assert_eq!(backend.models[0].id, "mistral:7b");
}

#[test]
fn test_routing_performance() {
    // Setup registry with many backends
    let registry = Arc::new(Registry::new());
    for i in 0..100 {
        registry
            .add_backend(create_test_backend(
                &format!("backend{}", i),
                &format!("Backend {}", i),
                "llama3:8b",
                i,
            ))
            .unwrap();
    }

    let config = Arc::new(NexusConfig::default());
    let state = AppState::new(registry, config);

    let request = ChatCompletionRequest {
        model: "llama3:8b".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text {
                content: "Hello".to_string(),
            },
            name: None,
        }],
        stream: false,
        temperature: None,
        max_tokens: None,
        top_p: None,
        stop: None,
        presence_penalty: None,
        frequency_penalty: None,
        user: None,
        extra: HashMap::new(),
    };

    let requirements = nexus::routing::RequestRequirements::from_request(&request);

    // Measure routing time
    let start = std::time::Instant::now();
    for _ in 0..1000 {
        let _ = state.router.select_backend(&requirements).unwrap();
    }
    let elapsed = start.elapsed();

    // Average should be < 1ms per routing decision
    let avg_micros = elapsed.as_micros() / 1000;
    println!(
        "Average routing time: {} microseconds",
        avg_micros
    );
    assert!(avg_micros < 1000, "Routing too slow: {} µs", avg_micros);
}
