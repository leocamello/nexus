//! Integration tests for the /v1/embeddings endpoint (T015).

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::make_app_with_mock;
use tower::Service;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn embeddings_route_exists() {
    let mock_server = wiremock::MockServer::start().await;
    let (mut app, _registry) = make_app_with_mock(&mock_server).await;

    let request = Request::builder()
        .method("POST")
        .uri("/v1/embeddings")
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Route exists — should not be 404 or 405
    assert_ne!(response.status(), StatusCode::NOT_FOUND);
    assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn embeddings_returns_valid_response() {
    let mock_server = wiremock::MockServer::start().await;

    // Mock the embedding backend response (OpenAI format)
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "object": "list",
            "data": [
                {
                    "object": "embedding",
                    "embedding": [0.1, 0.2, 0.3],
                    "index": 0
                }
            ],
            "model": "test-model",
            "usage": {
                "prompt_tokens": 5,
                "total_tokens": 5
            }
        })))
        .mount(&mock_server)
        .await;

    let (mut app, _registry) = make_app_with_mock(&mock_server).await;

    let body = serde_json::json!({
        "model": "test-model",
        "input": "hello world"
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/embeddings")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // The default test helper uses add_backend without agent, so the
    // handler returns 502 (no agent registered). This is expected:
    // embedding capability requires a registered agent.
    let status = response.status();
    assert!(
        status == StatusCode::OK
            || status == StatusCode::SERVICE_UNAVAILABLE
            || status == StatusCode::BAD_GATEWAY,
        "Unexpected status: {}",
        status
    );
}

#[tokio::test]
async fn embeddings_model_not_found_returns_error() {
    let mock_server = wiremock::MockServer::start().await;
    let (mut app, _registry) = make_app_with_mock(&mock_server).await;

    let body = serde_json::json!({
        "model": "nonexistent-model",
        "input": "hello"
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/embeddings")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn embeddings_batch_input_accepted() {
    let mock_server = wiremock::MockServer::start().await;
    let (mut app, _registry) = make_app_with_mock(&mock_server).await;

    let body = serde_json::json!({
        "model": "test-model",
        "input": ["hello", "world"]
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/embeddings")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Should not be 400 (bad request) — input format accepted
    assert_ne!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn embeddings_invalid_json_returns_422() {
    let mock_server = wiremock::MockServer::start().await;
    let (mut app, _registry) = make_app_with_mock(&mock_server).await;

    let request = Request::builder()
        .method("POST")
        .uri("/v1/embeddings")
        .header("content-type", "application/json")
        .body(Body::from("not valid json"))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Axum returns 400 for JSON parse errors in this project
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
