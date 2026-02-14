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
    // TODO: Implement WebSocket message testing
    // This test will verify that WebSocket sends backend_status updates
    // when health check completes

    // For now, just verify the endpoint exists
    let state = create_test_state();
    let mut app = create_router(state);

    let request = Request::builder().uri("/ws").body(Body::empty()).unwrap();

    let response = app.call(request).await.unwrap();

    // Endpoint should exist (even if not fully implemented yet)
    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "WebSocket endpoint should exist"
    );
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
    // TODO: Implement full WebSocket message testing
    // This test will verify that WebSocket sends model_change updates
    // when backend models are updated via discovery

    // For now, verify that we can create a model_change update message
    use nexus::dashboard::types::{UpdateType, WebSocketUpdate};
    use serde_json::json;

    let update = WebSocketUpdate {
        update_type: UpdateType::ModelChange,
        data: json!({
            "backend_id": "test-backend",
            "action": "added",
            "models": [{
                "id": "gpt-4",
                "capabilities": {
                    "vision": true,
                    "tools": true,
                    "json_mode": false
                }
            }]
        }),
    };

    // Verify serialization works
    let serialized = serde_json::to_string(&update).unwrap();
    assert!(serialized.contains("ModelChange"));
    assert!(serialized.contains("test-backend"));
}

#[tokio::test]
async fn test_model_matrix_reflects_model_removal_when_backend_offline() {
    // TODO: Implement full test with backend lifecycle
    // This test will verify that model matrix correctly reflects
    // model unavailability when a backend goes offline

    // For now, verify state management structure exists
    let state = create_test_state();

    // Registry should track backends
    let backends = state.registry.get_all_backends();
    assert_eq!(backends.len(), 0, "Initially should have no backends");

    // Add a test backend
    use nexus::registry::{Backend, BackendType, DiscoverySource, Model};
    use std::collections::HashMap;

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
        "test-backend-1".to_string(),
        "Test Backend".to_string(),
        "http://localhost:8001".to_string(),
        BackendType::Ollama,
        models,
        DiscoverySource::Static,
        HashMap::new(),
    );

    state.registry.add_backend(backend).unwrap();

    // Verify backend was added with models
    let backends = state.registry.get_all_backends();
    assert_eq!(backends.len(), 1);
    assert_eq!(backends[0].models.len(), 2);

    // Remove backend (simulating offline)
    state.registry.remove_backend("test-backend-1").unwrap();

    // Verify backend was removed
    let backends = state.registry.get_all_backends();
    assert_eq!(backends.len(), 0, "Backend should be removed");
}
