//! Contract tests for dashboard WebSocket messages
//!
//! These tests verify that WebSocket message schemas match the contract
//! defined in specs/010-web-dashboard/contracts/

use nexus::dashboard::types::{UpdateType, WebSocketUpdate};
use serde_json::json;

#[test]
fn test_websocket_update_deserialization_with_backend_status() {
    // Test that WebSocketUpdate can deserialize with BackendStatus type
    let json_data = json!({
        "update_type": "BackendStatus",
        "data": {
            "backend_id": "backend-1",
            "status": "Healthy",
            "pending_requests": 5
        }
    });

    let result: Result<WebSocketUpdate, _> = serde_json::from_value(json_data);
    assert!(result.is_ok(), "Failed to deserialize BackendStatus update");

    let update = result.unwrap();
    assert_eq!(update.update_type, UpdateType::BackendStatus);
    assert!(update.data.is_object());
}

#[test]
fn test_backend_status_update_schema_validation() {
    // Test that backend_status update has required fields
    let update = WebSocketUpdate {
        update_type: UpdateType::BackendStatus,
        data: json!({
            "backend_id": "backend-1",
            "status": "Healthy",
            "pending_requests": 5,
            "last_latency_ms": 150
        }),
    };

    let serialized = serde_json::to_string(&update).unwrap();
    let deserialized: WebSocketUpdate = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized.update_type, UpdateType::BackendStatus);

    // Verify required fields exist in data
    let data = deserialized.data.as_object().unwrap();
    assert!(data.contains_key("backend_id"));
    assert!(data.contains_key("status"));
    assert!(data.contains_key("pending_requests"));
}

#[test]
fn test_backend_status_data_includes_all_required_fields() {
    // Verify backend_status update includes: backend_id, status, pending_requests, last_latency_ms
    let update = WebSocketUpdate {
        update_type: UpdateType::BackendStatus,
        data: json!({
            "backend_id": "backend-1",
            "url": "http://localhost:8001",
            "status": "Healthy",
            "pending_requests": 3,
            "last_latency_ms": 120,
            "models": ["gpt-4", "gpt-3.5-turbo"]
        }),
    };

    let json_str = serde_json::to_string(&update).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    let data = parsed["data"].as_object().unwrap();

    // Required fields
    assert!(data.contains_key("backend_id"), "Missing backend_id");
    assert!(data.contains_key("status"), "Missing status");
    assert!(
        data.contains_key("pending_requests"),
        "Missing pending_requests"
    );

    // Optional but expected fields
    assert!(data.get("backend_id").unwrap().is_string());
    assert!(data.get("status").unwrap().is_string());
    assert!(data.get("pending_requests").unwrap().is_number());
}

// ========== Model Change Update Tests (T070-T071) ==========

#[test]
fn test_model_change_websocket_update_message_schema() {
    // Test that model_change WebSocket update message schema is valid
    let json_data = json!({
        "update_type": "ModelChange",
        "data": {
            "backend_id": "backend-1",
            "action": "added",
            "models": [{
                "id": "gpt-4",
                "capabilities": {
                    "vision": true,
                    "tools": true,
                    "json_mode": true
                },
                "context_length": 32768
            }]
        }
    });

    let result: Result<WebSocketUpdate, _> = serde_json::from_value(json_data);
    assert!(result.is_ok(), "Failed to deserialize ModelChange update");

    let update = result.unwrap();
    assert_eq!(update.update_type, UpdateType::ModelChange);
    assert!(update.data.is_object());
}

#[test]
fn test_model_data_includes_capabilities_fields() {
    // Verify model data includes capabilities (vision, tools, json_mode) and context_length
    let update = WebSocketUpdate {
        update_type: UpdateType::ModelChange,
        data: json!({
            "backend_id": "backend-1",
            "action": "added",
            "models": [{
                "id": "gpt-4-vision",
                "capabilities": {
                    "vision": true,
                    "tools": true,
                    "json_mode": false
                },
                "context_length": 128000
            }, {
                "id": "llama3:70b",
                "capabilities": {
                    "vision": false,
                    "tools": true,
                    "json_mode": true
                },
                "context_length": 8192
            }]
        }),
    };

    let json_str = serde_json::to_string(&update).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    let models = parsed["data"]["models"].as_array().unwrap();
    assert!(!models.is_empty(), "Models array should not be empty");

    for model in models {
        let model_obj = model.as_object().unwrap();

        // Verify required fields
        assert!(model_obj.contains_key("id"), "Missing model id");
        assert!(
            model_obj.contains_key("capabilities"),
            "Missing capabilities"
        );
        assert!(
            model_obj.contains_key("context_length"),
            "Missing context_length"
        );

        // Verify capabilities object
        let capabilities = model_obj["capabilities"].as_object().unwrap();
        assert!(
            capabilities.contains_key("vision"),
            "Missing vision capability"
        );
        assert!(
            capabilities.contains_key("tools"),
            "Missing tools capability"
        );
        assert!(
            capabilities.contains_key("json_mode"),
            "Missing json_mode capability"
        );

        // Verify types
        assert!(capabilities["vision"].is_boolean());
        assert!(capabilities["tools"].is_boolean());
        assert!(capabilities["json_mode"].is_boolean());
        assert!(model_obj["context_length"].is_number());
    }
}
