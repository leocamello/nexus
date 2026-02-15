//! Streaming SSE tests for the API Gateway.
//!
//! These tests verify SSE streaming functionality for chat completions.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use nexus::registry::BackendStatus;
use tower::Service;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Create SSE response body with chunks.
fn create_sse_response(chunks: &[&str], include_done: bool) -> String {
    let mut body = String::new();
    for (i, content) in chunks.iter().enumerate() {
        let chunk = format!(
            r#"{{"id":"chatcmpl-{}","object":"chat.completion.chunk","created":1699999999,"model":"test-model","choices":[{{"index":0,"delta":{{"content":"{}"}},"finish_reason":null}}]}}"#,
            i, content
        );
        body.push_str(&format!("data: {}\n\n", chunk));
    }
    if include_done {
        body.push_str("data: [DONE]\n\n");
    }
    body
}

/// Helper to read body as string
async fn body_to_string(body: Body) -> String {
    use futures::StreamExt;
    let mut body_stream = body.into_data_stream();
    let mut result = String::new();
    while let Some(chunk) = body_stream.next().await {
        if let Ok(bytes) = chunk {
            result.push_str(&String::from_utf8_lossy(&bytes));
        }
    }
    result
}

#[tokio::test]
async fn test_streaming_returns_sse_content_type() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(create_sse_response(&["Hello"], true))
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
    let content_type = response.headers().get("content-type").unwrap();
    assert!(
        content_type.to_str().unwrap().contains("text/event-stream"),
        "Expected text/event-stream, got {:?}",
        content_type
    );
}

#[tokio::test]
async fn test_streaming_sends_chunks() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(create_sse_response(&["Hello", " ", "World", "!"], true))
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
    let body_str = body_to_string(response.into_body()).await;

    // Should have multiple data: lines
    let data_lines: Vec<_> = body_str
        .lines()
        .filter(|l| l.starts_with("data: "))
        .collect();
    assert!(
        data_lines.len() >= 4,
        "Expected at least 4 data lines, got {}: {:?}",
        data_lines.len(),
        data_lines
    );
}

#[tokio::test]
async fn test_streaming_done_message() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(create_sse_response(&["Done"], true))
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
    let body_str = body_to_string(response.into_body()).await;

    assert!(
        body_str.contains("data: [DONE]"),
        "Expected [DONE] message in response: {}",
        body_str
    );
}

#[tokio::test]
async fn test_streaming_chunk_format() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(create_sse_response(&["Test"], true))
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
    let body_str = body_to_string(response.into_body()).await;

    // Parse each data line (except [DONE]) as JSON
    for line in body_str.lines() {
        if line.starts_with("data: ") && !line.contains("[DONE]") {
            let json_str = &line[6..];
            let json: serde_json::Value = serde_json::from_str(json_str)
                .unwrap_or_else(|e| panic!("Failed to parse chunk as JSON: {} - {}", json_str, e));
            assert_eq!(
                json["object"], "chat.completion.chunk",
                "Expected object=chat.completion.chunk"
            );
        }
    }
}

#[tokio::test]
async fn test_streaming_model_not_found() {
    let mock_server = MockServer::start().await;
    let (mut app, _) = common::make_app_with_mock(&mock_server).await;

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model": "nonexistent-model", "messages": [{"role": "user", "content": "Hi"}], "stream": true}"#,
        ))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Error returned before streaming starts
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_streaming_no_healthy_backends() {
    let mock_server = MockServer::start().await;
    let (mut app, registry) = common::make_app_with_mock(&mock_server).await;

    // Mark backend as unhealthy
    let _ = registry.update_status("test-backend", BackendStatus::Unhealthy, None);

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model": "test-model", "messages": [{"role": "user", "content": "Hi"}], "stream": true}"#,
        ))
        .unwrap();

    let response = app.call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_streaming_backend_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
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
    // Streaming starts, then error is reported as SSE event
    assert_eq!(response.status(), StatusCode::OK);

    let body_str = body_to_string(response.into_body()).await;

    // Should contain error indication
    assert!(
        body_str.contains("Error") || body_str.contains("error"),
        "Expected error in response: {}",
        body_str
    );
}

#[tokio::test]
async fn test_streaming_forwards_auth_header() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(wiremock::matchers::header(
            "Authorization",
            "Bearer test-token",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(create_sse_response(&["Auth OK"], true))
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let (mut app, _) = common::make_app_with_mock(&mock_server).await;

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .header("Authorization", "Bearer test-token")
        .body(Body::from(
            r#"{"model": "test-model", "messages": [{"role": "user", "content": "Hi"}], "stream": true}"#,
        ))
        .unwrap();

    let response = app.call(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
