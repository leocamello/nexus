//! WebSocket handler for real-time dashboard updates

use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::Response,
};
use futures::{SinkExt, StreamExt};
use std::sync::Arc;

use crate::api::AppState;
use crate::dashboard::types::{UpdateType, WebSocketUpdate};
use crate::registry::BackendView;

/// Handles WebSocket upgrade requests for dashboard real-time updates
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handles an established WebSocket connection
async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to broadcast channel
    let mut rx = state.ws_broadcast.subscribe();

    // Spawn task to forward broadcast messages to WebSocket
    let send_task = tokio::spawn(async move {
        while let Ok(update) = rx.recv().await {
            // Serialize update to JSON
            match serde_json::to_string(&update) {
                Ok(json) => {
                    // Enforce 10KB message size limit
                    if json.len() > 10 * 1024 {
                        tracing::warn!(
                            "WebSocket message exceeds 10KB limit ({}B), truncating",
                            json.len()
                        );
                        // Skip oversized messages instead of truncating to avoid malformed JSON
                        continue;
                    }

                    if (sender.send(Message::Text(json)).await).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to serialize WebSocket update: {}", e);
                }
            }
        }
    });

    // Handle incoming messages (ping/pong, close)
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Close(_) => break,
                Message::Ping(_) => {
                    // axum automatically handles pong responses
                }
                _ => {
                    // Ignore other message types
                }
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }
}

/// Create a backend status update message
pub fn create_backend_status_update(backends: Vec<BackendView>) -> WebSocketUpdate {
    WebSocketUpdate {
        update_type: UpdateType::BackendStatus,
        data: serde_json::to_value(backends).unwrap_or(serde_json::Value::Null),
    }
}

/// Create a model change update message
pub fn create_model_change_update(
    backend_id: String,
    models: Vec<serde_json::Value>,
) -> WebSocketUpdate {
    let data = serde_json::json!({
        "backend_id": backend_id,
        "models": models,
    });

    WebSocketUpdate {
        update_type: UpdateType::ModelChange,
        data,
    }
}

