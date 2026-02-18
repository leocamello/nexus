use super::*;
use std::collections::HashMap;

#[test]
fn test_backend_type_serialization() {
    use serde_json;

    // BackendType::Ollama serializes to "ollama"
    let backend_type = BackendType::Ollama;
    let json = serde_json::to_string(&backend_type).unwrap();
    assert_eq!(json, r#""ollama""#);

    // Deserialize back
    let deserialized: BackendType = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, backend_type);
}

#[test]
fn test_backend_status_serialization() {
    use serde_json;

    // BackendStatus::Healthy serializes to "healthy"
    let status = BackendStatus::Healthy;
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, r#""healthy""#);

    // Deserialize back
    let deserialized: BackendStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, status);
}

#[test]
fn test_discovery_source_serialization() {
    use serde_json;

    // DiscoverySource::MDNS serializes to "mdns"
    let source = DiscoverySource::MDNS;
    let json = serde_json::to_string(&source).unwrap();
    assert_eq!(json, r#""mdns""#);

    // Deserialize back
    let deserialized: DiscoverySource = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, source);
}

#[test]
fn test_model_creation() {
    // Model can be created with all fields
    let model = Model {
        id: "llama3:70b".to_string(),
        name: "Llama 3 70B".to_string(),
        context_length: 8192,
        supports_vision: false,
        supports_tools: true,
        supports_json_mode: true,
        max_output_tokens: Some(4096),
    };

    assert_eq!(model.id, "llama3:70b");
    assert_eq!(model.name, "Llama 3 70B");
    assert_eq!(model.context_length, 8192);
    assert!(!model.supports_vision);
    assert!(model.supports_tools);
    assert!(model.supports_json_mode);
    assert_eq!(model.max_output_tokens, Some(4096));
}

#[test]
fn test_model_json_roundtrip() {
    use serde_json;

    // Model serializes to JSON and deserializes back
    let model = Model {
        id: "gpt-4".to_string(),
        name: "GPT-4".to_string(),
        context_length: 128000,
        supports_vision: true,
        supports_tools: true,
        supports_json_mode: true,
        max_output_tokens: Some(4096),
    };

    let json = serde_json::to_string(&model).unwrap();
    let deserialized: Model = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.id, model.id);
    assert_eq!(deserialized.name, model.name);
    assert_eq!(deserialized.context_length, model.context_length);
    assert_eq!(deserialized.supports_vision, model.supports_vision);
    assert_eq!(deserialized.supports_tools, model.supports_tools);
    assert_eq!(deserialized.supports_json_mode, model.supports_json_mode);
    assert_eq!(deserialized.max_output_tokens, model.max_output_tokens);
}

// T03 Tests - Backend struct & BackendView

#[test]
fn test_backend_creation() {
    // Backend can be created with all fields
    let models = vec![Model {
        id: "llama3".to_string(),
        name: "Llama 3".to_string(),
        context_length: 8192,
        supports_vision: false,
        supports_tools: true,
        supports_json_mode: true,
        max_output_tokens: Some(4096),
    }];

    let mut metadata = HashMap::new();
    metadata.insert("region".to_string(), "us-west-2".to_string());

    let backend = Backend::new(
        "test-id".to_string(),
        "Test Backend".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        models.clone(),
        DiscoverySource::Static,
        metadata.clone(),
    );

    assert_eq!(backend.id, "test-id");
    assert_eq!(backend.name, "Test Backend");
    assert_eq!(backend.url, "http://localhost:11434");
    assert_eq!(backend.backend_type, BackendType::Ollama);
    assert_eq!(backend.status, BackendStatus::Unknown);
    assert_eq!(backend.models, models);
    assert_eq!(backend.priority, 0);
    assert_eq!(backend.discovery_source, DiscoverySource::Static);
    assert_eq!(backend.metadata, metadata);
}

