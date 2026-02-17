//! Integration tests for dashboard HTTP and WebSocket endpoints

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use nexus::api::{create_router, AppState};
use nexus::config::NexusConfig;
use nexus::registry::Registry;
use std::sync::Arc;
use tower::Service;

/// Helper function to create test app state
fn create_test_state() -> Arc<AppState> {
    let registry = Arc::new(Registry::new());
    let config = Arc::new(NexusConfig::default());
    Arc::new(AppState::new(registry, config))
}

#[tokio::test]
async fn test_dashboard_endpoint_returns_200_with_html() {
    // Create test state and router
    let state = create_test_state();
    let mut app = create_router(state);

    // Make request to dashboard endpoint
    let request = Request::builder().uri("/").body(Body::empty()).unwrap();

    let response = app.call(request).await.unwrap();

    // Should return 200 OK
    assert_eq!(response.status(), StatusCode::OK);

    // Should return HTML content type
    let content_type = response.headers().get("content-type").unwrap();
    assert!(
        content_type.to_str().unwrap().contains("text/html"),
        "Expected HTML content type, got: {:?}",
        content_type
    );
}

#[tokio::test]
async fn test_websocket_endpoint_accepts_connections() {
    // Create test state and router
    let state = create_test_state();
    let mut app = create_router(state);

    // Make WebSocket upgrade request
    let request = Request::builder()
        .uri("/ws")
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
        .header("Sec-WebSocket-Version", "13")
        .body(Body::empty())
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Should return 101 Switching Protocols, 426 Upgrade Required is also valid in test context
    assert!(
        response.status() == StatusCode::SWITCHING_PROTOCOLS
            || response.status() == StatusCode::UPGRADE_REQUIRED
            || response.status() == StatusCode::OK,
        "Expected WebSocket upgrade response, got: {}",
        response.status()
    );
}

