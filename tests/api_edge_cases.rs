//! Edge case tests for the API Gateway.
//!
//! These tests verify handling of edge cases like payload limits, backend failures,
//! invalid responses, and concurrent requests.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::StreamExt;
use nexus::api::{create_router, AppState};
use nexus::config::NexusConfig;
use nexus::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model, Registry};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower::Service;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Create a test app with two mock backends.
async fn create_test_app_with_two_backends(
    mock1: &MockServer,
    mock2: &MockServer,
) -> (axum::Router, Arc<Registry>) {
    let registry = Arc::new(Registry::new());
    let config = Arc::new(NexusConfig::default());

    // Backend 1
    let backend1 = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        mock1.uri(),
        BackendType::Generic,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    registry.add_backend(backend1).unwrap();
    let _ = registry.update_status("backend-1", BackendStatus::Healthy, None);
    let _ = registry.update_models(
        "backend-1",
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

    // Backend 2
    let backend2 = Backend::new(
        "backend-2".to_string(),
        "Backend 2".to_string(),
        mock2.uri(),
        BackendType::Generic,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    registry.add_backend(backend2).unwrap();
    let _ = registry.update_status("backend-2", BackendStatus::Healthy, None);
    let _ = registry.update_models(
        "backend-2",
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

/// Create a valid chat completion response.
fn create_completion_response() -> serde_json::Value {
    serde_json::json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "created": 1699999999,
        "model": "test-model",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "Hello!"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 5,
            "total_tokens": 15
        }
    })
}

/// Helper to read body as string.
async fn body_to_string(body: Body) -> String {
    let mut body_stream = body.into_data_stream();
    let mut result = String::new();
    while let Some(chunk) = body_stream.next().await {
        if let Ok(bytes) = chunk {
            result.push_str(&String::from_utf8_lossy(&bytes));
        }
    }
    result
}

// =============================================================================
// T10: Edge Case Tests
// =============================================================================

#[tokio::test]
async fn test_completions_payload_too_large() {
    let mock_server = MockServer::start().await;
    let (mut app, _) = common::make_app_with_mock(&mock_server).await;

    // Create a payload larger than 10MB
    let large_content = "x".repeat(11 * 1024 * 1024);
    let body = format!(
        r#"{{"model": "test-model", "messages": [{{"role": "user", "content": "{}"}}]}}"#,
        large_content
    );

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();

    let response = app.call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn test_completions_all_retries_fail() {
    let mock1 = MockServer::start().await;
    let mock2 = MockServer::start().await;

    // Both backends return 500 errors
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .mount(&mock1)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .mount(&mock2)
        .await;

    let (mut app, _) = create_test_app_with_two_backends(&mock1, &mock2).await;

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model": "test-model", "messages": [{"role": "user", "content": "Hi"}]}"#,
        ))
        .unwrap();

    let response = app.call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

    let body = body_to_string(response.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(json["error"]["message"].as_str().unwrap().contains("500"));
}

#[tokio::test]
async fn test_completions_backend_invalid_json() {
    let mock_server = MockServer::start().await;

    // Backend returns invalid JSON
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
        .mount(&mock_server)
        .await;

    let (mut app, _) = common::make_app_with_mock(&mock_server).await;

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model": "test-model", "messages": [{"role": "user", "content": "Hi"}]}"#,
        ))
        .unwrap();

    let response = app.call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

    let body = body_to_string(response.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Invalid"));
}

#[tokio::test]
async fn test_completions_empty_messages() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(create_completion_response()))
        .mount(&mock_server)
        .await;

    let (mut app, _) = common::make_app_with_mock(&mock_server).await;

    // Empty messages array should still be accepted (backend may validate)
    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"model": "test-model", "messages": []}"#))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Should succeed (we pass through to backend)
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_completions_missing_model_field() {
    let mock_server = MockServer::start().await;
    let (mut app, _) = common::make_app_with_mock(&mock_server).await;

    // Missing model field
    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"messages": [{"role": "user", "content": "Hi"}]}"#,
        ))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Should return 422 Unprocessable Entity (deserialization failure)
    assert!(
        response.status() == StatusCode::UNPROCESSABLE_ENTITY
            || response.status() == StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn test_completions_long_model_name() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(create_completion_response()))
        .mount(&mock_server)
        .await;

    let registry = Arc::new(Registry::new());
    let config = Arc::new(NexusConfig::default());

    let long_model_name = "x".repeat(500);

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
            id: long_model_name.clone(),
            name: "Long Name Model".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }],
    );

    let state = Arc::new(AppState::new(registry, config));
    let mut app = create_router(state);

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(format!(
            r#"{{"model": "{}", "messages": [{{"role": "user", "content": "Hi"}}]}}"#,
            long_model_name
        )))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Should work with long model names
    assert_eq!(response.status(), StatusCode::OK);
}

// =============================================================================
// T12: Concurrent Request Tests
// =============================================================================

