//! Contract tests for model lifecycle API endpoints.
//!
//! Tests the HTTP contract of POST /v1/models/load and related endpoints.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use nexus::agent::factory::create_agent;
use nexus::agent::types::{LifecycleOperation, OperationStatus, OperationType, PrivacyZone};
use nexus::api::{create_router, AppState};
use nexus::config::NexusConfig;
use nexus::registry::{Backend, BackendType, DiscoverySource, Model, Registry};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tower::Service;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// T018: Contract test for POST /v1/models/load success (202 Accepted)
#[tokio::test]
async fn test_load_model_success_returns_202() {
    // Start mock Ollama server
    let mock_server = MockServer::start().await;

    // Mock /api/ps to return sufficient VRAM
    Mock::given(method("GET"))
        .and(path("/api/ps"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": []
        })))
        .mount(&mock_server)
        .await;

    // Mock /api/pull to accept the load request
    Mock::given(method("POST"))
        .and(path("/api/pull"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    // Create test app with backend
    let registry = Arc::new(Registry::new());
    let config = Arc::new(NexusConfig::default());
    let client = Arc::new(reqwest::Client::new());

    let backend = Backend::new(
        "backend-1".to_string(),
        "Test Backend".to_string(),
        mock_server.uri(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    let agent = create_agent(
        "backend-1".to_string(),
        "Test Backend".to_string(),
        mock_server.uri(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    registry.add_backend_with_agent(backend, agent).unwrap();

    let state = Arc::new(AppState::new(registry, config));
    let mut app = create_router(state);

    // Send load request
    let request = Request::builder()
        .method("POST")
        .uri("/v1/models/load")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "model_id": "llama3:8b",
                "backend_id": "backend-1"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Should return 202 Accepted
    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

// T019: Contract test for insufficient VRAM (400 Bad Request)
#[tokio::test]
async fn test_load_model_insufficient_vram_returns_400() {
    // Start mock Ollama server
    let mock_server = MockServer::start().await;

    // Mock /api/ps to return high VRAM usage (insufficient for load)
    Mock::given(method("GET"))
        .and(path("/api/ps"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": [{
                "name": "existing-model:latest",
                "size_vram": 20_000_000_000u64  // 20GB used
            }]
        })))
        .mount(&mock_server)
        .await;

    // Create test app with backend
    let registry = Arc::new(Registry::new());
    let mut config = NexusConfig::default();
    // Set VRAM total to 24GB for testing, with 20% headroom = 4.8GB required free
    config.lifecycle.vram_headroom_percent = 20;
    let config = Arc::new(config);
    let client = Arc::new(reqwest::Client::new());

    let backend = Backend::new(
        "backend-1".to_string(),
        "Test Backend".to_string(),
        mock_server.uri(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    let agent = create_agent(
        "backend-1".to_string(),
        "Test Backend".to_string(),
        mock_server.uri(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    registry.add_backend_with_agent(backend, agent).unwrap();

    let state = Arc::new(AppState::new(registry, config));
    let mut app = create_router(state);

    // Send load request
    let request = Request::builder()
        .method("POST")
        .uri("/v1/models/load")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "model_id": "llama3:70b",
                "backend_id": "backend-1"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Should return 400 or 507 for insufficient resources
    assert!(
        response.status() == StatusCode::BAD_REQUEST
            || response.status() == StatusCode::INSUFFICIENT_STORAGE
    );
}

// T020: Contract test for concurrent load rejection (409 Conflict)
#[tokio::test]
async fn test_load_model_concurrent_load_returns_409() {
    // Start mock Ollama server
    let mock_server = MockServer::start().await;

    // Mock /api/ps to return sufficient VRAM
    Mock::given(method("GET"))
        .and(path("/api/ps"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": []
        })))
        .mount(&mock_server)
        .await;

    // Create test app with backend that has an active operation
    let registry = Arc::new(Registry::new());
    let config = Arc::new(NexusConfig::default());
    let client = Arc::new(reqwest::Client::new());

    let mut backend = Backend::new(
        "backend-1".to_string(),
        "Test Backend".to_string(),
        mock_server.uri(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    // Set current operation to simulate in-progress load
    backend.current_operation = Some(LifecycleOperation {
        operation_id: "op-123".to_string(),
        operation_type: OperationType::Load,
        model_id: "llama3:8b".to_string(),
        source_backend_id: None,
        target_backend_id: "backend-1".to_string(),
        status: OperationStatus::InProgress,
        progress_percent: 50,
        eta_ms: Some(30000),
        initiated_at: chrono::Utc::now(),
        completed_at: None,
        error_details: None,
    });

    let agent = create_agent(
        "backend-1".to_string(),
        "Test Backend".to_string(),
        mock_server.uri(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    registry.add_backend_with_agent(backend, agent).unwrap();

    let state = Arc::new(AppState::new(registry, config));
    let mut app = create_router(state);

    // Send load request while another is in progress
    let request = Request::builder()
        .method("POST")
        .uri("/v1/models/load")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "model_id": "llama3:70b",
                "backend_id": "backend-1"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Should return 409 Conflict
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

// T022: Integration test for HealthStatus::Loading blocking routing
#[tokio::test]
async fn test_loading_backend_blocked_from_routing() {
    use chrono::Utc;
    use nexus::routing::{RequestRequirements, Router, RoutingStrategy, ScoringWeights};

    // Create test registry
    let registry = Arc::new(Registry::new());
    let _config = Arc::new(NexusConfig::default());
    let client = Arc::new(reqwest::Client::new());

    // Backend 1: Has active load operation (should be blocked)
    let mut backend_loading = Backend::new(
        "backend-loading".to_string(),
        "Loading Backend".to_string(),
        "http://loading:11434".to_string(),
        BackendType::Ollama,
        vec![nexus::registry::Model {
            id: "llama3:8b".to_string(),
            name: "llama3:8b".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }],
        DiscoverySource::Static,
        HashMap::new(),
    );

    // Mark as healthy so it's considered for routing
    backend_loading.status = nexus::registry::BackendStatus::Healthy;

    backend_loading.current_operation = Some(LifecycleOperation {
        operation_id: "op-456".to_string(),
        operation_type: OperationType::Load,
        model_id: "llama3:70b".to_string(),
        source_backend_id: None,
        target_backend_id: "backend-loading".to_string(),
        status: OperationStatus::InProgress,
        progress_percent: 45,
        eta_ms: Some(60000),
        initiated_at: Utc::now(),
        completed_at: None,
        error_details: None,
    });

    let agent_loading = create_agent(
        "backend-loading".to_string(),
        "Loading Backend".to_string(),
        "http://loading:11434".to_string(),
        BackendType::Ollama,
        client.clone(),
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    registry
        .add_backend_with_agent(backend_loading, agent_loading)
        .unwrap();

    // Backend 2: Idle and ready (should be selected)
    let mut backend_idle = Backend::new(
        "backend-idle".to_string(),
        "Idle Backend".to_string(),
        "http://idle:11434".to_string(),
        BackendType::Ollama,
        vec![nexus::registry::Model {
            id: "llama3:8b".to_string(),
            name: "llama3:8b".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }],
        DiscoverySource::Static,
        HashMap::new(),
    );

    // Mark as healthy so it's considered for routing
    backend_idle.status = nexus::registry::BackendStatus::Healthy;

    let agent_idle = create_agent(
        "backend-idle".to_string(),
        "Idle Backend".to_string(),
        "http://idle:11434".to_string(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    registry
        .add_backend_with_agent(backend_idle, agent_idle)
        .unwrap();

    // Create router
    let router = Router::new(
        Arc::clone(&registry),
        RoutingStrategy::RoundRobin,
        ScoringWeights::default(),
    );

    // Route a request for llama3:8b
    let requirements = RequestRequirements {
        model: "llama3:8b".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
        prefers_streaming: false,
    };

    let result = router.select_backend(&requirements, None);

    // Should succeed routing
    if let Err(e) = &result {
        panic!("Routing failed: {:?}", e);
    }
    assert!(
        result.is_ok(),
        "Routing should succeed when idle backend available"
    );

    let routing_result = result.unwrap();

    // Should route to the idle backend, NOT the loading one
    assert_eq!(
        routing_result.backend.id, "backend-idle",
        "Should route to idle backend, not loading backend"
    );
}

// ==================== Phase 4: User Story 2 - Model Migration ====================

// T038: Contract test for migration coordination
#[tokio::test]
async fn test_migrate_model_initiates_coordination() {
    // Start mock Ollama servers for source and target
    let mock_source = MockServer::start().await;
    let mock_target = MockServer::start().await;

    // Mock source: has model loaded, resource_usage shows model present
    Mock::given(method("GET"))
        .and(path("/api/ps"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": [{
                "name": "llama3:8b",
                "size_vram": 8_000_000_000u64
            }]
        })))
        .mount(&mock_source)
        .await;

    // Mock target: has capacity
    Mock::given(method("GET"))
        .and(path("/api/ps"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": []
        })))
        .mount(&mock_target)
        .await;

    // Mock target: accepts load request
    Mock::given(method("POST"))
        .and(path("/api/pull"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_target)
        .await;

    // Create test app with both backends
    let registry = Arc::new(Registry::new());
    let config = Arc::new(NexusConfig::default());
    let client = Arc::new(reqwest::Client::new());

    // Source backend with model loaded
    let mut backend_source = Backend::new(
        "backend-source".to_string(),
        "Source Backend".to_string(),
        mock_source.uri(),
        BackendType::Ollama,
        vec![nexus::registry::Model {
            id: "llama3:8b".to_string(),
            name: "llama3:8b".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }],
        DiscoverySource::Static,
        HashMap::new(),
    );
    backend_source.status = nexus::registry::BackendStatus::Healthy;

    let agent_source = create_agent(
        "backend-source".to_string(),
        "Source Backend".to_string(),
        mock_source.uri(),
        BackendType::Ollama,
        client.clone(),
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    registry
        .add_backend_with_agent(backend_source, agent_source)
        .unwrap();

    // Target backend ready for load
    let mut backend_target = Backend::new(
        "backend-target".to_string(),
        "Target Backend".to_string(),
        mock_target.uri(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    backend_target.status = nexus::registry::BackendStatus::Healthy;

    let agent_target = create_agent(
        "backend-target".to_string(),
        "Target Backend".to_string(),
        mock_target.uri(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    registry
        .add_backend_with_agent(backend_target, agent_target)
        .unwrap();

    let state = Arc::new(AppState::new(registry, config));
    let mut app = create_router(state.clone());

    // Send migration request
    let request = Request::builder()
        .method("POST")
        .uri("/v1/models/migrate")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "model_id": "llama3:8b",
                "source_backend_id": "backend-source",
                "target_backend_id": "backend-target"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Should return 202 Accepted
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    // Verify source backend still has the model and is marked as Migrating
    let source_op = state.registry.get_operation("backend-source").unwrap();
    assert!(
        source_op.is_some(),
        "Source should have migration operation"
    );
    let source_op = source_op.unwrap();
    assert_eq!(source_op.operation_type, OperationType::Migrate);
    assert_eq!(source_op.status, OperationStatus::InProgress);

    // Verify target backend has load operation
    let target_op = state.registry.get_operation("backend-target").unwrap();
    assert!(target_op.is_some(), "Target should have load operation");
    let target_op = target_op.unwrap();
    assert_eq!(target_op.operation_type, OperationType::Load);
    assert_eq!(target_op.status, OperationStatus::InProgress);
}

// T039: Integration test for migration without request drops
#[tokio::test]
async fn test_migration_source_continues_serving() {
    use nexus::routing::{RequestRequirements, Router, RoutingStrategy, ScoringWeights};

    // Create test registry
    let registry = Arc::new(Registry::new());
    let _config = Arc::new(NexusConfig::default());
    let client = Arc::new(reqwest::Client::new());

    // Source backend: serving model, has migrate operation in progress
    let mut backend_source = Backend::new(
        "backend-source".to_string(),
        "Source Backend".to_string(),
        "http://source:11434".to_string(),
        BackendType::Ollama,
        vec![nexus::registry::Model {
            id: "llama3:8b".to_string(),
            name: "llama3:8b".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }],
        DiscoverySource::Static,
        HashMap::new(),
    );
    backend_source.status = nexus::registry::BackendStatus::Healthy;
    backend_source.current_operation = Some(LifecycleOperation {
        operation_id: "op-migrate-123".to_string(),
        operation_type: OperationType::Migrate,
        model_id: "llama3:8b".to_string(),
        source_backend_id: Some("backend-source".to_string()),
        target_backend_id: "backend-target".to_string(),
        status: OperationStatus::InProgress,
        progress_percent: 0,
        eta_ms: None,
        initiated_at: chrono::Utc::now(),
        completed_at: None,
        error_details: None,
    });

    let agent_source = create_agent(
        "backend-source".to_string(),
        "Source Backend".to_string(),
        "http://source:11434".to_string(),
        BackendType::Ollama,
        client.clone(),
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    registry
        .add_backend_with_agent(backend_source, agent_source)
        .unwrap();

    // Target backend: loading model (should NOT be selected for routing yet)
    let mut backend_target = Backend::new(
        "backend-target".to_string(),
        "Target Backend".to_string(),
        "http://target:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    backend_target.status = nexus::registry::BackendStatus::Healthy;
    backend_target.current_operation = Some(LifecycleOperation {
        operation_id: "op-load-456".to_string(),
        operation_type: OperationType::Load,
        model_id: "llama3:8b".to_string(),
        source_backend_id: None,
        target_backend_id: "backend-target".to_string(),
        status: OperationStatus::InProgress,
        progress_percent: 50,
        eta_ms: Some(30000),
        initiated_at: chrono::Utc::now(),
        completed_at: None,
        error_details: None,
    });

    let agent_target = create_agent(
        "backend-target".to_string(),
        "Target Backend".to_string(),
        "http://target:11434".to_string(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    registry
        .add_backend_with_agent(backend_target, agent_target)
        .unwrap();

    // Create router
    let router = Router::new(
        Arc::clone(&registry),
        RoutingStrategy::RoundRobin,
        ScoringWeights::default(),
    );

    // Route a request for llama3:8b
    let requirements = RequestRequirements {
        model: "llama3:8b".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
        prefers_streaming: false,
    };

    let result = router.select_backend(&requirements, None);

    // Should succeed routing to source backend (even though it's migrating)
    assert!(result.is_ok(), "Routing should succeed during migration");

    let routing_result = result.unwrap();

    // Should route to source backend (which is migrating but still serving)
    assert_eq!(
        routing_result.backend.id, "backend-source",
        "Should route to source backend during migration, NOT target which is loading"
    );
}

// T040: Integration test for traffic shift after migration completes
#[tokio::test]
async fn test_traffic_shifts_after_target_loads() {
    use nexus::routing::{RequestRequirements, Router, RoutingStrategy, ScoringWeights};

    // Create test registry
    let registry = Arc::new(Registry::new());
    let _config = Arc::new(NexusConfig::default());
    let client = Arc::new(reqwest::Client::new());

    // Source backend: model loaded, migration completed (operation cleared or Completed)
    let mut backend_source = Backend::new(
        "backend-source".to_string(),
        "Source Backend".to_string(),
        "http://source:11434".to_string(),
        BackendType::Ollama,
        vec![], // Model removed after unload
        DiscoverySource::Static,
        HashMap::new(),
    );
    backend_source.status = nexus::registry::BackendStatus::Healthy;
    // No current operation (migration complete and cleared)

    let agent_source = create_agent(
        "backend-source".to_string(),
        "Source Backend".to_string(),
        "http://source:11434".to_string(),
        BackendType::Ollama,
        client.clone(),
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    registry
        .add_backend_with_agent(backend_source, agent_source)
        .unwrap();

    // Target backend: model now loaded and ready
    let mut backend_target = Backend::new(
        "backend-target".to_string(),
        "Target Backend".to_string(),
        "http://target:11434".to_string(),
        BackendType::Ollama,
        vec![nexus::registry::Model {
            id: "llama3:8b".to_string(),
            name: "llama3:8b".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }],
        DiscoverySource::Static,
        HashMap::new(),
    );
    backend_target.status = nexus::registry::BackendStatus::Healthy;
    // No operation (load completed)

    let agent_target = create_agent(
        "backend-target".to_string(),
        "Target Backend".to_string(),
        "http://target:11434".to_string(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    registry
        .add_backend_with_agent(backend_target, agent_target)
        .unwrap();

    // Create router
    let router = Router::new(
        Arc::clone(&registry),
        RoutingStrategy::RoundRobin,
        ScoringWeights::default(),
    );

    // Route a request for llama3:8b
    let requirements = RequestRequirements {
        model: "llama3:8b".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
        prefers_streaming: false,
    };

    let result = router.select_backend(&requirements, None);

    // Should succeed routing
    assert!(
        result.is_ok(),
        "Routing should succeed after migration completes"
    );

    let routing_result = result.unwrap();

    // Should route to target backend (which now has the model)
    assert_eq!(
        routing_result.backend.id, "backend-target",
        "Should route to target backend after migration completes"
    );
}

// T041: Integration test for migration rollback on target failure
#[tokio::test]
async fn test_migration_rollback_on_failure() {
    // Start mock Ollama servers
    let mock_source = MockServer::start().await;
    let mock_target = MockServer::start().await;

    // Mock source: has model loaded
    Mock::given(method("GET"))
        .and(path("/api/ps"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": [{
                "name": "llama3:8b",
                "size_vram": 8_000_000_000u64
            }]
        })))
        .mount(&mock_source)
        .await;

    // Mock target: insufficient capacity
    Mock::given(method("GET"))
        .and(path("/api/ps"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": []
        })))
        .mount(&mock_target)
        .await;

    // Mock target: load fails (return error)
    Mock::given(method("POST"))
        .and(path("/api/pull"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "error": "insufficient VRAM"
        })))
        .mount(&mock_target)
        .await;

    // Create test app with both backends
    let registry = Arc::new(Registry::new());
    let config = Arc::new(NexusConfig::default());
    let client = Arc::new(reqwest::Client::new());

    // Source backend
    let mut backend_source = Backend::new(
        "backend-source".to_string(),
        "Source Backend".to_string(),
        mock_source.uri(),
        BackendType::Ollama,
        vec![nexus::registry::Model {
            id: "llama3:8b".to_string(),
            name: "llama3:8b".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }],
        DiscoverySource::Static,
        HashMap::new(),
    );
    backend_source.status = nexus::registry::BackendStatus::Healthy;

    let agent_source = create_agent(
        "backend-source".to_string(),
        "Source Backend".to_string(),
        mock_source.uri(),
        BackendType::Ollama,
        client.clone(),
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    registry
        .add_backend_with_agent(backend_source, agent_source)
        .unwrap();

    // Target backend
    let mut backend_target = Backend::new(
        "backend-target".to_string(),
        "Target Backend".to_string(),
        mock_target.uri(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    backend_target.status = nexus::registry::BackendStatus::Healthy;

    let agent_target = create_agent(
        "backend-target".to_string(),
        "Target Backend".to_string(),
        mock_target.uri(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    registry
        .add_backend_with_agent(backend_target, agent_target)
        .unwrap();

    let state = Arc::new(AppState::new(registry, config));
    let mut app = create_router(state.clone());

    // Send migration request
    let request = Request::builder()
        .method("POST")
        .uri("/v1/models/migrate")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "model_id": "llama3:8b",
                "source_backend_id": "backend-source",
                "target_backend_id": "backend-target"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Migration initiation fails because target load fails
    // Should return error status (400, 502, or 507)
    assert!(
        response.status() == StatusCode::BAD_REQUEST
            || response.status() == StatusCode::BAD_GATEWAY
            || response.status() == StatusCode::INSUFFICIENT_STORAGE,
        "Migration should fail when target cannot load model"
    );

    // Verify source backend still healthy and serving
    let source_status = state.registry.get_backend("backend-source").unwrap();
    assert_eq!(
        source_status.status,
        nexus::registry::BackendStatus::Healthy,
        "Source should remain healthy after migration failure"
    );

    // Verify source still has the model
    assert!(
        source_status.models.iter().any(|m| m.id == "llama3:8b"),
        "Source should still have model after migration failure"
    );
}

// T049: Contract test for DELETE /v1/models/{id} success (200 OK)
#[tokio::test]
async fn test_unload_model_success_returns_200() {
    // Start mock Ollama server
    let mock_server = MockServer::start().await;

    // Mock /api/ps to return model loaded
    Mock::given(method("GET"))
        .and(path("/api/ps"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": [{
                "name": "llama3:8b",
                "size_vram": 8_000_000_000u64
            }]
        })))
        .mount(&mock_server)
        .await;

    // Mock /api/generate with keep_alive=0 to accept the unload request
    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    // Create test app with backend
    let registry = Arc::new(Registry::new());
    let config = Arc::new(NexusConfig::default());
    let client = Arc::new(reqwest::Client::new());

    let mut backend = Backend::new(
        "backend-1".to_string(),
        "Test Backend".to_string(),
        mock_server.uri(),
        BackendType::Ollama,
        vec![Model {
            id: "llama3:8b".to_string(),
            name: "Llama 3 8B".to_string(),
            context_length: 8192,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }],
        DiscoverySource::Static,
        HashMap::new(),
    );
    backend.status = nexus::registry::BackendStatus::Healthy;
    // No pending requests (default is 0)

    let agent = create_agent(
        "backend-1".to_string(),
        "Test Backend".to_string(),
        mock_server.uri(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    registry.add_backend_with_agent(backend, agent).unwrap();

    let state = Arc::new(AppState::new(registry, config));
    let mut app = create_router(state.clone());

    // Send unload request
    let request = Request::builder()
        .method("DELETE")
        .uri("/v1/models/llama3:8b?backend_id=backend-1")
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Should return 200 OK
    assert_eq!(response.status(), StatusCode::OK);

    // Verify model was removed from backend
    let backend_after = state.registry.get_backend("backend-1").unwrap();
    assert!(
        !backend_after.models.iter().any(|m| m.id == "llama3:8b"),
        "Model should be removed from backend after unload"
    );
}

// T050: Contract test for DELETE with active requests (409 Conflict)
#[tokio::test]
async fn test_unload_model_with_active_requests_returns_409() {
    // Start mock Ollama server
    let mock_server = MockServer::start().await;

    // Mock /api/ps to return model loaded
    Mock::given(method("GET"))
        .and(path("/api/ps"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": [{
                "name": "llama3:8b",
                "size_vram": 8_000_000_000u64
            }]
        })))
        .mount(&mock_server)
        .await;

    // Create test app with backend
    let registry = Arc::new(Registry::new());
    let config = Arc::new(NexusConfig::default());
    let client = Arc::new(reqwest::Client::new());

    let mut backend = Backend::new(
        "backend-1".to_string(),
        "Test Backend".to_string(),
        mock_server.uri(),
        BackendType::Ollama,
        vec![Model {
            id: "llama3:8b".to_string(),
            name: "Llama 3 8B".to_string(),
            context_length: 8192,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }],
        DiscoverySource::Static,
        HashMap::new(),
    );
    backend.status = nexus::registry::BackendStatus::Healthy;
    // Simulate active requests
    use std::sync::atomic::Ordering;
    backend.pending_requests.store(2, Ordering::SeqCst);

    let agent = create_agent(
        "backend-1".to_string(),
        "Test Backend".to_string(),
        mock_server.uri(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    registry.add_backend_with_agent(backend, agent).unwrap();

    let state = Arc::new(AppState::new(registry, config));
    let mut app = create_router(state.clone());

    // Send unload request
    let request = Request::builder()
        .method("DELETE")
        .uri("/v1/models/llama3:8b?backend_id=backend-1")
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Should return 409 Conflict
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // Verify model was NOT removed from backend
    let backend_after = state.registry.get_backend("backend-1").unwrap();
    assert!(
        backend_after.models.iter().any(|m| m.id == "llama3:8b"),
        "Model should still be present on backend when active requests exist"
    );
}
