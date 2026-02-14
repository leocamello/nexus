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
