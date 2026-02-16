//! Integration tests for intelligent routing

mod common;

use nexus::api::{types::*, AppState};
use nexus::config::NexusConfig;
use nexus::registry::Registry;
use std::collections::HashMap;
use std::sync::Arc;

#[test]
fn test_routing_with_multiple_backends() {
    // Setup registry with multiple backends
    let registry = Arc::new(Registry::new());
    registry
        .add_backend(common::make_backend(
            "backend1",
            "Backend 1",
            "llama3:8b",
            1,
        ))
        .unwrap();
    registry
        .add_backend(common::make_backend(
            "backend2",
            "Backend 2",
            "llama3:8b",
            2,
        ))
        .unwrap();
    registry
        .add_backend(common::make_backend(
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
            function_call: None,
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
    let result = state.router.select_backend(&requirements, None).unwrap();

    // Should select one of the llama3:8b backends (Backend 1 or 2)
    assert!(result.backend.name == "Backend 1" || result.backend.name == "Backend 2");
    assert_eq!(result.backend.models[0].id, "llama3:8b");
}

#[test]
fn test_routing_with_aliases() {
    // Setup registry
    let registry = Arc::new(Registry::new());
    registry
        .add_backend(common::make_backend(
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
            function_call: None,
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
    let result = state.router.select_backend(&requirements, None).unwrap();
    let backend = &result.backend;

    // Should resolve alias and select backend
    assert_eq!(backend.name, "Backend 1");
    assert_eq!(backend.models[0].id, "llama3:70b");
    // The resolved model name should be the actual model, not the alias
    assert_eq!(result.actual_model, "llama3:70b");
}

#[test]
fn test_routing_with_fallbacks() {
    // Setup registry with only fallback model
    let registry = Arc::new(Registry::new());
    registry
        .add_backend(common::make_backend(
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
            function_call: None,
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
    let result = state.router.select_backend(&requirements, None).unwrap();
    let backend = &result.backend;

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
            .add_backend(common::make_backend(
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
            function_call: None,
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
        let _ = state.router.select_backend(&requirements, None).unwrap();
    }
    let elapsed = start.elapsed();

    // Average should be < 5ms per routing decision (pipeline adds reconciler overhead)
    // Under coverage instrumentation (tarpaulin), overhead can be 3-5x higher
    let avg_micros = elapsed.as_micros() / 1000;
    println!("Average routing time: {} microseconds", avg_micros);
    assert!(avg_micros < 5000, "Routing too slow: {} µs", avg_micros);
}

// T06: Integration Tests for Model Aliases Feature
#[test]
fn test_routing_with_chained_aliases() {
    // Setup registry with final backend
    let registry = Arc::new(Registry::new());
    registry
        .add_backend(common::make_backend(
            "backend1",
            "Backend 1",
            "llama3:70b",
            1,
        ))
        .unwrap();

    // Create config with 2-level alias chain: gpt-4 → llama-large → llama3:70b
    let mut config = NexusConfig::default();
    config
        .routing
        .aliases
        .insert("gpt-4".to_string(), "llama-large".to_string());
    config
        .routing
        .aliases
        .insert("llama-large".to_string(), "llama3:70b".to_string());
    let config = Arc::new(config);

    let state = AppState::new(registry, config);

    let request = ChatCompletionRequest {
        model: "gpt-4".to_string(), // Will chain through: gpt-4 → llama-large → llama3:70b
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text {
                content: "Hello".to_string(),
            },
            name: None,
            function_call: None,
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
    let result = state.router.select_backend(&requirements, None).unwrap();
    let backend = &result.backend;

    // Should resolve through 2-level chain and select backend
    assert_eq!(backend.name, "Backend 1");
    assert_eq!(backend.models[0].id, "llama3:70b");
}

#[test]
fn test_routing_rejects_circular_config() {
    // Create config with circular aliases: a → b, b → a
    let mut config = NexusConfig::default();
    config
        .routing
        .aliases
        .insert("a".to_string(), "b".to_string());
    config
        .routing
        .aliases
        .insert("b".to_string(), "a".to_string());

    // Validation should fail
    let result = config.validate();
    assert!(matches!(
        result,
        Err(nexus::config::ConfigError::CircularAlias { ref start, ref cycle })
            if (start == "a" && cycle == "a") || (start == "b" && cycle == "b")
    ));
}

#[test]
fn test_routing_with_max_depth_chain() {
    // Setup registry with backends at different chain depths
    let registry = Arc::new(Registry::new());
    registry
        .add_backend(common::make_backend("backend_c", "Backend C", "c", 1))
        .unwrap();
    registry
        .add_backend(common::make_backend("backend_d", "Backend D", "d", 2))
        .unwrap();

    // Create config with 4-level chain: a → b → c → d → e
    // Should stop at 3 levels (max depth) and resolve to "d"
    let mut config = NexusConfig::default();
    config
        .routing
        .aliases
        .insert("a".to_string(), "b".to_string());
    config
        .routing
        .aliases
        .insert("b".to_string(), "c".to_string());
    config
        .routing
        .aliases
        .insert("c".to_string(), "d".to_string());
    config
        .routing
        .aliases
        .insert("d".to_string(), "e".to_string());
    let config = Arc::new(config);

    let state = AppState::new(registry, config);

    let request = ChatCompletionRequest {
        model: "a".to_string(), // Will chain through max 3 levels: a → b → c → d (stops)
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text {
                content: "Test".to_string(),
            },
            name: None,
            function_call: None,
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
    let result = state.router.select_backend(&requirements, None).unwrap();
    let backend = &result.backend;

    // Should stop at max depth (3) and resolve to "d"
    assert_eq!(backend.name, "Backend D");
    assert_eq!(backend.models[0].id, "d");
}

// T09: Header Unit Tests
#[test]
fn test_routing_result_with_alias_and_fallback() {
    // Given alias "alias" → "primary"
    // And fallback "primary" → ["fallback"]
    // And only "fallback" is available
    let registry = Arc::new(Registry::new());
    registry
        .add_backend(common::make_backend(
            "backend_fallback",
            "Backend Fallback",
            "fallback",
            1,
        ))
        .unwrap();

    let mut config = NexusConfig::default();
    config
        .routing
        .aliases
        .insert("alias".to_string(), "primary".to_string());
    config
        .routing
        .fallbacks
        .insert("primary".to_string(), vec!["fallback".to_string()]);
    let config = Arc::new(config);

    let state = AppState::new(registry, config);

    let request = ChatCompletionRequest {
        model: "alias".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text {
                content: "Test".to_string(),
            },
            name: None,
            function_call: None,
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
    let result = state.router.select_backend(&requirements, None).unwrap();

    // Then result.fallback_used == true
    assert!(result.fallback_used, "Expected fallback_used to be true");
    // And result.actual_model == "fallback"
    assert_eq!(result.actual_model, "fallback");
    // And result.backend is the fallback backend
    assert_eq!(result.backend.name, "Backend Fallback");
}

#[test]
fn test_routing_result_no_fallback_info_when_no_fallback_configured() {
    // Given no fallback configured
    let registry = Arc::new(Registry::new());
    registry
        .add_backend(common::make_backend("backend1", "Backend 1", "model1", 1))
        .unwrap();

    let config = Arc::new(NexusConfig::default());
    let state = AppState::new(registry, config);

    let request = ChatCompletionRequest {
        model: "model1".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text {
                content: "Test".to_string(),
            },
            name: None,
            function_call: None,
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
    let result = state.router.select_backend(&requirements, None).unwrap();

    // Then result.fallback_used == false
    assert!(!result.fallback_used, "Expected fallback_used to be false");
    // And result.actual_model == "model1"
    assert_eq!(result.actual_model, "model1");
}

#[test]
fn test_round_robin_routing_with_fallback() {
    let registry = Arc::new(Registry::new());
    registry
        .add_backend(common::make_backend(
            "backend1",
            "Backend 1",
            "fallback-model",
            1,
        ))
        .unwrap();
    registry
        .add_backend(common::make_backend(
            "backend2",
            "Backend 2",
            "fallback-model",
            2,
        ))
        .unwrap();

    let mut config = NexusConfig::default();
    config.routing.strategy = nexus::config::routing::RoutingStrategy::RoundRobin;
    config.routing.fallbacks.insert(
        "primary-model".to_string(),
        vec!["fallback-model".to_string()],
    );
    let config = Arc::new(config);
    let state = AppState::new(registry, config);

    let request = ChatCompletionRequest {
        model: "primary-model".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text {
                content: "Test".to_string(),
            },
            name: None,
            function_call: None,
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
    let result = state.router.select_backend(&requirements, None).unwrap();

    assert!(result.fallback_used);
    assert_eq!(result.actual_model, "fallback-model");
    assert!(result.route_reason.contains("fallback:"));
    assert!(result.route_reason.contains("round_robin:"));
}

#[test]
fn test_priority_routing_with_fallback() {
    let registry = Arc::new(Registry::new());
    registry
        .add_backend(common::make_backend(
            "backend1",
            "Backend Low",
            "fallback-model",
            10,
        ))
        .unwrap();
    registry
        .add_backend(common::make_backend(
            "backend2",
            "Backend High",
            "fallback-model",
            1,
        ))
        .unwrap();

    let mut config = NexusConfig::default();
    config.routing.strategy = nexus::config::routing::RoutingStrategy::PriorityOnly;
    config.routing.fallbacks.insert(
        "primary-model".to_string(),
        vec!["fallback-model".to_string()],
    );
    let config = Arc::new(config);
    let state = AppState::new(registry, config);

    let request = ChatCompletionRequest {
        model: "primary-model".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text {
                content: "Test".to_string(),
            },
            name: None,
            function_call: None,
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
    let result = state.router.select_backend(&requirements, None).unwrap();

    assert!(result.fallback_used);
    assert_eq!(result.actual_model, "fallback-model");
    assert!(result.route_reason.contains("fallback:"));
    assert!(result.route_reason.contains("priority:"));
}

#[test]
fn test_random_routing_with_fallback() {
    let registry = Arc::new(Registry::new());
    registry
        .add_backend(common::make_backend(
            "backend1",
            "Backend 1",
            "fallback-model",
            1,
        ))
        .unwrap();

    let mut config = NexusConfig::default();
    config.routing.strategy = nexus::config::routing::RoutingStrategy::Random;
    config.routing.fallbacks.insert(
        "primary-model".to_string(),
        vec!["fallback-model".to_string()],
    );
    let config = Arc::new(config);
    let state = AppState::new(registry, config);

    let request = ChatCompletionRequest {
        model: "primary-model".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text {
                content: "Test".to_string(),
            },
            name: None,
            function_call: None,
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
    let result = state.router.select_backend(&requirements, None).unwrap();

    assert!(result.fallback_used);
    assert_eq!(result.actual_model, "fallback-model");
    assert!(result.route_reason.contains("fallback:"));
}

#[test]
fn test_round_robin_route_reason_multiple_backends() {
    let registry = Arc::new(Registry::new());
    registry
        .add_backend(common::make_backend("b1", "B1", "model1", 1))
        .unwrap();
    registry
        .add_backend(common::make_backend("b2", "B2", "model1", 1))
        .unwrap();

    let mut config = NexusConfig::default();
    config.routing.strategy = nexus::config::routing::RoutingStrategy::RoundRobin;
    let config = Arc::new(config);
    let state = AppState::new(registry, config);

    let request = ChatCompletionRequest {
        model: "model1".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text {
                content: "Test".to_string(),
            },
            name: None,
            function_call: None,
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
    let result = state.router.select_backend(&requirements, None).unwrap();

    assert!(!result.fallback_used);
    assert!(result.route_reason.starts_with("round_robin:index_"));
}

#[test]
fn test_priority_route_reason_multiple_backends() {
    let registry = Arc::new(Registry::new());
    registry
        .add_backend(common::make_backend("b1", "B1", "model1", 5))
        .unwrap();
    registry
        .add_backend(common::make_backend("b2", "B2", "model1", 1))
        .unwrap();

    let mut config = NexusConfig::default();
    config.routing.strategy = nexus::config::routing::RoutingStrategy::PriorityOnly;
    let config = Arc::new(config);
    let state = AppState::new(registry, config);

    let request = ChatCompletionRequest {
        model: "model1".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text {
                content: "Test".to_string(),
            },
            name: None,
            function_call: None,
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
    let result = state.router.select_backend(&requirements, None).unwrap();

    assert!(!result.fallback_used);
    assert!(result.route_reason.starts_with("priority:"));
}

#[test]
fn test_random_route_reason_multiple_backends() {
    let registry = Arc::new(Registry::new());
    registry
        .add_backend(common::make_backend("b1", "B1", "model1", 1))
        .unwrap();
    registry
        .add_backend(common::make_backend("b2", "B2", "model1", 1))
        .unwrap();

    let mut config = NexusConfig::default();
    config.routing.strategy = nexus::config::routing::RoutingStrategy::Random;
    let config = Arc::new(config);
    let state = AppState::new(registry, config);

    let request = ChatCompletionRequest {
        model: "model1".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text {
                content: "Test".to_string(),
            },
            name: None,
            function_call: None,
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
    let result = state.router.select_backend(&requirements, None).unwrap();

    assert!(!result.fallback_used);
    assert!(result.route_reason.starts_with("random:"));
}