#[test]
fn test_backend_default_values() {
    // New backend has sensible defaults (pending=0, total=0, etc.)
    let backend = Backend::new(
        "test-id".to_string(),
        "Test Backend".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    assert_eq!(
        backend
            .pending_requests
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    assert_eq!(
        backend
            .total_requests
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    assert_eq!(
        backend
            .avg_latency_ms
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    assert_eq!(backend.status, BackendStatus::Unknown);
    assert_eq!(backend.priority, 0);
    assert!(backend.last_error.is_none());
}

#[test]
fn test_backend_view_from_backend() {
    // BackendView can be created from Backend
    let models = vec![Model {
        id: "llama3".to_string(),
        name: "Llama 3".to_string(),
        context_length: 8192,
        supports_vision: false,
        supports_tools: true,
        supports_json_mode: true,
        max_output_tokens: Some(4096),
    }];

    let backend = Backend::new(
        "test-id".to_string(),
        "Test Backend".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        models.clone(),
        DiscoverySource::Static,
        HashMap::new(),
    );

    let view: BackendView = (&backend).into();

    assert_eq!(view.id, backend.id);
    assert_eq!(view.name, backend.name);
    assert_eq!(view.url, backend.url);
    assert_eq!(view.backend_type, backend.backend_type);
    assert_eq!(view.status, backend.status);
    assert_eq!(view.models, backend.models);
    assert_eq!(view.priority, backend.priority);
    assert_eq!(view.pending_requests, 0);
    assert_eq!(view.total_requests, 0);
    assert_eq!(view.avg_latency_ms, 0);
}

#[test]
fn test_backend_view_json_roundtrip() {
    use serde_json;

    // BackendView serializes to JSON correctly
    let models = vec![Model {
        id: "llama3".to_string(),
        name: "Llama 3".to_string(),
        context_length: 8192,
        supports_vision: false,
        supports_tools: true,
        supports_json_mode: true,
        max_output_tokens: Some(4096),
    }];

    let backend = Backend::new(
        "test-id".to_string(),
        "Test Backend".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        models.clone(),
        DiscoverySource::Static,
        HashMap::new(),
    );

    let view: BackendView = (&backend).into();
    let json = serde_json::to_string(&view).unwrap();

    // Verify it deserializes back
    let deserialized: BackendView = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, view.id);
    assert_eq!(deserialized.name, view.name);
    assert_eq!(deserialized.url, view.url);
    assert_eq!(deserialized.backend_type, view.backend_type);
}

// T04 Tests - RegistryError

#[test]
fn test_error_duplicate_backend() {
    // DuplicateBackend error contains the ID
    let error = RegistryError::DuplicateBackend("backend-123".to_string());
    let error_msg = format!("{}", error);
    assert!(error_msg.contains("backend-123"));
    assert!(error_msg.contains("already exists"));
}

#[test]
fn test_error_backend_not_found() {
    // BackendNotFound error contains the ID
    let error = RegistryError::BackendNotFound("backend-456".to_string());
    let error_msg = format!("{}", error);
    assert!(error_msg.contains("backend-456"));
    assert!(error_msg.contains("not found"));
}

#[test]
fn test_error_display() {
    // Errors implement Display with useful messages
    let dup_error = RegistryError::DuplicateBackend("id-1".to_string());
    let not_found_error = RegistryError::BackendNotFound("id-2".to_string());

    assert_eq!(format!("{}", dup_error), "backend already exists: id-1");
    assert_eq!(format!("{}", not_found_error), "backend not found: id-2");

    // Verify they implement std::error::Error
    use std::error::Error;
    let _: &dyn Error = &dup_error;
    let _: &dyn Error = &not_found_error;
}

// T05 Tests - Registry core operations

#[test]
fn test_registry_new_empty() {
    // New registry has 0 backends
    let registry = Registry::new();
    assert_eq!(registry.backend_count(), 0);
}

#[test]
fn test_add_backend_success() {
    // Adding backend stores it and can be retrieved
    let registry = Registry::new();

    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    let result = registry.add_backend(backend);
    assert!(result.is_ok());
    assert_eq!(registry.backend_count(), 1);

    let retrieved = registry.get_backend("backend-1");
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.id, "backend-1");
    assert_eq!(retrieved.name, "Backend 1");
}

#[test]
fn test_add_backend_duplicate_error() {
    // Adding duplicate ID returns DuplicateBackend error
    let registry = Registry::new();

    let backend1 = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    let backend2 = Backend::new(
        "backend-1".to_string(),
        "Backend 1 Duplicate".to_string(),
        "http://localhost:11435".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend1).unwrap();
    let result = registry.add_backend(backend2);

    assert!(matches!(result, Err(RegistryError::DuplicateBackend(ref id)) if id == "backend-1"));
}

#[test]
fn test_remove_backend_success() {
    // Removing backend returns it and removes from registry
    let registry = Registry::new();

    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend).unwrap();
    assert_eq!(registry.backend_count(), 1);

    let removed = registry.remove_backend("backend-1");
    assert!(removed.is_ok());
    assert_eq!(registry.backend_count(), 0);

    let removed_backend = removed.unwrap();
    assert_eq!(removed_backend.id, "backend-1");
}

#[test]
fn test_remove_backend_not_found() {
    // Removing non-existent ID returns BackendNotFound error
    let registry = Registry::new();

    let result = registry.remove_backend("nonexistent");
    assert!(matches!(result, Err(RegistryError::BackendNotFound(ref id)) if id == "nonexistent"));
}

#[test]
fn test_get_backend_found() {
    // Getting existing backend returns Some(backend)
    let registry = Registry::new();

    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend).unwrap();

    let result = registry.get_backend("backend-1");
    assert!(result.is_some());
    assert_eq!(result.unwrap().id, "backend-1");
}

#[test]
fn test_get_backend_not_found() {
    // Getting non-existent ID returns None
    let registry = Registry::new();

    let result = registry.get_backend("nonexistent");
    assert!(result.is_none());
}

#[test]
fn test_get_all_backends() {
    // Returns all registered backends
    let registry = Registry::new();

    for i in 1..=3 {
        let backend = Backend::new(
            format!("backend-{}", i),
            format!("Backend {}", i),
            format!("http://localhost:1143{}", i),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            HashMap::new(),
        );
        registry.add_backend(backend).unwrap();
    }

    let all_backends = registry.get_all_backends();
    assert_eq!(all_backends.len(), 3);

    let ids: Vec<String> = all_backends.iter().map(|b| b.id.clone()).collect();
    assert!(ids.contains(&"backend-1".to_string()));
    assert!(ids.contains(&"backend-2".to_string()));
    assert!(ids.contains(&"backend-3".to_string()));
}

#[test]
fn test_backend_count() {
    // Returns correct count after add/remove
    let registry = Registry::new();
    assert_eq!(registry.backend_count(), 0);

    let backend1 = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend1).unwrap();
    assert_eq!(registry.backend_count(), 1);

    let backend2 = Backend::new(
        "backend-2".to_string(),
        "Backend 2".to_string(),
        "http://localhost:11435".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend2).unwrap();
    assert_eq!(registry.backend_count(), 2);

    registry.remove_backend("backend-1").unwrap();
    assert_eq!(registry.backend_count(), 1);
}

// T06 Tests - Model index & queries

#[test]
fn test_model_index_updated_on_add() {
    // Adding backend updates model index
    let registry = Registry::new();

    let models = vec![
        Model {
            id: "llama3".to_string(),
            name: "Llama 3".to_string(),
            context_length: 8192,
            supports_vision: false,
            supports_tools: true,
            supports_json_mode: true,
            max_output_tokens: Some(4096),
        },
        Model {
            id: "mistral".to_string(),
            name: "Mistral".to_string(),
            context_length: 8192,
            supports_vision: false,
            supports_tools: true,
            supports_json_mode: true,
            max_output_tokens: Some(4096),
        },
    ];

    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        models,
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend).unwrap();

    // Check that models are indexed
    let llama3_backends = registry.get_backends_for_model("llama3");
    assert_eq!(llama3_backends.len(), 1);
    assert_eq!(llama3_backends[0].id, "backend-1");

    let mistral_backends = registry.get_backends_for_model("mistral");
    assert_eq!(mistral_backends.len(), 1);
    assert_eq!(mistral_backends[0].id, "backend-1");
}

