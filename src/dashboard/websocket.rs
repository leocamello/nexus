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
}
