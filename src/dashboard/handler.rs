//! HTTP handlers for dashboard routes

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::RustEmbed;
use std::sync::Arc;
use std::time::Duration;

use crate::api::AppState;

/// System summary data for dashboard header
#[derive(Debug, Clone, serde::Serialize)]
pub struct SystemSummary {
    /// Server uptime in seconds
    pub uptime_seconds: u64,
    /// Total number of requests processed
    pub total_requests: u64,
}

/// Embedded dashboard assets from dashboard/ directory
#[derive(RustEmbed)]
#[folder = "dashboard/"]
struct DashboardAssets;

/// Serves the main dashboard HTML page with injected initial data
pub async fn dashboard_handler(State(state): State<Arc<AppState>>) -> Response {
    match DashboardAssets::get("index.html") {
        Some(content) => {
            let body = content.data;
            let html = match std::str::from_utf8(&body) {
                Ok(html) => html,
                Err(_) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, "Invalid HTML encoding")
                        .into_response()
                }
            };

            // Generate initial stats data
            state.metrics_collector.update_fleet_gauges();
            let registry = state.metrics_collector.registry();
            let backend_stats = crate::metrics::handler::compute_backend_stats(registry);
            let stats = crate::metrics::types::StatsResponse {
                uptime_seconds: state.metrics_collector.uptime_seconds(),
                requests: crate::metrics::handler::compute_request_stats(&backend_stats),
                backends: backend_stats,
                models: crate::metrics::handler::compute_model_stats(registry),
            };
            let stats_json = serde_json::to_string(&stats).unwrap_or_else(|_| "{}".to_string());

            // Generate initial backend views (full details for cards)
            let all_backends = state.registry.get_all_backends();
            let backend_views: Vec<crate::registry::BackendView> =
                all_backends.iter().map(|b| b.into()).collect();
            let backends_json =
                serde_json::to_string(&backend_views).unwrap_or_else(|_| "[]".to_string());

            // Generate initial models data
            let healthy_backends: Vec<_> = all_backends
                .into_iter()
                .filter(|b| b.status == crate::registry::BackendStatus::Healthy)
                .collect();

            let mut models_map = std::collections::HashMap::new();
            for backend in healthy_backends {
                for model in &backend.models {
                    models_map.entry(model.name.clone()).or_insert_with(|| {
                        crate::api::models::ModelObject {
                            id: model.name.clone(),
                            object: "model".to_string(),
                            created: 0,
                            owned_by: "nexus".to_string(),
                            context_length: Some(model.context_length),
                            capabilities: Some(crate::api::models::ModelCapabilities {
                                vision: model.supports_vision,
                                tools: model.supports_tools,
                                json_mode: model.supports_json_mode,
                            }),
                        }
                    });
                }
            }

            let models = crate::api::models::ModelsResponse {
                object: "list".to_string(),
                data: models_map.into_values().collect(),
            };
            let models_json = serde_json::to_string(&models).unwrap_or_else(|_| "{}".to_string());

            // Create initial data object with stats, models, and backends
            let initial_data = format!(
                r#"{{"stats":{}, "models":{}, "backends":{}}}"#,
                stats_json, models_json, backends_json
            );

            // Inject initial data into the HTML template
            let updated_html = html.replace(
                r#"<script id="initial-data" type="application/json">
        {}
    </script>"#,
                &format!(
                    r#"<script id="initial-data" type="application/json">
        {}
    </script>"#,
                    initial_data
                ),
            );

            Html(updated_html).into_response()
        }
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Dashboard HTML not found",
        )
            .into_response(),
    }
}

/// Serves request history data
pub async fn history_handler(State(state): State<Arc<AppState>>) -> Response {
    let entries = state.request_history.get_all();
    axum::Json(entries).into_response()
}

/// Serves static assets (CSS, JS, etc.)
pub async fn assets_handler(Path(path): Path<String>) -> Response {
    match DashboardAssets::get(&path) {
        Some(content) => {
            let body = content.data;
            let mime_type = mime_guess::from_path(&path).first_or_octet_stream();

            ([(header::CONTENT_TYPE, mime_type.as_ref())], body).into_response()
        }
        None => (StatusCode::NOT_FOUND, "Asset not found").into_response(),
    }
}

/// Calculate system uptime from start time
pub fn calculate_uptime(start_time: std::time::Instant) -> Duration {
    start_time.elapsed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_calculate_uptime() {
        // Create a start time 10 seconds ago
        let start_time = Instant::now() - Duration::from_secs(10);
        let uptime = calculate_uptime(start_time);

        // Uptime should be approximately 10 seconds (allow 1s margin for test execution)
        assert!(uptime.as_secs() >= 9 && uptime.as_secs() <= 11);
    }

    #[test]
    fn test_calculate_uptime_immediate() {
        // Create a start time right now
        let start_time = Instant::now();
        let uptime = calculate_uptime(start_time);

        // Uptime should be close to 0 (allow 1s margin)
        assert!(uptime.as_secs() <= 1);
    }

    #[test]
    fn test_system_summary_serialization() {
        let summary = SystemSummary {
            uptime_seconds: 3600,
            total_requests: 1234,
        };

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("3600"));
        assert!(json.contains("1234"));
    }
}