#[test]
fn test_model_index_updated_on_remove() {
    // Removing backend cleans up model index
    let registry = Registry::new();

    let models = vec![Model {
        id: "llama3".to_string(),
        name: "Llama 3".to_string(),
        context_length: 8192,
        supports_vision: false,
        supports_tools: true,
        supports_json_mode: true,
        max_output_tokens: Some(4096),
    }];

    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        models,
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend).unwrap();

    // Verify model is indexed
    let backends = registry.get_backends_for_model("llama3");
    assert_eq!(backends.len(), 1);

    // Remove backend
    registry.remove_backend("backend-1").unwrap();

    // Verify model index is cleaned up
    let backends = registry.get_backends_for_model("llama3");
    assert_eq!(backends.len(), 0);
}

#[test]
fn test_get_backends_for_model_single() {
    // Returns single backend with model
    let registry = Registry::new();

    let models = vec![Model {
        id: "llama3".to_string(),
        name: "Llama 3".to_string(),
        context_length: 8192,
        supports_vision: false,
        supports_tools: true,
        supports_json_mode: true,
        max_output_tokens: Some(4096),
    }];

    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        models,
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend).unwrap();

    let backends = registry.get_backends_for_model("llama3");
    assert_eq!(backends.len(), 1);
    assert_eq!(backends[0].id, "backend-1");
}

#[test]
fn test_get_backends_for_model_multiple() {
    // Returns multiple backends with same model
    let registry = Registry::new();

    for i in 1..=3 {
        let models = vec![Model {
            id: "llama3".to_string(),
            name: "Llama 3".to_string(),
            context_length: 8192,
            supports_vision: false,
            supports_tools: true,
            supports_json_mode: true,
            max_output_tokens: Some(4096),
        }];

        let backend = Backend::new(
            format!("backend-{}", i),
            format!("Backend {}", i),
            format!("http://localhost:1143{}", i),
            BackendType::Ollama,
            models,
            DiscoverySource::Static,
            HashMap::new(),
        );

        registry.add_backend(backend).unwrap();
    }

    let backends = registry.get_backends_for_model("llama3");
    assert_eq!(backends.len(), 3);

    let ids: Vec<String> = backends.iter().map(|b| b.id.clone()).collect();
    assert!(ids.contains(&"backend-1".to_string()));
    assert!(ids.contains(&"backend-2".to_string()));
    assert!(ids.contains(&"backend-3".to_string()));
}

#[test]
fn test_get_backends_for_model_none() {
    // Returns empty vec for unknown model
    let registry = Registry::new();

    let backends = registry.get_backends_for_model("nonexistent-model");
    assert_eq!(backends.len(), 0);
}

#[test]
fn test_get_healthy_backends_includes_healthy() {
    // Includes backends with Healthy status
    let registry = Registry::new();

    let mut backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    backend.status = BackendStatus::Healthy;

    registry.add_backend(backend).unwrap();

    let healthy = registry.get_healthy_backends();
    assert_eq!(healthy.len(), 1);
    assert_eq!(healthy[0].id, "backend-1");
}

#[test]
fn test_get_healthy_backends_excludes_unhealthy() {
    // Excludes backends with Unhealthy status
    let registry = Registry::new();

    let mut backend1 = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    backend1.status = BackendStatus::Healthy;

    let mut backend2 = Backend::new(
        "backend-2".to_string(),
        "Backend 2".to_string(),
        "http://localhost:11435".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    backend2.status = BackendStatus::Unhealthy;

    registry.add_backend(backend1).unwrap();
    registry.add_backend(backend2).unwrap();

    let healthy = registry.get_healthy_backends();
    assert_eq!(healthy.len(), 1);
    assert_eq!(healthy[0].id, "backend-1");
}

#[test]
fn test_get_healthy_backends_excludes_draining() {
    // Excludes backends with Draining status
    let registry = Registry::new();

    let mut backend1 = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    backend1.status = BackendStatus::Healthy;

    let mut backend2 = Backend::new(
        "backend-2".to_string(),
        "Backend 2".to_string(),
        "http://localhost:11435".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    backend2.status = BackendStatus::Draining;

    registry.add_backend(backend1).unwrap();
    registry.add_backend(backend2).unwrap();

    let healthy = registry.get_healthy_backends();
    assert_eq!(healthy.len(), 1);
    assert_eq!(healthy[0].id, "backend-1");
}

#[test]
fn test_model_count() {
    // Counts unique models across all backends
    let registry = Registry::new();

    // Backend 1 with llama3 and mistral
    let models1 = vec![
        Model {
            id: "llama3".to_string(),
            name: "Llama 3".to_string(),
            context_length: 8192,
            supports_vision: false,
            supports_tools: true,
            supports_json_mode: true,
            max_output_tokens: Some(4096),
        },
        Model {
            id: "mistral".to_string(),
            name: "Mistral".to_string(),
            context_length: 8192,
            supports_vision: false,
            supports_tools: true,
            supports_json_mode: true,
            max_output_tokens: Some(4096),
        },
    ];

    let backend1 = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        models1,
        DiscoverySource::Static,
        HashMap::new(),
    );

    // Backend 2 with llama3 and qwen (llama3 is duplicate)
    let models2 = vec![
        Model {
            id: "llama3".to_string(),
            name: "Llama 3".to_string(),
            context_length: 8192,
            supports_vision: false,
            supports_tools: true,
            supports_json_mode: true,
            max_output_tokens: Some(4096),
        },
        Model {
            id: "qwen".to_string(),
            name: "Qwen".to_string(),
            context_length: 8192,
            supports_vision: false,
            supports_tools: true,
            supports_json_mode: true,
            max_output_tokens: Some(4096),
        },
    ];

    let backend2 = Backend::new(
        "backend-2".to_string(),
        "Backend 2".to_string(),
        "http://localhost:11435".to_string(),
        BackendType::Ollama,
        models2,
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend1).unwrap();
    registry.add_backend(backend2).unwrap();

    // Should have 3 unique models: llama3, mistral, qwen
    assert_eq!(registry.model_count(), 3);
}

// T07 Tests - Status & model updates

#[test]
fn test_update_status_changes_status() {
    // Status changes from Healthy to Unhealthy
    let registry = Registry::new();

    let mut backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    backend.status = BackendStatus::Healthy;

    registry.add_backend(backend).unwrap();

    // Update status to Unhealthy
    registry
        .update_status(
            "backend-1",
            BackendStatus::Unhealthy,
            Some("Connection error".to_string()),
        )
        .unwrap();

    let updated = registry.get_backend("backend-1").unwrap();
    assert_eq!(updated.status, BackendStatus::Unhealthy);
}

#[test]
fn test_update_status_sets_timestamp() {
    use std::thread;
    use std::time::Duration;

    // last_health_check is updated
    let registry = Registry::new();

    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    let old_timestamp = backend.last_health_check;
    registry.add_backend(backend).unwrap();

    // Wait a bit to ensure timestamp difference
    thread::sleep(Duration::from_millis(10));

    registry
        .update_status("backend-1", BackendStatus::Healthy, None)
        .unwrap();

    let updated = registry.get_backend("backend-1").unwrap();
    assert!(updated.last_health_check > old_timestamp);
}

#[test]
fn test_update_status_sets_error() {
    // last_error is set when status is Unhealthy
    let registry = Registry::new();

    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend).unwrap();

    registry
        .update_status(
            "backend-1",
            BackendStatus::Unhealthy,
            Some("Connection refused".to_string()),
        )
        .unwrap();

    let updated = registry.get_backend("backend-1").unwrap();
    assert_eq!(updated.status, BackendStatus::Unhealthy);
    assert_eq!(updated.last_error, Some("Connection refused".to_string()));
}

