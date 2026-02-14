//! Type definitions for dashboard data structures

use serde::{Deserialize, Serialize};

/// Entry in the request history ring buffer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Unix timestamp in seconds
    pub timestamp: u64,
    /// Model name requested
    pub model: String,
    /// Backend ID that handled the request
    pub backend_id: String,
    /// Request duration in milliseconds
    pub duration_ms: u64,
    /// Request outcome
    pub status: RequestStatus,
    /// Error message if status is Error
    pub error_message: Option<String>,
}

/// Status of a completed request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RequestStatus {
    Success,
    Error,
}

/// WebSocket update message sent to dashboard clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketUpdate {
    /// Type of update
    pub update_type: UpdateType,
    /// JSON payload for the update
    pub data: serde_json::Value,
}

/// Type of WebSocket update
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UpdateType {
    /// Backend health status changed
    BackendStatus,
    /// Request completed
    RequestComplete,
    /// Model availability changed
    ModelChange,
}