#[tokio::test]
async fn test_concurrent_100_requests() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(create_completion_response()))
        .mount(&mock_server)
        .await;

    let (app, _) = common::make_app_with_mock(&mock_server).await;
    let app = Arc::new(tokio::sync::Mutex::new(app));

    let handles: Vec<_> = (0..100)
        .map(|_| {
            let app = Arc::clone(&app);
            tokio::spawn(async move {
                let request = Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model": "test-model", "messages": [{"role": "user", "content": "Hi"}]}"#,
                    ))
                    .unwrap();

                let mut app = app.lock().await;
                app.call(request).await
            })
        })
        .collect();

    let results: Vec<_> = futures::future::join_all(handles).await;

    let success_count = results
        .iter()
        .filter(|r| {
            r.as_ref()
                .ok()
                .and_then(|r| r.as_ref().ok())
                .map(|r| r.status() == StatusCode::OK)
                .unwrap_or(false)
        })
        .count();

    assert!(
        success_count >= 95,
        "Expected at least 95 successful requests out of 100, got {}",
        success_count
    );
}

#[tokio::test]
async fn test_concurrent_streaming_requests() {
    let mock_server = MockServer::start().await;

    let sse_response = "data: {\"id\":\"1\",\"object\":\"chat.completion.chunk\",\"created\":1699999999,\"model\":\"test\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n";

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_response)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let (app, _) = common::make_app_with_mock(&mock_server).await;
    let app = Arc::new(tokio::sync::Mutex::new(app));

    let handles: Vec<_> = (0..20)
        .map(|_| {
            let app = Arc::clone(&app);
            tokio::spawn(async move {
                let request = Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model": "test-model", "messages": [{"role": "user", "content": "Hi"}], "stream": true}"#,
                    ))
                    .unwrap();

                let mut app = app.lock().await;
                app.call(request).await
            })
        })
        .collect();

    let results: Vec<_> = futures::future::join_all(handles).await;

    let success_count = results
        .iter()
        .filter(|r| {
            r.as_ref()
                .ok()
                .and_then(|r| r.as_ref().ok())
                .map(|r| r.status() == StatusCode::OK)
                .unwrap_or(false)
        })
        .count();

    assert!(
        success_count >= 18,
        "Expected at least 18 successful streaming requests, got {}",
        success_count
    );
}

// =============================================================================
// T13: Performance Tests
// =============================================================================

#[tokio::test]
async fn test_response_overhead_reasonable() {
    let mock_server = MockServer::start().await;

    // Backend responds instantly
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(create_completion_response()))
        .mount(&mock_server)
        .await;

    let (app, _) = common::make_app_with_mock(&mock_server).await;
    let app = Arc::new(tokio::sync::Mutex::new(app));

    let mut times = Vec::new();

    // Warm up
    for _ in 0..5 {
        let request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"model": "test-model", "messages": [{"role": "user", "content": "Hi"}]}"#,
            ))
            .unwrap();

        let mut app = app.lock().await;
        let _ = app.call(request).await;
    }

    // Measure
    for _ in 0..20 {
        let request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"model": "test-model", "messages": [{"role": "user", "content": "Hi"}]}"#,
            ))
            .unwrap();

        let start = Instant::now();
        let mut app = app.lock().await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        times.push(start.elapsed());
    }

    let avg = times.iter().sum::<Duration>() / times.len() as u32;

    // Average overhead should be reasonable (< 50ms for test environment)
    // Note: In CI, this may be higher due to resource constraints
    assert!(
        avg < Duration::from_millis(100),
        "Average response time too high: {:?}",
        avg
    );
}

