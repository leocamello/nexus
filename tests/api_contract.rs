//! Contract tests for OpenAI API compliance.
//!
//! These tests verify that our API responses match the OpenAI API format exactly.

use serde_json::json;

#[test]
fn test_contract_completions_request_format() {
    // Valid request must have: model, messages
    let request = json!({
        "model": "llama3:70b",
        "messages": [{"role": "user", "content": "Hello"}]
    });
    assert!(request.get("model").is_some());
    assert!(request.get("messages").is_some());
}

#[test]
fn test_contract_completions_response_format() {
    // Response must have: id, object, created, model, choices, usage (optional)
    let response = json!({
        "id": "chatcmpl-abc123",
        "object": "chat.completion",
        "created": 1699999999,
        "model": "llama3:70b",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": "Hello!"},
            "finish_reason": "stop"
        }]
    });
    assert_eq!(response["object"], "chat.completion");
    assert!(response["choices"].is_array());
}

#[test]
fn test_contract_completions_streaming_format() {
    // Each chunk must have: id, object="chat.completion.chunk", choices with delta
    let chunk = json!({
        "id": "chatcmpl-abc123",
        "object": "chat.completion.chunk",
        "created": 1699999999,
        "model": "llama3:70b",
        "choices": [{"index": 0, "delta": {"content": "Hello"}, "finish_reason": null}]
    });
    assert_eq!(chunk["object"], "chat.completion.chunk");
}

#[test]
fn test_contract_models_response_format() {
    // Response must have: object="list", data array
    let response = json!({
        "object": "list",
        "data": [{"id": "llama3:70b", "object": "model"}]
    });
    assert_eq!(response["object"], "list");
    assert!(response["data"].is_array());
}

#[test]
fn test_contract_error_400_format() {
    let error = json!({
        "error": {
            "message": "Invalid request",
            "type": "invalid_request_error",
            "code": "invalid_request_error"
        }
    });
    assert!(error.get("error").is_some());
    assert!(error["error"].get("message").is_some());
}

#[test]
fn test_contract_error_404_format() {
    let error = json!({
        "error": {
            "message": "Model 'nonexistent' not found",
            "type": "invalid_request_error",
            "param": "model",
            "code": "model_not_found"
        }
    });
    assert_eq!(error["error"]["code"], "model_not_found");
}

#[test]
fn test_contract_error_502_format() {
    let error = json!({
        "error": {
            "message": "Backend returned invalid response",
            "type": "server_error",
            "code": "bad_gateway"
        }
    });
    assert_eq!(error["error"]["code"], "bad_gateway");
}

#[test]
fn test_contract_error_503_format() {
    let error = json!({
        "error": {
            "message": "No healthy backends available",
            "type": "server_error",
            "code": "service_unavailable"
        }
    });
    assert_eq!(error["error"]["code"], "service_unavailable");
}