/// Create a request complete update message
pub fn create_request_complete_update(
    entry: crate::dashboard::types::HistoryEntry,
) -> WebSocketUpdate {
    WebSocketUpdate {
        update_type: UpdateType::RequestComplete,
        data: serde_json::to_value(entry).unwrap_or(serde_json::Value::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::types::{HistoryEntry, RequestStatus};

    #[test]
    fn test_create_backend_status_update() {
        use crate::registry::{BackendStatus, BackendType, DiscoverySource};
        use chrono::Utc;
        use std::collections::HashMap;

        let backends = vec![BackendView {
            id: "b1".to_string(),
            name: "b1".to_string(),
            url: "http://localhost:11434".to_string(),
            backend_type: BackendType::Ollama,
            status: BackendStatus::Healthy,
            last_health_check: Utc::now(),
            last_error: None,
            models: vec![],
            priority: 10,
            pending_requests: 0,
            total_requests: 5,
            avg_latency_ms: 42,
            discovery_source: DiscoverySource::Static,
            metadata: HashMap::new(),
        }];

        let update = create_backend_status_update(backends);
        assert_eq!(update.update_type, UpdateType::BackendStatus);
        assert!(update.data.is_array());
        assert_eq!(update.data[0]["id"], "b1");
    }

    #[test]
    fn test_create_model_change_update() {
        let models = vec![serde_json::json!({"id": "gpt-4", "context_length": 8192})];
        let update = create_model_change_update("backend-1".to_string(), models);

        assert_eq!(update.update_type, UpdateType::ModelChange);
        assert_eq!(update.data["backend_id"], "backend-1");
        assert!(update.data["models"].is_array());
    }

    #[test]
    fn test_create_request_complete_update() {
        let entry = HistoryEntry {
            timestamp: 1234567890,
            model: "gpt-4".to_string(),
            backend_id: "backend-1".to_string(),
            duration_ms: 150,
            status: RequestStatus::Success,
            error_message: None,
        };

        let update = create_request_complete_update(entry);
        assert_eq!(update.update_type, UpdateType::RequestComplete);
        assert_eq!(update.data["model"], "gpt-4");
        assert_eq!(update.data["duration_ms"], 150);
    }

    #[test]
    fn test_create_backend_status_update_empty() {
        let update = create_backend_status_update(vec![]);
        assert_eq!(update.update_type, UpdateType::BackendStatus);
        assert!(update.data.as_array().unwrap().is_empty());
    }

    #[test]
    fn test_create_model_change_update_empty_models() {
        let update = create_model_change_update("b1".to_string(), vec![]);
        assert_eq!(update.update_type, UpdateType::ModelChange);
        assert!(update.data["models"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_create_request_complete_update_with_error() {
        let entry = HistoryEntry {
            timestamp: 1234567890,
            model: "gpt-4".to_string(),
            backend_id: "backend-1".to_string(),
            duration_ms: 0,
            status: RequestStatus::Error,
            error_message: Some("Connection refused".to_string()),
        };

        let update = create_request_complete_update(entry);
        assert_eq!(update.update_type, UpdateType::RequestComplete);
        assert_eq!(update.data["error_message"], "Connection refused");
    }

    #[test]
    fn test_backend_status_update_serialization() {
        use crate::registry::{BackendStatus, BackendType, DiscoverySource};
        use chrono::Utc;
        use std::collections::HashMap;

        let backends = vec![BackendView {
            id: "b1".to_string(),
            name: "b1".to_string(),
            url: "http://localhost:11434".to_string(),
            backend_type: BackendType::Ollama,
            status: BackendStatus::Healthy,
            last_health_check: Utc::now(),
            last_error: None,
            models: vec![],
            priority: 10,
            pending_requests: 0,
            total_requests: 5,
            avg_latency_ms: 42,
            discovery_source: DiscoverySource::Static,
            metadata: HashMap::new(),
        }];

        let update = create_backend_status_update(backends);
        let json = serde_json::to_string(&update).unwrap();
        assert!(json.contains("BackendStatus") || json.contains("backend_status"));
        assert!(json.contains("b1"));
    }

    #[test]
    fn test_create_backend_status_update_many_models() {
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model};
        use chrono::Utc;
        use std::collections::HashMap;

        let models: Vec<Model> = (0..7)
            .map(|i| Model {
                id: format!("model-{i}"),
                name: format!("Model {i}"),
                context_length: 4096 * (i + 1),
                supports_vision: i % 2 == 0,
                supports_tools: i % 3 == 0,
                supports_json_mode: true,
                max_output_tokens: Some(2048),
            })
            .collect();

        let backends = vec![BackendView {
            id: "multi-model".to_string(),
            name: "multi-model-backend".to_string(),
            url: "http://localhost:11434".to_string(),
            backend_type: BackendType::Ollama,
            status: BackendStatus::Healthy,
            last_health_check: Utc::now(),
            last_error: None,
            models,
            priority: 5,
            pending_requests: 3,
            total_requests: 100,
            avg_latency_ms: 50,
            discovery_source: DiscoverySource::Static,
            metadata: HashMap::new(),
        }];

        let update = create_backend_status_update(backends);
        assert_eq!(update.update_type, UpdateType::BackendStatus);
        let arr = update.data[0]["models"].as_array().unwrap();
        assert_eq!(arr.len(), 7);
        assert_eq!(update.data[0]["models"][6]["id"], "model-6");
    }

    #[test]
    fn test_create_backend_status_update_special_chars_in_name() {
        use crate::registry::{BackendStatus, BackendType, DiscoverySource};
        use chrono::Utc;
        use std::collections::HashMap;

        let backends = vec![BackendView {
            id: "special-1".to_string(),
            name: "my-backend/v2 (héllo \"wörld\") <test> & co.".to_string(),
            url: "http://localhost:11434".to_string(),
            backend_type: BackendType::Generic,
            status: BackendStatus::Unhealthy,
            last_health_check: Utc::now(),
            last_error: Some("connection: timed out".to_string()),
            models: vec![],
            priority: 1,
            pending_requests: 0,
            total_requests: 0,
            avg_latency_ms: 0,
            discovery_source: DiscoverySource::Manual,
            metadata: HashMap::new(),
        }];

        let update = create_backend_status_update(backends);
        assert_eq!(update.update_type, UpdateType::BackendStatus);
        // Verify the special characters survive serialization round-trip
        let json = serde_json::to_string(&update).unwrap();
        let parsed: WebSocketUpdate = serde_json::from_str(&json).unwrap();
        assert!(parsed.data[0]["name"].as_str().unwrap().contains("héllo"));
        assert!(parsed.data[0]["name"].as_str().unwrap().contains("& co."));
    }

    #[test]
    fn test_create_request_complete_update_error_with_message() {
        let entry = HistoryEntry {
            timestamp: 9999999999,
            model: "gpt-4-turbo".to_string(),
            backend_id: "backend-err".to_string(),
            duration_ms: 5000,
            status: RequestStatus::Error,
            error_message: Some("Backend returned HTTP 502: Bad Gateway".to_string()),
        };

        let update = create_request_complete_update(entry);
        assert_eq!(update.update_type, UpdateType::RequestComplete);
        assert_eq!(update.data["status"], "Error");
        assert_eq!(
            update.data["error_message"],
            "Backend returned HTTP 502: Bad Gateway"
        );
        assert_eq!(update.data["duration_ms"], 5000);
    }

    #[test]
    fn test_create_request_complete_update_very_long_model_name() {
        let long_name = "a".repeat(500);
        let entry = HistoryEntry {
            timestamp: 1234567890,
            model: long_name.clone(),
            backend_id: "b1".to_string(),
            duration_ms: 42,
            status: RequestStatus::Success,
            error_message: None,
        };

        let update = create_request_complete_update(entry);
        assert_eq!(update.data["model"].as_str().unwrap(), long_name);
    }

    #[test]
    fn test_create_model_change_update_empty_models_for_backend() {
        let update = create_model_change_update("backend-gone".to_string(), vec![]);
        assert_eq!(update.update_type, UpdateType::ModelChange);
        assert_eq!(update.data["backend_id"], "backend-gone");
        assert!(update.data["models"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_all_update_types_produce_valid_json() {
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model};
        use chrono::Utc;
        use std::collections::HashMap;

        // BackendStatus update
        let backend_update = create_backend_status_update(vec![BackendView {
            id: "b1".to_string(),
            name: "test".to_string(),
            url: "http://localhost:11434".to_string(),
            backend_type: BackendType::Ollama,
            status: BackendStatus::Healthy,
            last_health_check: Utc::now(),
            last_error: None,
            models: vec![Model {
                id: "m1".to_string(),
                name: "M1".to_string(),
                context_length: 4096,
                supports_vision: false,
                supports_tools: false,
                supports_json_mode: false,
                max_output_tokens: None,
            }],
            priority: 1,
            pending_requests: 0,
            total_requests: 0,
            avg_latency_ms: 0,
            discovery_source: DiscoverySource::Static,
            metadata: HashMap::new(),
        }]);
        let json1 = serde_json::to_string(&backend_update).unwrap();
        let _: serde_json::Value = serde_json::from_str(&json1).unwrap();

        // ModelChange update
        let model_update =
            create_model_change_update("b1".to_string(), vec![serde_json::json!({"id": "llama3"})]);
        let json2 = serde_json::to_string(&model_update).unwrap();
        let _: serde_json::Value = serde_json::from_str(&json2).unwrap();

        // RequestComplete update
        let req_update = create_request_complete_update(HistoryEntry {
            timestamp: 0,
            model: "m".to_string(),
            backend_id: "b".to_string(),
            duration_ms: 1,
            status: RequestStatus::Success,
            error_message: None,
        });
        let json3 = serde_json::to_string(&req_update).unwrap();
        let parsed: WebSocketUpdate = serde_json::from_str(&json3).unwrap();
        assert_eq!(parsed.update_type, UpdateType::RequestComplete);
    }

    #[test]
    fn test_create_backend_status_update_empty_vec() {
        let update = create_backend_status_update(vec![]);
        assert_eq!(update.update_type, UpdateType::BackendStatus);
        assert_eq!(update.data, serde_json::json!([]));
    }

    #[test]
    fn test_create_update_with_request_history() {
        let entry = HistoryEntry {
            timestamp: 1700000000,
            model: "llama3:8b".to_string(),
            backend_id: "backend-abc".to_string(),
            duration_ms: 320,
            status: RequestStatus::Success,
            error_message: None,
        };

        let update = create_request_complete_update(entry);
        assert_eq!(update.update_type, UpdateType::RequestComplete);
        assert_eq!(update.data["timestamp"], 1700000000);
        assert_eq!(update.data["model"], "llama3:8b");
        assert_eq!(update.data["backend_id"], "backend-abc");
        assert_eq!(update.data["duration_ms"], 320);
        assert_eq!(update.data["status"], "Success");
        assert!(update.data["error_message"].is_null());
    }

    #[test]
    fn test_create_update_with_multiple_backends() {
        use crate::registry::{BackendStatus, BackendType, DiscoverySource};
        use chrono::Utc;
        use std::collections::HashMap;

        let backends = vec![
            BackendView {
                id: "healthy-1".to_string(),
                name: "Healthy Backend".to_string(),
                url: "http://localhost:11434".to_string(),
                backend_type: BackendType::Ollama,
                status: BackendStatus::Healthy,
                last_health_check: Utc::now(),
                last_error: None,
                models: vec![],
                priority: 10,
                pending_requests: 2,
                total_requests: 100,
                avg_latency_ms: 50,
                discovery_source: DiscoverySource::Static,
                metadata: HashMap::new(),
            },
            BackendView {
                id: "unhealthy-1".to_string(),
                name: "Unhealthy Backend".to_string(),
                url: "http://localhost:8000".to_string(),
                backend_type: BackendType::VLLM,
                status: BackendStatus::Unhealthy,
                last_health_check: Utc::now(),
                last_error: Some("Connection refused".to_string()),
                models: vec![],
                priority: 5,
                pending_requests: 0,
                total_requests: 50,
                avg_latency_ms: 0,
                discovery_source: DiscoverySource::MDNS,
                metadata: HashMap::new(),
            },
            BackendView {
                id: "unknown-1".to_string(),
                name: "Unknown Backend".to_string(),
                url: "http://localhost:9090".to_string(),
                backend_type: BackendType::Generic,
                status: BackendStatus::Unknown,
                last_health_check: Utc::now(),
                last_error: None,
                models: vec![],
                priority: 1,
                pending_requests: 0,
                total_requests: 0,
                avg_latency_ms: 0,
                discovery_source: DiscoverySource::Manual,
                metadata: HashMap::new(),
            },
        ];

        let update = create_backend_status_update(backends);
        assert_eq!(update.update_type, UpdateType::BackendStatus);
        let arr = update.data.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["id"], "healthy-1");
        assert_eq!(arr[1]["id"], "unhealthy-1");
        assert_eq!(arr[2]["id"], "unknown-1");
        assert_eq!(arr[1]["last_error"], "Connection refused");
    }

    #[test]
    fn test_websocket_update_serialization_roundtrip() {
        use crate::registry::{BackendStatus, BackendType, DiscoverySource};
        use chrono::Utc;
        use std::collections::HashMap;

        let backends = vec![BackendView {
            id: "rt-1".to_string(),
            name: "Roundtrip Backend".to_string(),
            url: "http://10.0.0.1:11434".to_string(),
            backend_type: BackendType::Ollama,
            status: BackendStatus::Healthy,
            last_health_check: Utc::now(),
            last_error: None,
            models: vec![],
            priority: 7,
            pending_requests: 1,
            total_requests: 42,
            avg_latency_ms: 99,
            discovery_source: DiscoverySource::Static,
            metadata: HashMap::new(),
        }];

        let original = create_backend_status_update(backends);
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: WebSocketUpdate = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.update_type, original.update_type);
        assert_eq!(deserialized.data["id"], original.data["id"]);
        assert_eq!(deserialized.data[0]["name"], "Roundtrip Backend");
        assert_eq!(deserialized.data[0]["url"], "http://10.0.0.1:11434");
        assert_eq!(deserialized.data[0]["priority"], 7);
        assert_eq!(deserialized.data[0]["total_requests"], 42);
        assert_eq!(deserialized.data[0]["avg_latency_ms"], 99);
    }

    #[test]
    fn test_backend_update_fields() {
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model};
        use chrono::Utc;
        use std::collections::HashMap;

        let mut metadata = HashMap::new();
        metadata.insert("version".to_string(), "0.4.1".to_string());
        metadata.insert(
            "mdns_instance".to_string(),
            "my-server._ollama._tcp.local".to_string(),
        );

        let backends = vec![BackendView {
            id: "full-fields".to_string(),
            name: "Fully Populated".to_string(),
            url: "http://192.168.1.100:11434".to_string(),
            backend_type: BackendType::Ollama,
            status: BackendStatus::Healthy,
            last_health_check: Utc::now(),
            last_error: None,
            models: vec![
                Model {
                    id: "llama3".to_string(),
                    name: "LLaMA 3".to_string(),
                    context_length: 8192,
                    supports_vision: false,
                    supports_tools: true,
                    supports_json_mode: true,
                    max_output_tokens: Some(4096),
                },
                Model {
                    id: "llava".to_string(),
                    name: "LLaVA".to_string(),
                    context_length: 4096,
                    supports_vision: true,
                    supports_tools: false,
                    supports_json_mode: false,
                    max_output_tokens: None,
                },
            ],
            priority: 10,
            pending_requests: 5,
            total_requests: 1000,
            avg_latency_ms: 250,
            discovery_source: DiscoverySource::MDNS,
            metadata,
        }];

        let update = create_backend_status_update(backends);
        let data = &update.data[0];

        assert_eq!(data["id"], "full-fields");
        assert_eq!(data["name"], "Fully Populated");
        assert_eq!(data["url"], "http://192.168.1.100:11434");
        assert_eq!(data["priority"], 10);
        assert_eq!(data["pending_requests"], 5);
        assert_eq!(data["total_requests"], 1000);
        assert_eq!(data["avg_latency_ms"], 250);

        let models = data["models"].as_array().unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0]["id"], "llama3");
        assert_eq!(models[0]["supports_tools"], true);
        assert_eq!(models[1]["id"], "llava");
        assert_eq!(models[1]["supports_vision"], true);
    }

    #[test]
    fn test_create_model_change_update_with_models() {
        let models = vec![serde_json::json!({"id": "llama3", "name": "LLaMA 3"})];
        let update = create_model_change_update("backend-1".to_string(), models);
        assert_eq!(update.update_type, UpdateType::ModelChange);
        assert_eq!(update.data["backend_id"], "backend-1");
        assert_eq!(update.data["models"][0]["id"], "llama3");
    }

    #[test]
    fn test_create_request_complete_update_fields() {
        let entry = HistoryEntry {
            timestamp: 1234567890,
            model: "test-model".to_string(),
            backend_id: "backend-1".to_string(),
            duration_ms: 150,
            status: RequestStatus::Success,
            error_message: None,
        };
        let update = create_request_complete_update(entry);
        assert_eq!(update.update_type, UpdateType::RequestComplete);
        assert_eq!(update.data["model"], "test-model");
        assert_eq!(update.data["duration_ms"], 150);
    }

    #[test]
    fn test_websocket_update_serialization() {
        let update = create_backend_status_update(vec![]);
        let json = serde_json::to_string(&update).unwrap();
        assert!(json.contains("BackendStatus"));
        assert!(json.len() < 10 * 1024, "Should be under 10KB limit");
    }
}
