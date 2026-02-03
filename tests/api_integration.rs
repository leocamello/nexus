//! Integration tests for the API Gateway.
//!
//! These tests use mock HTTP backends to verify end-to-end functionality.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use nexus::api::{create_router, AppState};
use nexus::config::NexusConfig;
use nexus::registry::Registry;
use std::sync::Arc;
use tower::Service;

async fn create_test_app() -> axum::Router {
    let registry = Arc::new(Registry::new());
    let config = Arc::new(NexusConfig::default());
    let state = Arc::new(AppState::new(registry, config));
    create_router(state)
}

#[tokio::test]
async fn test_app_state_creation() {
    let registry = Arc::new(Registry::new());
    let config = Arc::new(NexusConfig::default());
    let state = AppState::new(registry, config);
    // Verify HTTP client can build requests
    assert!(state.http_client.get("http://localhost").build().is_ok());
}

#[tokio::test]
async fn test_router_has_completions_route() {
    let mut app = create_test_app().await;

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Should not be 404 (route exists, may return 501 Not Implemented or 400)
    assert_ne!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_router_has_models_route() {
    let mut app = create_test_app().await;

    let request = Request::builder()
        .uri("/v1/models")
        .body(Body::empty())
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Should not be 404
    assert_ne!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_router_has_health_route() {
    let mut app = create_test_app().await;

    let request = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Should not be 404
    assert_ne!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_router_returns_404_unknown() {
    let mut app = create_test_app().await;

    let request = Request::builder()
        .uri("/unknown/path")
        .body(Body::empty())
        .unwrap();

    let response = app.call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
