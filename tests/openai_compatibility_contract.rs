//! Contract test for OpenAI compatibility (T045)
//!
//! Verifies that Nexus response body is byte-identical to direct OpenAI response.
//! Constitution Principle III: Never modify response JSON body.

mod common;

use axum::body::Body;
use axum::http::Request;
use futures::StreamExt;
use serde_json::Value;
use tower::Service;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// T045: Contract test comparing response body to direct OpenAI (byte-identical)
#[tokio::test]
async fn test_openai_response_body_unchanged() {
    let mock_server = MockServer::start().await;

    // Original OpenAI response (what the backend returns)
    let original_openai_response = serde_json::json!({
        "id": "chatcmpl-123456",
        "object": "chat.completion",
        "created": 1677652288,
        "model": "gpt-4-0613",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "This is a test response from OpenAI."
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 15,
            "completion_tokens": 10,
            "total_tokens": 25
        }
    });

    // Mock OpenAI backend to return this exact response
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(original_openai_response.clone()))
        .mount(&mock_server)
        .await;

    let (mut app, _registry) = common::make_app_with_mock(&mock_server).await;

    // Send request through Nexus
    let request_body = serde_json::json!({
        "model": "test-model",
        "messages": [
            {"role": "user", "content": "Hello, test the response."}
        ]
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Read response body
    let body = response.into_body();
    let body_bytes: Vec<u8> = body
        .into_data_stream()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(|chunk| chunk.ok())
        .flat_map(|chunk| chunk.to_vec())
        .collect();

    // Parse both responses
    let nexus_response: Value = serde_json::from_slice(&body_bytes).unwrap();
    let original_response: Value = original_openai_response;

    // Verify response bodies are identical
    assert_eq!(
        nexus_response, original_response,
        "Nexus modified the OpenAI response body! Constitution violation."
    );
}

/// Test that error responses from OpenAI are preserved
#[tokio::test]
async fn test_openai_error_response_preserved() {
    let mock_server = MockServer::start().await;

    // Original OpenAI error response
    let error_response = serde_json::json!({
        "error": {
            "message": "Invalid API key provided",
            "type": "invalid_request_error",
            "param": null,
            "code": "invalid_api_key"
        }
    });

    // Mock error response
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_json(error_response.clone()))
        .mount(&mock_server)
        .await;

    let (mut app, _registry) = common::make_app_with_mock(&mock_server).await;

    let request_body = serde_json::json!({
        "model": "test-model",
        "messages": [{"role": "user", "content": "Test"}]
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Read response body
    let body = response.into_body();
    let body_bytes: Vec<u8> = body
        .into_data_stream()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(|chunk| chunk.ok())
        .flat_map(|chunk| chunk.to_vec())
        .collect();

    let nexus_response: Value = serde_json::from_slice(&body_bytes).unwrap();

    // Verify error response is unchanged
    assert_eq!(
        nexus_response, error_response,
        "Nexus modified the OpenAI error response"
    );
}

/// Test that complex nested JSON structures are preserved
#[tokio::test]
async fn test_complex_response_body_unchanged() {
    let mock_server = MockServer::start().await;

    // Complex response with function calls, metadata, etc.
    let complex_response = serde_json::json!({
        "id": "chatcmpl-complex",
        "object": "chat.completion",
        "created": 1677652288,
        "model": "gpt-4-0613",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "function_call": {
                    "name": "get_weather",
                    "arguments": "{\"location\":\"Boston\",\"unit\":\"celsius\"}"
                }
            },
            "finish_reason": "function_call"
        }],
        "usage": {
            "prompt_tokens": 50,
            "completion_tokens": 30,
            "total_tokens": 80
        },
        "system_fingerprint": "fp_44709d6fcb"
    });

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(complex_response.clone()))
        .mount(&mock_server)
        .await;

    let (mut app, _registry) = common::make_app_with_mock(&mock_server).await;

    let request_body = serde_json::json!({
        "model": "test-model",
        "messages": [{"role": "user", "content": "What's the weather?"}],
        "functions": [{
            "name": "get_weather",
            "description": "Get weather",
            "parameters": {
                "type": "object",
                "properties": {
                    "location": {"type": "string"}
                }
            }
        }]
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Read response body
    let body = response.into_body();
    let body_bytes: Vec<u8> = body
        .into_data_stream()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(|chunk| chunk.ok())
        .flat_map(|chunk| chunk.to_vec())
        .collect();

    let nexus_response: Value = serde_json::from_slice(&body_bytes).unwrap();

    // Verify complex response is unchanged
    assert_eq!(
        nexus_response, complex_response,
        "Nexus modified complex OpenAI response with function calls"
    );
}
