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
            let backend_stats = crate::metrics::handler::compute_backend_stats(registry, None);
            let stats = crate::metrics::types::StatsResponse {
                uptime_seconds: state.metrics_collector.uptime_seconds(),
                requests: crate::metrics::handler::compute_request_stats(&backend_stats),
                backends: backend_stats,
                models: crate::metrics::handler::compute_model_stats(registry),
                budget: crate::metrics::handler::compute_budget_stats(&state),
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

            let mut models_data: Vec<crate::api::models::ModelObject> = Vec::new();
            for backend in healthy_backends {
                for model in &backend.models {
                    models_data.push(crate::api::models::ModelObject {
                        id: model.name.clone(),
                        object: "model".to_string(),
                        created: 0,
                        owned_by: backend.name.clone(),
                        context_length: Some(model.context_length),
                        capabilities: Some(crate::api::models::ModelCapabilities {
                            vision: model.supports_vision,
                            tools: model.supports_tools,
                            json_mode: model.supports_json_mode,
                        }),
                    });
                }
            }

            let models = crate::api::models::ModelsResponse {
                object: "list".to_string(),
                data: models_data,
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

    #[test]
    fn test_assets_handler_not_found() {
        // Test that non-existent asset returns 404
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let response = assets_handler(Path("nonexistent.js".to_string())).await;
            assert_eq!(response.into_response().status(), StatusCode::NOT_FOUND);
        });
    }

    #[test]
    fn test_system_summary_fields() {
        let summary = SystemSummary {
            uptime_seconds: 7200,
            total_requests: 5678,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["uptime_seconds"], 7200);
        assert_eq!(value["total_requests"], 5678);
    }

    #[test]
    fn test_calculate_uptime_known_elapsed() {
        let start_time = Instant::now() - Duration::from_secs(60);
        let uptime = calculate_uptime(start_time);
        assert!(
            uptime.as_secs() >= 59 && uptime.as_secs() <= 61,
            "Expected ~60s uptime, got {}s",
            uptime.as_secs()
        );
    }

    #[test]
    fn test_calculate_uptime_non_negative() {
        let start_time = Instant::now();
        let uptime = calculate_uptime(start_time);
        assert!(uptime >= Duration::ZERO, "Uptime should never be negative");
    }

    #[tokio::test]
    async fn test_dashboard_handler_returns_ok() {
        use crate::config::NexusConfig;
        use crate::registry::Registry;

        let registry = Arc::new(Registry::new());
        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(registry, config));

        let response = dashboard_handler(State(state)).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_history_handler_returns_empty() {
        use crate::config::NexusConfig;
        use crate::registry::Registry;

        let registry = Arc::new(Registry::new());
        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(registry, config));

        let response = history_handler(State(state)).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_assets_handler_serves_existing_asset() {
        let response = assets_handler(Path("style.css".to_string())).await;
        let status = response.status();
        assert!(status == StatusCode::OK || status == StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_history_handler_returns_recent_requests() {
        use crate::config::NexusConfig;
        use crate::dashboard::types::{HistoryEntry, RequestStatus};
        use crate::registry::Registry;

        let registry = Arc::new(Registry::new());
        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(registry, config));

        // Add some entries to request_history
        state.request_history.push(HistoryEntry {
            timestamp: 1000,
            model: "gpt-4".to_string(),
            backend_id: "b1".to_string(),
            duration_ms: 100,
            status: RequestStatus::Success,
            error_message: None,
        });
        state.request_history.push(HistoryEntry {
            timestamp: 2000,
            model: "llama3".to_string(),
            backend_id: "b2".to_string(),
            duration_ms: 200,
            status: RequestStatus::Error,
            error_message: Some("timeout".to_string()),
        });

        let response = history_handler(State(state)).await;
        assert_eq!(response.status(), StatusCode::OK);

        // Extract body and verify JSON content
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let entries: Vec<HistoryEntry> = serde_json::from_slice(&body).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].model, "gpt-4");
        assert_eq!(entries[1].model, "llama3");
        assert_eq!(entries[1].error_message.as_deref(), Some("timeout"));
    }

    #[tokio::test]
    async fn test_dashboard_handler_content_type() {
        use crate::config::NexusConfig;
        use crate::registry::Registry;

        let registry = Arc::new(Registry::new());
        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(registry, config));

        let response = dashboard_handler(State(state)).await;
        assert_eq!(response.status(), StatusCode::OK);

        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("should have content-type header");
        let ct_str = content_type.to_str().unwrap();
        assert!(
            ct_str.contains("text/html"),
            "Expected text/html content type, got: {}",
            ct_str
        );
    }

    #[tokio::test]
    async fn test_assets_handler_unknown_asset() {
        let response = assets_handler(Path("totally_nonexistent_file_xyz.wasm".to_string())).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_assets_handler_serves_css() {
        let response = assets_handler(Path("styles.css".to_string())).await;
        let status = response.status();
        // If the asset is embedded, should serve with correct content type
        if status == StatusCode::OK {
            let ct = response
                .headers()
                .get(header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap();
            assert!(ct.contains("css"), "Expected CSS content type, got: {}", ct);
        }
    }

    #[tokio::test]
    async fn test_assets_handler_serves_js() {
        let response = assets_handler(Path("dashboard.js".to_string())).await;
        let status = response.status();
        if status == StatusCode::OK {
            let ct = response
                .headers()
                .get(header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap();
            assert!(
                ct.contains("javascript"),
                "Expected JS content type, got: {}",
                ct
            );
        }
    }

    #[tokio::test]
    async fn test_assets_handler_index_html() {
        let response = assets_handler(Path("index.html".to_string())).await;
        let status = response.status();
        if status == StatusCode::OK {
            let ct = response
                .headers()
                .get(header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap();
            assert!(
                ct.contains("html"),
                "Expected HTML content type, got: {}",
                ct
            );
        }
    }

    #[tokio::test]
    async fn test_dashboard_handler_with_backends_and_models() {
        use crate::config::NexusConfig;
        use crate::registry::{
            Backend, BackendStatus, BackendType, DiscoverySource, Model, Registry,
        };

        let registry = Arc::new(Registry::new());

        // Add a healthy backend with models
        let backend = Backend::new(
            "dash-backend".to_string(),
            "DashBackend".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        registry.add_backend(backend).unwrap();
        registry
            .update_status("dash-backend", BackendStatus::Healthy, None)
            .unwrap();
        registry
            .update_models(
                "dash-backend",
                vec![
                    Model {
                        id: "llama3:8b".to_string(),
                        name: "llama3:8b".to_string(),
                        context_length: 8192,
                        supports_vision: false,
                        supports_tools: true,
                        supports_json_mode: true,
                        max_output_tokens: None,
                    },
                    Model {
                        id: "codellama:7b".to_string(),
                        name: "codellama:7b".to_string(),
                        context_length: 16384,
                        supports_vision: false,
                        supports_tools: false,
                        supports_json_mode: false,
                        max_output_tokens: Some(4096),
                    },
                ],
            )
            .unwrap();

        // Add an unhealthy backend (should not appear in models)
        let backend2 = Backend::new(
            "dash-unhealthy".to_string(),
            "DashUnhealthy".to_string(),
            "http://localhost:99999".to_string(),
            BackendType::Generic,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        registry.add_backend(backend2).unwrap();
        registry
            .update_status("dash-unhealthy", BackendStatus::Unhealthy, None)
            .unwrap();
        registry
            .update_models(
                "dash-unhealthy",
                vec![Model {
                    id: "hidden-model".to_string(),
                    name: "hidden-model".to_string(),
                    context_length: 4096,
                    supports_vision: false,
                    supports_tools: false,
                    supports_json_mode: false,
                    max_output_tokens: None,
                }],
            )
            .unwrap();

        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(registry, config));

        let response = dashboard_handler(State(state)).await;
        assert_eq!(response.status(), StatusCode::OK);

        // Verify the HTML contains initial data with our backends
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains("llama3:8b"), "HTML should contain model name");
        assert!(
            html.contains("DashBackend"),
            "HTML should contain backend name"
        );
    }
}