#[test]
fn test_update_status_clears_error() {
    // last_error is cleared when status becomes Healthy
    let registry = Registry::new();

    let mut backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    backend.last_error = Some("Previous error".to_string());

    registry.add_backend(backend).unwrap();

    registry
        .update_status("backend-1", BackendStatus::Healthy, None)
        .unwrap();

    let updated = registry.get_backend("backend-1").unwrap();
    assert_eq!(updated.status, BackendStatus::Healthy);
    assert!(updated.last_error.is_none());
}

#[test]
fn test_update_status_not_found() {
    // Returns error for unknown backend ID
    let registry = Registry::new();

    let result = registry.update_status("nonexistent", BackendStatus::Healthy, None);
    assert!(matches!(result, Err(RegistryError::BackendNotFound(ref id)) if id == "nonexistent"));
}

#[test]
fn test_update_models_replaces_list() {
    // Model list is completely replaced
    let registry = Registry::new();

    let models = vec![Model {
        id: "llama3".to_string(),
        name: "Llama 3".to_string(),
        context_length: 8192,
        supports_vision: false,
        supports_tools: true,
        supports_json_mode: true,
        max_output_tokens: Some(4096),
    }];

    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        models,
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend).unwrap();

    // Replace with new models
    let new_models = vec![
        Model {
            id: "mistral".to_string(),
            name: "Mistral".to_string(),
            context_length: 8192,
            supports_vision: false,
            supports_tools: true,
            supports_json_mode: true,
            max_output_tokens: Some(4096),
        },
        Model {
            id: "qwen".to_string(),
            name: "Qwen".to_string(),
            context_length: 8192,
            supports_vision: false,
            supports_tools: true,
            supports_json_mode: true,
            max_output_tokens: Some(4096),
        },
    ];

    registry
        .update_models("backend-1", new_models.clone())
        .unwrap();

    let updated = registry.get_backend("backend-1").unwrap();
    assert_eq!(updated.models.len(), 2);
    assert_eq!(updated.models[0].id, "mistral");
    assert_eq!(updated.models[1].id, "qwen");
}

#[test]
fn test_update_models_updates_index() {
    // Model index reflects new models, removes old
    let registry = Registry::new();

    let models = vec![
        Model {
            id: "llama3".to_string(),
            name: "Llama 3".to_string(),
            context_length: 8192,
            supports_vision: false,
            supports_tools: true,
            supports_json_mode: true,
            max_output_tokens: Some(4096),
        },
        Model {
            id: "mistral".to_string(),
            name: "Mistral".to_string(),
            context_length: 8192,
            supports_vision: false,
            supports_tools: true,
            supports_json_mode: true,
            max_output_tokens: Some(4096),
        },
    ];

    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        models,
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend).unwrap();

    // Verify old models are indexed
    assert_eq!(registry.get_backends_for_model("llama3").len(), 1);
    assert_eq!(registry.get_backends_for_model("mistral").len(), 1);

    // Replace with new models (mistral + qwen)
    let new_models = vec![
        Model {
            id: "mistral".to_string(),
            name: "Mistral".to_string(),
            context_length: 8192,
            supports_vision: false,
            supports_tools: true,
            supports_json_mode: true,
            max_output_tokens: Some(4096),
        },
        Model {
            id: "qwen".to_string(),
            name: "Qwen".to_string(),
            context_length: 8192,
            supports_vision: false,
            supports_tools: true,
            supports_json_mode: true,
            max_output_tokens: Some(4096),
        },
    ];

    registry.update_models("backend-1", new_models).unwrap();

    // Verify index is updated
    assert_eq!(registry.get_backends_for_model("llama3").len(), 0); // Removed
    assert_eq!(registry.get_backends_for_model("mistral").len(), 1); // Still there
    assert_eq!(registry.get_backends_for_model("qwen").len(), 1); // Added
}