#[tokio::test]
async fn test_pending_request_tracking_accuracy() {
    let mock_server = MockServer::start().await;

    // Backend has a small delay
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(create_completion_response())
                .set_delay(Duration::from_millis(50)),
        )
        .mount(&mock_server)
        .await;

    let (app, registry) = common::make_app_with_mock(&mock_server).await;
    let app = Arc::new(tokio::sync::Mutex::new(app));

    // Initial pending should be 0
    let backend = registry.get_backend("test-backend").unwrap();
    assert_eq!(
        backend
            .pending_requests
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );

    // Start a request
    let app_clone = Arc::clone(&app);
    let handle = tokio::spawn(async move {
        let request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"model": "test-model", "messages": [{"role": "user", "content": "Hi"}]}"#,
            ))
            .unwrap();

        let mut app = app_clone.lock().await;
        app.call(request).await
    });

    // Wait a bit and check pending count
    tokio::time::sleep(Duration::from_millis(20)).await;
    let backend = registry.get_backend("test-backend").unwrap();
    let pending = backend
        .pending_requests
        .load(std::sync::atomic::Ordering::SeqCst);
    // May be 0 or 1 depending on timing
    assert!(pending <= 1);

    // Wait for request to complete
    let _ = handle.await;

    // After completion, pending should be 0
    let backend = registry.get_backend("test-backend").unwrap();
    assert_eq!(
        backend
            .pending_requests
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

// =============================================================================
// Additional Edge Case Tests (from analysis)
// =============================================================================

#[tokio::test]
async fn test_completions_forwards_auth_header_nonstreaming() {
    let mock_server = MockServer::start().await;

    // Mock expects Authorization header
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(wiremock::matchers::header(
            "Authorization",
            "Bearer test-token-123",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(create_completion_response()))
        .mount(&mock_server)
        .await;

    let (mut app, _) = common::make_app_with_mock(&mock_server).await;

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .header("Authorization", "Bearer test-token-123")
        .body(Body::from(
            r#"{"model": "test-model", "messages": [{"role": "user", "content": "Hi"}]}"#,
        ))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // If auth header wasn't forwarded, wiremock wouldn't match and we'd get an error
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_streaming_latency_under_threshold() {
    let mock_server = MockServer::start().await;

    // Create SSE response with multiple chunks
    let sse_response = r#"data: {"id":"1","object":"chat.completion.chunk","created":1699999999,"model":"test","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}

data: {"id":"1","object":"chat.completion.chunk","created":1699999999,"model":"test","choices":[{"index":0,"delta":{"content":" world"},"finish_reason":null}]}

data: {"id":"1","object":"chat.completion.chunk","created":1699999999,"model":"test","choices":[{"index":0,"delta":{"content":"!"},"finish_reason":"stop"}]}

data: [DONE]

"#;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_response)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let (mut app, _) = common::make_app_with_mock(&mock_server).await;

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model": "test-model", "messages": [{"role": "user", "content": "Hi"}], "stream": true}"#,
        ))
        .unwrap();

    let start = Instant::now();
    let response = app.call(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Collect all chunks and measure time to first chunk
    let mut body_stream = response.into_body().into_data_stream();
    let mut first_chunk_time = None;
    let mut chunk_count = 0;

    while let Some(chunk) = body_stream.next().await {
        if chunk.is_ok() {
            if first_chunk_time.is_none() {
                first_chunk_time = Some(start.elapsed());
            }
            chunk_count += 1;
        }
    }

    // First chunk should arrive quickly (< 50ms overhead in test environment)
    if let Some(time) = first_chunk_time {
        assert!(
            time < Duration::from_millis(50),
            "First chunk took too long: {:?}",
            time
        );
    }

    // Should have received chunks
    assert!(chunk_count > 0, "No chunks received");
}

#[tokio::test]
async fn test_streaming_handles_backend_disconnect() {
    let mock_server = MockServer::start().await;

    // Backend sends partial response then disconnects (simulated by incomplete SSE)
    let partial_sse = r#"data: {"id":"1","object":"chat.completion.chunk","created":1699999999,"model":"test","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}

"#;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(partial_sse)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let (mut app, _) = common::make_app_with_mock(&mock_server).await;

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model": "test-model", "messages": [{"role": "user", "content": "Hi"}], "stream": true}"#,
        ))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Should still return 200 (streaming started)
    assert_eq!(response.status(), StatusCode::OK);

    // Collect body - should get partial data without panic
    let body = body_to_string(response.into_body()).await;

    // Should have received at least some data
    assert!(
        body.contains("Hello") || body.contains("data:"),
        "Expected partial data, got: {}",
        body
    );
}

#[tokio::test]
async fn test_streaming_transforms_backend_format() {
    let mock_server = MockServer::start().await;

    // Backend returns valid OpenAI SSE format - verify we forward it correctly
    let sse_response = r#"data: {"id":"chatcmpl-abc","object":"chat.completion.chunk","created":1699999999,"model":"gpt-4","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}

data: {"id":"chatcmpl-abc","object":"chat.completion.chunk","created":1699999999,"model":"gpt-4","choices":[{"index":0,"delta":{"content":"Test"},"finish_reason":null}]}

data: {"id":"chatcmpl-abc","object":"chat.completion.chunk","created":1699999999,"model":"gpt-4","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]

"#;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_response)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let (mut app, _) = common::make_app_with_mock(&mock_server).await;

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model": "test-model", "messages": [{"role": "user", "content": "Hi"}], "stream": true}"#,
        ))
        .unwrap();

    let response = app.call(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_string(response.into_body()).await;

    // Verify output is valid SSE with OpenAI format
    let data_lines: Vec<_> = body
        .lines()
        .filter(|l| l.starts_with("data: ") && !l.contains("[DONE]"))
        .collect();

    assert!(!data_lines.is_empty(), "No data lines found");

    // Each data line should be valid JSON with correct structure
    for line in data_lines {
        let json_str = &line[6..];
        let json: serde_json::Value = serde_json::from_str(json_str)
            .unwrap_or_else(|e| panic!("Invalid JSON in line '{}': {}", line, e));
        assert_eq!(json["object"], "chat.completion.chunk");
        assert!(json.get("choices").is_some());
    }

    // Should end with [DONE]
    assert!(body.contains("[DONE]"), "Missing [DONE] terminator");
}
