//! Health check endpoint handler.

use crate::api::AppState;
use crate::registry::BackendStatus;
use axum::{extract::State, Json};
use serde::Serialize;
use std::sync::Arc;

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub uptime_seconds: u64,
    pub backends: BackendCounts,
    pub models: usize,
}

/// Backend health counts.
#[derive(Debug, Serialize)]
pub struct BackendCounts {
    pub total: usize,
    pub healthy: usize,
    pub unhealthy: usize,
}

/// GET /health - Return system health status.
pub async fn handle(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let all_backends = state.registry.get_all_backends();
    let healthy_backends: Vec<_> = all_backends
        .iter()
        .filter(|b| b.status == BackendStatus::Healthy)
        .collect();

    let model_count = state.registry.model_count();

    let status = match (healthy_backends.len(), all_backends.len()) {
        (h, t) if h == t && t > 0 => "healthy",
        (h, _) if h > 0 => "degraded",
        _ => "unhealthy",
    };

    Json(HealthResponse {
        status: status.to_string(),
        uptime_seconds: 0, // TODO: Track startup time
        backends: BackendCounts {
            total: all_backends.len(),
            healthy: healthy_backends.len(),
            unhealthy: all_backends.len() - healthy_backends.len(),
        },
        models: model_count,
    })
}
