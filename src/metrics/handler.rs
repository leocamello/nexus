//! # Metrics HTTP Handlers
//!
//! Axum handlers for metrics endpoints.

use super::{BackendStats, ModelStats, RequestStats, StatsResponse};
use crate::api::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use std::sync::Arc;

/// Handler for GET /metrics endpoint (Prometheus text format).
///
/// Returns metrics in Prometheus exposition format for scraping.
pub async fn metrics_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Update fleet gauges before rendering
    state.metrics_collector.update_fleet_gauges();

    // Get Prometheus text format from collector
    let metrics = state.metrics_collector.render_metrics();
    (StatusCode::OK, metrics)
}

/// Handler for GET /v1/stats endpoint (JSON format).
///
/// Returns aggregated statistics in JSON format for dashboards and debugging.
pub async fn stats_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Update fleet gauges before computing stats
    state.metrics_collector.update_fleet_gauges();

    // Compute stats from Prometheus metrics
    let uptime_seconds = state.metrics_collector.uptime_seconds();
    let requests = compute_request_stats();
    let backends = compute_backend_stats(state.metrics_collector.registry());
    let models = compute_model_stats();

    let response = StatsResponse {
        uptime_seconds,
        requests,
        backends,
        models,
    };

    Json(response)
}

/// Compute aggregate request statistics from Prometheus metrics.
fn compute_request_stats() -> RequestStats {
    // For now, we return zeros since we need to implement proper Prometheus parsing
    // This will be enhanced in later implementation
    // TODO: Parse prometheus metrics to get actual counts
    RequestStats {
        total: 0,
        success: 0,
        errors: 0,
    }
}

/// Compute per-backend statistics from Prometheus metrics and Registry.
fn compute_backend_stats(registry: &crate::registry::Registry) -> Vec<BackendStats> {
    // Get all backends from registry
    let backends = registry.get_all_backends();

    backends
        .into_iter()
        .map(|backend| {
            // TODO: Parse Prometheus for actual request counts and latencies
            // For now, return current state from registry
            BackendStats {
                id: backend.id.clone(),
                requests: backend
                    .total_requests
                    .load(std::sync::atomic::Ordering::SeqCst),
                average_latency_ms: backend
                    .avg_latency_ms
                    .load(std::sync::atomic::Ordering::SeqCst) as f64,
                pending: backend
                    .pending_requests
                    .load(std::sync::atomic::Ordering::SeqCst) as usize,
            }
        })
        .collect()
}

/// Compute per-model statistics from Prometheus metrics.
fn compute_model_stats() -> Vec<ModelStats> {
    // TODO: Parse Prometheus metrics for per-model request counts and durations
    // For now, return empty until we implement proper parsing
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_request_stats_stub() {
        let stats = compute_request_stats();
        assert_eq!(stats.total, 0);
        assert_eq!(stats.success, 0);
        assert_eq!(stats.errors, 0);
    }

    #[test]
    fn test_compute_backend_stats_empty() {
        let registry = crate::registry::Registry::new();
        let stats = compute_backend_stats(&registry);
        assert_eq!(stats.len(), 0);
    }

    #[test]
    fn test_compute_model_stats_stub() {
        let stats = compute_model_stats();
        assert_eq!(stats.len(), 0);
    }
}