#[test]
fn test_update_models_not_found() {
    // Returns error for unknown backend ID
    let registry = Registry::new();

    let models = vec![Model {
        id: "llama3".to_string(),
        name: "Llama 3".to_string(),
        context_length: 8192,
        supports_vision: false,
        supports_tools: true,
        supports_json_mode: true,
        max_output_tokens: Some(4096),
    }];

    let result = registry.update_models("nonexistent", models);
    assert!(matches!(result, Err(RegistryError::BackendNotFound(ref id)) if id == "nonexistent"));
}

// T08 Tests - Atomic counters

#[test]
fn test_increment_pending_success() {
    // pending_requests increases by 1
    let registry = Registry::new();

    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend).unwrap();

    let new_val = registry.increment_pending("backend-1").unwrap();
    assert_eq!(new_val, 1);

    let backend = registry.get_backend("backend-1").unwrap();
    assert_eq!(
        backend
            .pending_requests
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
}

#[test]
fn test_increment_pending_returns_new_value() {
    // Returns the value after increment
    let registry = Registry::new();

    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend).unwrap();

    assert_eq!(registry.increment_pending("backend-1").unwrap(), 1);
    assert_eq!(registry.increment_pending("backend-1").unwrap(), 2);
    assert_eq!(registry.increment_pending("backend-1").unwrap(), 3);
}

#[test]
fn test_decrement_pending_success() {
    // pending_requests decreases by 1
    let registry = Registry::new();

    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend).unwrap();

    registry.increment_pending("backend-1").unwrap();
    registry.increment_pending("backend-1").unwrap();

    let new_val = registry.decrement_pending("backend-1").unwrap();
    assert_eq!(new_val, 1);

    let backend = registry.get_backend("backend-1").unwrap();
    assert_eq!(
        backend
            .pending_requests
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
}

#[test]
fn test_decrement_pending_clamps_to_zero() {
    // Never goes negative, stays at 0
    let registry = Registry::new();

    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend).unwrap();

    // Decrement when already at 0
    let new_val = registry.decrement_pending("backend-1").unwrap();
    assert_eq!(new_val, 0);

    let backend = registry.get_backend("backend-1").unwrap();
    assert_eq!(
        backend
            .pending_requests
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

#[test]
fn test_decrement_pending_at_zero_logs_warning() {
    // Tracing warning emitted (we'll just verify it doesn't panic)
    let registry = Registry::new();

    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend).unwrap();

    // This should log a warning but not panic
    let result = registry.decrement_pending("backend-1");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 0);
}

#[test]
fn test_update_latency_first_sample() {
    // First sample sets the initial value
    let registry = Registry::new();

    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend).unwrap();

    registry.update_latency("backend-1", 100).unwrap();

    let backend = registry.get_backend("backend-1").unwrap();
    assert_eq!(
        backend
            .avg_latency_ms
            .load(std::sync::atomic::Ordering::SeqCst),
        100
    );
}

#[test]
fn test_update_latency_ema_calculation() {
    // Verify EMA: new = (sample + 4*old) / 5
    let registry = Registry::new();

    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend).unwrap();

    // First sample: 100
    registry.update_latency("backend-1", 100).unwrap();
    let backend = registry.get_backend("backend-1").unwrap();
    assert_eq!(
        backend
            .avg_latency_ms
            .load(std::sync::atomic::Ordering::SeqCst),
        100
    );

    // Second sample: 200
    // EMA = (200 + 4*100) / 5 = 600 / 5 = 120
    registry.update_latency("backend-1", 200).unwrap();
    let backend = registry.get_backend("backend-1").unwrap();
    assert_eq!(
        backend
            .avg_latency_ms
            .load(std::sync::atomic::Ordering::SeqCst),
        120
    );

    // Third sample: 150
    // EMA = (150 + 4*120) / 5 = 630 / 5 = 126
    registry.update_latency("backend-1", 150).unwrap();
    let backend = registry.get_backend("backend-1").unwrap();
    assert_eq!(
        backend
            .avg_latency_ms
            .load(std::sync::atomic::Ordering::SeqCst),
        126
    );
}

