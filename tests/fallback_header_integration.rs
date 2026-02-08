//! T10: Integration tests for X-Nexus-Fallback-Model header
//!
//! These tests verify that the fallback header is correctly added to HTTP responses
//! when a fallback model is used, and absent when the primary model is used.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use nexus::api::{create_router, AppState};
use nexus::config::NexusConfig;
use nexus::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model, Registry};
use std::collections::HashMap;
use std::sync::Arc;
use tower::Service;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Create a test app with fallback configuration and mock backends
async fn create_test_app_with_fallback(
    primary_mock: &MockServer,
    fallback_mock: &MockServer,
) -> (axum::Router, Arc<Registry>) {
    let registry = Arc::new(Registry::new());
    let mut config = NexusConfig::default();

    // Configure fallback: primary → fallback
    config.routing.fallbacks.insert(
        "primary-model".to_string(),
        vec!["fallback-model".to_string()],
    );
    let config = Arc::new(config);

    // Primary backend (will be unavailable in fallback tests)
    let primary_backend = Backend::new(
        "primary-backend".to_string(),
        "Primary Backend".to_string(),
        primary_mock.uri(),
        BackendType::Generic,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    registry.add_backend(primary_backend).unwrap();
    let _ = registry.update_models(
        "primary-backend",
        vec![Model {
            id: "primary-model".to_string(),
            name: "Primary Model".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }],
    );

    // Fallback backend
    let fallback_backend = Backend::new(
        "fallback-backend".to_string(),
        "Fallback Backend".to_string(),
        fallback_mock.uri(),
        BackendType::Generic,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    registry.add_backend(fallback_backend).unwrap();
    let _ = registry.update_status("fallback-backend", BackendStatus::Healthy, None);
    let _ = registry.update_models(
        "fallback-backend",
        vec![Model {
            id: "fallback-model".to_string(),
            name: "Fallback Model".to_string(),
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

#[tokio::test]
async fn api_response_includes_fallback_header() {
    // Setup: primary unavailable, only fallback available
    let primary_mock = MockServer::start().await;
    let fallback_mock = MockServer::start().await;

    // Mock fallback backend response
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "fallback-model",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello from fallback!"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 10,
                "total_tokens": 20
            }
        })))
        .mount(&fallback_mock)
        .await;

    let (mut app, registry) = create_test_app_with_fallback(&primary_mock, &fallback_mock).await;

    // Make primary backend unhealthy (so fallback is used)
    let _ = registry.update_status(
        "primary-backend",
        BackendStatus::Unhealthy,
        Some("down".to_string()),
    );

    // Request using primary model
    let request_body = serde_json::json!({
        "model": "primary-model",
        "messages": [{
            "role": "user",
            "content": "Hello"
        }]
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Verify response is successful
    assert_eq!(response.status(), StatusCode::OK);

    // Verify X-Nexus-Fallback-Model header is present
    let headers = response.headers();
    assert!(
        headers.contains_key("x-nexus-fallback-model"),
        "Expected x-nexus-fallback-model header to be present"
    );
    assert_eq!(
        headers
            .get("x-nexus-fallback-model")
            .unwrap()
            .to_str()
            .unwrap(),
        "fallback-model"
    );
}

#[tokio::test]
async fn response_no_fallback_header_when_primary_used() {
    // Setup: both primary and fallback available
    let primary_mock = MockServer::start().await;
    let fallback_mock = MockServer::start().await;

    // Mock primary backend response
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "chatcmpl-456",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "primary-model",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello from primary!"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 10,
                "total_tokens": 20
            }
        })))
        .mount(&primary_mock)
        .await;

    let (mut app, registry) = create_test_app_with_fallback(&primary_mock, &fallback_mock).await;

    // Make primary backend healthy
    let _ = registry.update_status("primary-backend", BackendStatus::Healthy, None);

    // Request using primary model
    let request_body = serde_json::json!({
        "model": "primary-model",
        "messages": [{
            "role": "user",
            "content": "Hello"
        }]
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Verify response is successful
    assert_eq!(response.status(), StatusCode::OK);

    // Verify X-Nexus-Fallback-Model header is NOT present
    let headers = response.headers();
    assert!(
        !headers.contains_key("x-nexus-fallback-model"),
        "Expected x-nexus-fallback-model header to be absent when primary is used"
    );
}

#[tokio::test]
async fn streaming_response_includes_fallback_header() {
    // Setup: primary unavailable, only fallback available
    let primary_mock = MockServer::start().await;
    let fallback_mock = MockServer::start().await;

    // Mock fallback backend streaming response
    let sse_data = "data: {\"id\":\"chatcmpl-789\",\"object\":\"chat.completion.chunk\",\"created\":1677652288,\"model\":\"fallback-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hello\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n";

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&fallback_mock)
        .await;

    let (mut app, registry) = create_test_app_with_fallback(&primary_mock, &fallback_mock).await;

    // Make primary backend unhealthy
    let _ = registry.update_status(
        "primary-backend",
        BackendStatus::Unhealthy,
        Some("down".to_string()),
    );

    // Request streaming with primary model
    let request_body = serde_json::json!({
        "model": "primary-model",
        "messages": [{
            "role": "user",
            "content": "Hello"
        }],
        "stream": true
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Verify response is successful
    assert_eq!(response.status(), StatusCode::OK);

    // Verify X-Nexus-Fallback-Model header is present in streaming response
    let headers = response.headers();
    assert!(
        headers.contains_key("x-nexus-fallback-model"),
        "Expected x-nexus-fallback-model header to be present in streaming response"
    );
    assert_eq!(
        headers
            .get("x-nexus-fallback-model")
            .unwrap()
            .to_str()
            .unwrap(),
        "fallback-model"
    );
}