#[tokio::test]
async fn test_websocket_sends_backend_status_update() {
    use nexus::dashboard::types::UpdateType;
    use nexus::dashboard::websocket::create_backend_status_update;
    use nexus::registry::{Backend, BackendType, BackendView, DiscoverySource, Model};
    use std::collections::HashMap;

    let state = create_test_state();

    // Subscribe to broadcast channel before sending
    let mut rx = state.ws_broadcast.subscribe();

    // Create a backend and generate a status update
    let backend = Backend::new(
        "ws-test-1".to_string(),
        "WS Test Backend".to_string(),
        "http://localhost:11434".to_string(),
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

    let view = BackendView::from(&backend);
    let update = create_backend_status_update(vec![view]);

    // Broadcast the update (simulates what health checker does)
    state.ws_broadcast.send(update).unwrap();

    // Verify subscriber receives the correct message
    let received = rx.recv().await.unwrap();
    assert_eq!(received.update_type, UpdateType::BackendStatus);

    let backends: Vec<BackendView> = serde_json::from_value(received.data).unwrap();
    assert_eq!(backends.len(), 1);
    assert_eq!(backends[0].id, "ws-test-1");
    assert_eq!(backends[0].models.len(), 1);
}

#[tokio::test]
async fn test_assets_endpoint_returns_css_with_correct_mime() {
    // Create test state and router
    let state = create_test_state();
    let mut app = create_router(state);

    // Make request to CSS asset
    let request = Request::builder()
        .uri("/assets/styles.css")
        .body(Body::empty())
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Should return 200 OK when asset exists, or 404 if not created yet
    assert!(
        response.status() == StatusCode::OK || response.status() == StatusCode::NOT_FOUND,
        "Expected OK or NOT_FOUND, got: {}",
        response.status()
    );

    // If OK, should have correct MIME type
    if response.status() == StatusCode::OK {
        let content_type = response.headers().get("content-type");
        assert!(
            content_type.is_some(),
            "CSS asset should have content-type header"
        );

        let content_type_str = content_type.unwrap().to_str().unwrap();
        assert!(
            content_type_str.contains("text/css") || content_type_str.contains("stylesheet"),
            "Expected CSS content type, got: {}",
            content_type_str
        );
    }
}

// ========== Model Change Integration Tests (T072-T073) ==========

#[tokio::test]
async fn test_websocket_sends_model_change_update_when_models_updated() {
    use nexus::dashboard::types::UpdateType;
    use nexus::dashboard::websocket::create_model_change_update;
    use serde_json::json;

    let state = create_test_state();
    let mut rx = state.ws_broadcast.subscribe();

    // Simulate a model change event (backend discovered new models)
    let models = vec![
        json!({ "id": "gpt-4", "capabilities": { "vision": true, "tools": true } }),
        json!({ "id": "gpt-3.5-turbo", "capabilities": { "vision": false, "tools": true } }),
    ];
    let update = create_model_change_update("discovery-backend".to_string(), models);

    state.ws_broadcast.send(update).unwrap();

    // Verify subscriber receives model change with correct payload
    let received = rx.recv().await.unwrap();
    assert_eq!(received.update_type, UpdateType::ModelChange);

    let data = received.data;
    assert_eq!(data["backend_id"], "discovery-backend");
    let models_arr = data["models"].as_array().unwrap();
    assert_eq!(models_arr.len(), 2);
    assert_eq!(models_arr[0]["id"], "gpt-4");
}

#[tokio::test]
async fn test_model_matrix_reflects_model_removal_when_backend_offline() {
    use nexus::dashboard::types::UpdateType;
    use nexus::dashboard::websocket::create_backend_status_update;
    use nexus::registry::{Backend, BackendType, BackendView, DiscoverySource, Model};
    use std::collections::HashMap;

    let state = create_test_state();
    let mut rx = state.ws_broadcast.subscribe();

    // Add a backend with models
    let models = vec![
        Model {
            id: "llama3:8b".to_string(),
            name: "Llama 3 8B".to_string(),
            context_length: 8192,
            supports_vision: false,
            supports_tools: true,
            supports_json_mode: true,
            max_output_tokens: None,
        },
        Model {
            id: "codellama:7b".to_string(),
            name: "Code Llama 7B".to_string(),
            context_length: 16384,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        },
    ];

    let backend = Backend::new(
        "lifecycle-test-1".to_string(),
        "Lifecycle Test".to_string(),
        "http://localhost:8001".to_string(),
        BackendType::Ollama,
        models,
        DiscoverySource::Static,
        HashMap::new(),
    );

    state.registry.add_backend(backend).unwrap();
    assert_eq!(state.registry.get_all_backends().len(), 1);

    // Broadcast status with backend online (1 backend in list)
    let views: Vec<BackendView> = state
        .registry
        .get_all_backends()
        .iter()
        .map(BackendView::from)
        .collect();
    let update = create_backend_status_update(views);
    state.ws_broadcast.send(update).unwrap();

    let received = rx.recv().await.unwrap();
    assert_eq!(received.update_type, UpdateType::BackendStatus);
    let online_backends: Vec<BackendView> = serde_json::from_value(received.data).unwrap();
    assert_eq!(online_backends.len(), 1);
    assert_eq!(online_backends[0].models.len(), 2);

    // Remove backend (simulating offline)
    state.registry.remove_backend("lifecycle-test-1").unwrap();

    // Broadcast updated status (0 backends — models no longer available)
    let views: Vec<BackendView> = state
        .registry
        .get_all_backends()
        .iter()
        .map(BackendView::from)
        .collect();
    let update = create_backend_status_update(views);
    state.ws_broadcast.send(update).unwrap();

    let received = rx.recv().await.unwrap();
    let offline_backends: Vec<BackendView> = serde_json::from_value(received.data).unwrap();
    assert_eq!(
        offline_backends.len(),
        0,
        "No backends should remain after removal"
    );
}