#[test]
fn test_update_latency_zero_valid() {
    // 0ms is accepted as valid latency
    let registry = Registry::new();

    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend).unwrap();

    let result = registry.update_latency("backend-1", 0);
    assert!(result.is_ok());

    let backend = registry.get_backend("backend-1").unwrap();
    assert_eq!(
        backend
            .avg_latency_ms
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

// T09 Tests - Property-based tests

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_increment_decrement_balanced(n in 1u32..100) {
            // n increments followed by n decrements = 0
            let registry = Registry::new();

            let backend = Backend::new(
                "backend-1".to_string(),
                "Backend 1".to_string(),
                "http://localhost:11434".to_string(),
                BackendType::Ollama,
                vec![],
                DiscoverySource::Static,
                HashMap::new(),
            );

            registry.add_backend(backend).unwrap();

            // Increment n times
            for _ in 0..n {
                registry.increment_pending("backend-1").unwrap();
            }

            // Decrement n times
            for _ in 0..n {
                registry.decrement_pending("backend-1").unwrap();
            }

            // Should be back at 0
            let backend = registry.get_backend("backend-1").unwrap();
            prop_assert_eq!(backend.pending_requests.load(std::sync::atomic::Ordering::SeqCst), 0);
        }

        #[test]
        fn prop_concurrent_increments_correct(n in 1u32..50) {
            // n concurrent increments result in pending_requests == n
            use std::sync::Arc;

            let registry = Arc::new(Registry::new());

            let backend = Backend::new(
                "backend-1".to_string(),
                "Backend 1".to_string(),
                "http://localhost:11434".to_string(),
                BackendType::Ollama,
                vec![],
                DiscoverySource::Static,
                HashMap::new(),
            );

            registry.add_backend(backend).unwrap();

            // Spawn n tasks to increment concurrently
            let mut handles = vec![];
            for _ in 0..n {
                let reg = Arc::clone(&registry);
                let handle = std::thread::spawn(move || {
                    reg.increment_pending("backend-1").unwrap();
                });
                handles.push(handle);
            }

            // Wait for all to complete
            for handle in handles {
                handle.join().unwrap();
            }

            // Final count should be n
            let backend = registry.get_backend("backend-1").unwrap();
            prop_assert_eq!(backend.pending_requests.load(std::sync::atomic::Ordering::SeqCst), n);
        }

        #[test]
        fn prop_latency_bounded(samples in proptest::collection::vec(0u32..10000, 1..100)) {
            // After any sequence of updates, latency is within [min, max] of samples
            let registry = Registry::new();

            let backend = Backend::new(
                "backend-1".to_string(),
                "Backend 1".to_string(),
                "http://localhost:11434".to_string(),
                BackendType::Ollama,
                vec![],
                DiscoverySource::Static,
                HashMap::new(),
            );

            registry.add_backend(backend).unwrap();

            let min_sample = *samples.iter().min().unwrap();
            let max_sample = *samples.iter().max().unwrap();

            // Apply all samples
            for sample in &samples {
                registry.update_latency("backend-1", *sample).unwrap();
            }

            // Final latency should be within bounds
            let backend = registry.get_backend("backend-1").unwrap();
            let final_latency = backend.avg_latency_ms.load(std::sync::atomic::Ordering::SeqCst);

            prop_assert!(final_latency >= min_sample);
            prop_assert!(final_latency <= max_sample);
        }

        #[test]
        fn prop_decrement_never_negative(decrements in 1u32..100) {
            // Any number of decrements on empty counter stays at 0
            let registry = Registry::new();

            let backend = Backend::new(
                "backend-1".to_string(),
                "Backend 1".to_string(),
                "http://localhost:11434".to_string(),
                BackendType::Ollama,
                vec![],
                DiscoverySource::Static,
                HashMap::new(),
            );

            registry.add_backend(backend).unwrap();

            // Decrement many times from 0
            for _ in 0..decrements {
                let val = registry.decrement_pending("backend-1").unwrap();
                prop_assert_eq!(val, 0);
            }

            // Should still be at 0
            let backend = registry.get_backend("backend-1").unwrap();
            prop_assert_eq!(backend.pending_requests.load(std::sync::atomic::Ordering::SeqCst), 0);
        }
    }
}

// T10 Tests - Concurrency stress tests

#[tokio::test]
async fn test_concurrent_reads_no_deadlock() {
    // 10,000 concurrent get_backend calls complete
    use std::sync::Arc;
    use tokio::time::{timeout, Duration};

    let registry = Arc::new(Registry::new());

    // Add a backend
    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    registry.add_backend(backend).unwrap();

    // Spawn 10,000 concurrent reads
    let mut handles = vec![];
    for _ in 0..10_000 {
        let reg = Arc::clone(&registry);
        let handle = tokio::spawn(async move { reg.get_backend("backend-1") });
        handles.push(handle);
    }

    // All reads should complete within 5 seconds
    let result = timeout(Duration::from_secs(5), async {
        for handle in handles {
            handle.await.unwrap();
        }
    })
    .await;

    assert!(
        result.is_ok(),
        "Concurrent reads should complete without deadlock"
    );
}

#[tokio::test]
async fn test_concurrent_read_write_safe() {
    // Mixed read/write workload completes without panic
    use std::sync::Arc;
    use tokio::time::{timeout, Duration};

    let registry = Arc::new(Registry::new());

    // Add initial backends
    for i in 0..10 {
        let backend = Backend::new(
            format!("backend-{}", i),
            format!("Backend {}", i),
            format!("http://localhost:1143{}", i),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            HashMap::new(),
        );
        registry.add_backend(backend).unwrap();
    }

    let mut handles = vec![];

    // Spawn readers
    for _ in 0..100 {
        let reg = Arc::clone(&registry);
        let handle = tokio::spawn(async move {
            for i in 0..100 {
                reg.get_backend(&format!("backend-{}", i % 10));
            }
        });
        handles.push(handle);
    }

    // Spawn writers (status updates)
    for _ in 0..50 {
        let reg = Arc::clone(&registry);
        let handle = tokio::spawn(async move {
            for i in 0..100 {
                let _ =
                    reg.update_status(&format!("backend-{}", i % 10), BackendStatus::Healthy, None);
            }
        });
        handles.push(handle);
    }

    // All should complete within 5 seconds
    let result = timeout(Duration::from_secs(5), async {
        for handle in handles {
            handle.await.unwrap();
        }
    })
    .await;

    assert!(result.is_ok(), "Mixed read/write should complete safely");
}

#[tokio::test]
async fn test_concurrent_add_remove_same_id() {
    // Concurrent add/remove of same ID: no panic, consistent state
    use std::sync::Arc;
    use tokio::time::{timeout, Duration};

    let registry = Arc::new(Registry::new());

    let mut handles = vec![];

    // Spawn tasks that add and remove the same backend ID
    for _ in 0..100 {
        let reg = Arc::clone(&registry);
        let handle = tokio::spawn(async move {
            let backend = Backend::new(
                "contested-backend".to_string(),
                "Contested".to_string(),
                "http://localhost:11434".to_string(),
                BackendType::Ollama,
                vec![],
                DiscoverySource::Static,
                HashMap::new(),
            );

            // Try to add (may fail if already exists)
            let _ = reg.add_backend(backend);

            // Try to remove (may fail if not exists)
            let _ = reg.remove_backend("contested-backend");
        });
        handles.push(handle);
    }

    // All should complete within 5 seconds without panic
    let result = timeout(Duration::from_secs(5), async {
        for handle in handles {
            handle.await.unwrap();
        }
    })
    .await;

    assert!(result.is_ok(), "Concurrent add/remove should not panic");
}

