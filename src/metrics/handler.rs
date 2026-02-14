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
/// Always returns 200 with the correct Content-Type for Prometheus scrapers,
/// even if no metrics have been recorded yet (returns empty text).
pub async fn metrics_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Update fleet gauges before rendering
    state.metrics_collector.update_fleet_gauges();

    // Get Prometheus text format from collector
    let metrics = state.metrics_collector.render_metrics();
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        metrics,
    )
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
pub fn compute_request_stats() -> RequestStats {
    // Request stats require parsing Prometheus text format to extract counter values.
    // The `metrics` crate records metrics but doesn't provide a query API.
    // Use GET /metrics (Prometheus format) for accurate request counts.
    // Enhancement: parse PrometheusHandle::render() output to populate this.
    RequestStats {
        total: 0,
        success: 0,
        errors: 0,
    }
}

/// Compute per-backend statistics from Prometheus metrics and Registry.
pub fn compute_backend_stats(registry: &crate::registry::Registry) -> Vec<BackendStats> {
    // Get all backends from registry
    let backends = registry.get_all_backends();

    backends
        .into_iter()
        .map(|backend| {
            // Backend stats sourced from Registry atomics (real-time values)
            BackendStats {
                id: backend.id.clone(),
                requests: backend
                    .total_requests
                    .load(std::sync::atomic::Ordering::SeqCst),
                average_latency_ms: backend
                    .avg_latency_ms
                    .load(std::sync::atomic::Ordering::SeqCst)
                    as f64,
                pending: backend
                    .pending_requests
                    .load(std::sync::atomic::Ordering::SeqCst) as usize,
            }
        })
        .collect()
}

/// Compute per-model statistics from Prometheus metrics.
pub fn compute_model_stats() -> Vec<ModelStats> {
    // Per-model stats require parsing Prometheus text format to extract histogram data.
    // Use GET /metrics (Prometheus format) for per-model request counts and durations.
    // Enhancement: parse PrometheusHandle::render() output to populate this.
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