#[tokio::test]
async fn test_concurrent_model_queries() {
    // Concurrent get_backends_for_model with updates: consistent results
    use std::sync::Arc;
    use tokio::time::{timeout, Duration};

    let registry = Arc::new(Registry::new());

    // Add backends with models
    for i in 0..10 {
        let models = vec![Model {
            id: format!("model-{}", i % 3), // 3 different models
            name: format!("Model {}", i % 3),
            context_length: 8192,
            supports_vision: false,
            supports_tools: true,
            supports_json_mode: true,
            max_output_tokens: Some(4096),
        }];

        let backend = Backend::new(
            format!("backend-{}", i),
            format!("Backend {}", i),
            format!("http://localhost:1143{}", i),
            BackendType::Ollama,
            models,
            DiscoverySource::Static,
            HashMap::new(),
        );

        registry.add_backend(backend).unwrap();
    }

    let mut handles = vec![];

    // Spawn query tasks
    for _ in 0..200 {
        let reg = Arc::clone(&registry);
        let handle = tokio::spawn(async move {
            for i in 0..100 {
                let backends = reg.get_backends_for_model(&format!("model-{}", i % 3));
                // Each model should have some backends
                assert!(!backends.is_empty());
            }
        });
        handles.push(handle);
    }

    // All should complete within 5 seconds
    let result = timeout(Duration::from_secs(5), async {
        for handle in handles {
            handle.await.unwrap();
        }
    })
    .await;

    assert!(
        result.is_ok(),
        "Concurrent model queries should be consistent"
    );
}

// mDNS discovery extension tests

fn create_test_backend(url: &str) -> Backend {
    Backend::new(
        uuid::Uuid::new_v4().to_string(),
        "Test Backend".to_string(),
        url.to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    )
}

#[test]
fn test_registry_has_backend_url_true() {
    let registry = Registry::new();
    let backend = create_test_backend("http://localhost:11434");
    registry.add_backend(backend).unwrap();
    assert!(registry.has_backend_url("http://localhost:11434"));
}

#[test]
fn test_registry_has_backend_url_false() {
    let registry = Registry::new();
    assert!(!registry.has_backend_url("http://localhost:11434"));
}

#[test]
fn test_registry_has_backend_url_normalized() {
    let registry = Registry::new();
    let backend = create_test_backend("http://localhost:11434/");
    registry.add_backend(backend).unwrap();
    // Should match with or without trailing slash
    assert!(registry.has_backend_url("http://localhost:11434"));
    assert!(registry.has_backend_url("http://localhost:11434/"));
}

#[test]
fn test_registry_set_mdns_instance() {
    let registry = Registry::new();
    let backend = create_test_backend("http://localhost:11434");
    let id = backend.id.clone();
    registry.add_backend(backend).unwrap();
    registry
        .set_mdns_instance(&id, "my-instance._ollama._tcp.local")
        .unwrap();

    let backend = registry.get_backend(&id).unwrap();
    assert_eq!(
        backend.metadata.get("mdns_instance"),
        Some(&"my-instance._ollama._tcp.local".to_string())
    );
}

#[test]
fn test_registry_find_by_mdns_instance_found() {
    let registry = Registry::new();
    let backend = create_test_backend("http://localhost:11434");
    let id = backend.id.clone();
    registry.add_backend(backend).unwrap();
    registry.set_mdns_instance(&id, "test-instance").unwrap();

    let found = registry.find_by_mdns_instance("test-instance");
    assert_eq!(found, Some(id));
}

#[test]
fn test_registry_find_by_mdns_instance_not_found() {
    let registry = Registry::new();
    assert!(registry.find_by_mdns_instance("nonexistent").is_none());
}

// ── Agent registry methods (T023-T026) ──────────────────────────────

#[test]
fn test_add_backend_with_agent_and_get_agent() {
    use crate::agent::types::{AgentCapabilities, AgentProfile, PrivacyZone};
    use crate::agent::{AgentError, HealthStatus, InferenceAgent, ModelCapability, StreamChunk};
    use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};
    use async_trait::async_trait;
    use axum::http::HeaderMap;
    use futures_util::stream::BoxStream;
    use std::sync::Arc;

    struct DummyAgent;

    #[async_trait]
    impl InferenceAgent for DummyAgent {
        fn id(&self) -> &str {
            "dummy"
        }
        fn name(&self) -> &str {
            "Dummy"
        }
        fn profile(&self) -> AgentProfile {
            AgentProfile {
                backend_type: "ollama".to_string(),
                version: None,
                privacy_zone: PrivacyZone::Restricted,
                capabilities: AgentCapabilities::default(),
                capability_tier: Some(1),
            }
        }
        async fn health_check(&self) -> Result<HealthStatus, AgentError> {
            Ok(HealthStatus::Healthy { model_count: 1 })
        }
        async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
            Ok(vec![])
        }
        async fn chat_completion(
            &self,
            _req: ChatCompletionRequest,
            _h: Option<&HeaderMap>,
        ) -> Result<ChatCompletionResponse, AgentError> {
            Err(AgentError::Unsupported("chat_completion"))
        }
        async fn chat_completion_stream(
            &self,
            _req: ChatCompletionRequest,
            _h: Option<&HeaderMap>,
        ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
            Err(AgentError::Unsupported("streaming"))
        }
    }

    let registry = Registry::new();
    let backend = Backend::new(
        "dummy".to_string(),
        "Dummy".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![Model {
            id: "model-a".to_string(),
            name: "model-a".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }],
        DiscoverySource::Static,
        HashMap::new(),
    );

    let agent: Arc<dyn InferenceAgent> = Arc::new(DummyAgent);
    registry.add_backend_with_agent(backend, agent).unwrap();

    // get_agent should return the agent
    let retrieved = registry.get_agent("dummy");
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().id(), "dummy");

    // get_agent for unknown returns None
    assert!(registry.get_agent("nonexistent").is_none());

    // get_all_agents returns all registered agents
    let all = registry.get_all_agents();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id(), "dummy");

    // model index should be populated
    let backends = registry.get_backends_for_model("model-a");
    assert_eq!(backends.len(), 1);
}

#[test]
fn test_add_backend_with_agent_duplicate_error() {
    use crate::agent::types::{AgentCapabilities, AgentProfile, PrivacyZone};
    use crate::agent::{AgentError, HealthStatus, InferenceAgent, ModelCapability, StreamChunk};
    use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};
    use async_trait::async_trait;
    use axum::http::HeaderMap;
    use futures_util::stream::BoxStream;
    use std::sync::Arc;

    struct DummyAgent2;

    #[async_trait]
    impl InferenceAgent for DummyAgent2 {
        fn id(&self) -> &str {
            "dup"
        }
        fn name(&self) -> &str {
            "Dup"
        }
        fn profile(&self) -> AgentProfile {
            AgentProfile {
                backend_type: "ollama".to_string(),
                version: None,
                privacy_zone: PrivacyZone::Restricted,
                capabilities: AgentCapabilities::default(),
                capability_tier: None,
            }
        }
        async fn health_check(&self) -> Result<HealthStatus, AgentError> {
            Ok(HealthStatus::Unhealthy)
        }
        async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
            Ok(vec![])
        }
        async fn chat_completion(
            &self,
            _req: ChatCompletionRequest,
            _h: Option<&HeaderMap>,
        ) -> Result<ChatCompletionResponse, AgentError> {
            Err(AgentError::Unsupported("chat_completion"))
        }
        async fn chat_completion_stream(
            &self,
            _req: ChatCompletionRequest,
            _h: Option<&HeaderMap>,
        ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
            Err(AgentError::Unsupported("streaming"))
        }
    }

    let registry = Registry::new();
    let backend1 = Backend::new(
        "dup".to_string(),
        "Dup".to_string(),
        "http://localhost:1".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    let backend2 = Backend::new(
        "dup".to_string(),
        "Dup2".to_string(),
        "http://localhost:2".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    let agent1: Arc<dyn InferenceAgent> = Arc::new(DummyAgent2);
    let agent2: Arc<dyn InferenceAgent> = Arc::new(DummyAgent2);

    registry.add_backend_with_agent(backend1, agent1).unwrap();
    let result = registry.add_backend_with_agent(backend2, agent2);
    assert!(matches!(result, Err(RegistryError::DuplicateBackend(_))));
}

#[test]
fn test_remove_backend_also_removes_agent() {
    use crate::agent::types::{AgentCapabilities, AgentProfile, PrivacyZone};
    use crate::agent::{AgentError, HealthStatus, InferenceAgent, ModelCapability, StreamChunk};
    use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};
    use async_trait::async_trait;
    use axum::http::HeaderMap;
    use futures_util::stream::BoxStream;
    use std::sync::Arc;

    struct DummyAgent3;

    #[async_trait]
    impl InferenceAgent for DummyAgent3 {
        fn id(&self) -> &str {
            "rm-agent"
        }
        fn name(&self) -> &str {
            "RM"
        }
        fn profile(&self) -> AgentProfile {
            AgentProfile {
                backend_type: "ollama".to_string(),
                version: None,
                privacy_zone: PrivacyZone::Restricted,
                capabilities: AgentCapabilities::default(),
                capability_tier: None,
            }
        }
        async fn health_check(&self) -> Result<HealthStatus, AgentError> {
            Ok(HealthStatus::Unhealthy)
        }
        async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
            Ok(vec![])
        }
        async fn chat_completion(
            &self,
            _req: ChatCompletionRequest,
            _h: Option<&HeaderMap>,
        ) -> Result<ChatCompletionResponse, AgentError> {
            Err(AgentError::Unsupported("chat_completion"))
        }
        async fn chat_completion_stream(
            &self,
            _req: ChatCompletionRequest,
            _h: Option<&HeaderMap>,
        ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
            Err(AgentError::Unsupported("streaming"))
        }
    }

    let registry = Registry::new();
    let backend = Backend::new(
        "rm-agent".to_string(),
        "RM".to_string(),
        "http://localhost:1".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    let agent: Arc<dyn InferenceAgent> = Arc::new(DummyAgent3);
    registry.add_backend_with_agent(backend, agent).unwrap();

    assert!(registry.get_agent("rm-agent").is_some());
    registry.remove_backend("rm-agent").unwrap();
    assert!(registry.get_agent("rm-agent").is_none());
    assert_eq!(registry.get_all_agents().len(), 0);
}

#[test]
fn test_increment_total_requests() {
    let registry = Registry::new();
    let backend = Backend::new(
        "total-req".to_string(),
        "TotalReq".to_string(),
        "http://localhost:1".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    registry.add_backend(backend).unwrap();

    let v1 = registry.increment_total_requests("total-req").unwrap();
    assert_eq!(v1, 1);
    let v2 = registry.increment_total_requests("total-req").unwrap();
    assert_eq!(v2, 2);

    // Nonexistent backend
    let err = registry.increment_total_requests("nonexistent");
    assert!(matches!(err, Err(RegistryError::BackendNotFound(_))));
}

#[test]
fn test_remove_backend_cleans_up_model_index_completely() {
    let registry = Registry::new();
    let backend = Backend::new(
        "rm-model-idx".to_string(),
        "RM".to_string(),
        "http://localhost:1".to_string(),
        BackendType::Ollama,
        vec![Model {
            id: "unique-model".to_string(),
            name: "unique-model".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }],
        DiscoverySource::Static,
        HashMap::new(),
    );
    registry.add_backend(backend).unwrap();

    // Model should be in index
    assert_eq!(registry.get_backends_for_model("unique-model").len(), 1);
    assert_eq!(registry.model_count(), 1);

    // Remove backend — model index should be cleaned up
    registry.remove_backend("rm-model-idx").unwrap();
    assert_eq!(registry.get_backends_for_model("unique-model").len(), 0);
    assert_eq!(registry.model_count(), 0);
}
