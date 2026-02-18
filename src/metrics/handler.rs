//! # Metrics HTTP Handlers
//!
//! Axum handlers for metrics endpoints.

use super::{BackendStats, BudgetStats, ModelStats, RequestStats, StatsResponse};
use crate::api::AppState;
use crate::config::routing::HardLimitAction;
use crate::routing::reconciler::budget::GLOBAL_BUDGET_KEY;
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

    let registry = state.metrics_collector.registry();

    // Compute stats from Registry atomics
    let uptime_seconds = state.metrics_collector.uptime_seconds();
    let quality_store = state.router.quality_store();
    let backends = compute_backend_stats(registry, Some(quality_store));
    let requests = compute_request_stats(&backends);
    let models = compute_model_stats(registry);
    let budget = compute_budget_stats(&state);

    let response = StatsResponse {
        uptime_seconds,
        requests,
        backends,
        models,
        budget,
    };

    Json(response)
}

/// Compute aggregate request statistics from Prometheus metrics.
/// Compute aggregate request statistics by summing backend totals.
pub fn compute_request_stats(backends: &[BackendStats]) -> RequestStats {
    let total: u64 = backends.iter().map(|b| b.requests).sum();
    RequestStats {
        total,
        success: total,
        errors: 0,
    }
}

/// Compute per-backend statistics from Prometheus metrics and Registry.
pub fn compute_backend_stats(
    registry: &crate::registry::Registry,
    quality_store: Option<&crate::agent::quality::QualityMetricsStore>,
) -> Vec<BackendStats> {
    // Get all backends from registry
    let backends = registry.get_all_backends();

    backends
        .into_iter()
        .map(|backend| {
            let (error_rate_1h, avg_ttft_ms, success_rate_24h) = if let Some(store) = quality_store
            {
                let m = store.get_metrics(&backend.id);
                if m.request_count_1h > 0 || m.last_failure_ts.is_some() {
                    (
                        Some(m.error_rate_1h),
                        Some(m.avg_ttft_ms),
                        Some(m.success_rate_24h),
                    )
                } else {
                    (None, None, None)
                }
            } else {
                (None, None, None)
            };

            // Backend stats sourced from Registry atomics (real-time values)
            BackendStats {
                id: backend.id.clone(),
                name: backend.name.clone(),
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
                error_rate_1h,
                avg_ttft_ms,
                success_rate_24h,
            }
        })
        .collect()
}

/// Compute per-model statistics from Prometheus metrics.
/// Compute per-model statistics from Registry.
pub fn compute_model_stats(registry: &crate::registry::Registry) -> Vec<ModelStats> {
    let backends = registry.get_all_backends();
    let mut model_names = std::collections::HashSet::new();

    for backend in &backends {
        if backend.status == crate::registry::BackendStatus::Healthy {
            for model in &backend.models {
                model_names.insert(model.name.clone());
            }
        }
    }

    model_names
        .into_iter()
        .map(|name| ModelStats {
            name,
            requests: 0,
            average_duration_ms: 0.0,
        })
        .collect()
}

/// Compute budget statistics from router state (F14).
///
/// Returns budget statistics if a monthly limit is configured, None otherwise.
/// This provides real-time visibility into spending, utilization, and enforcement status.
pub fn compute_budget_stats(state: &AppState) -> Option<BudgetStats> {
    let router = &state.router;
    let budget_config = router.budget_config();
    let budget_state = router.budget_state();

    // Only return budget stats if a monthly limit is configured
    let monthly_limit_usd = budget_config.monthly_limit_usd?;

    // Get current spending from global budget key
    let metrics = budget_state.get(GLOBAL_BUDGET_KEY)?;
    let current_spending_usd = metrics.current_month_spending;
    let billing_month = metrics.month_key.clone();
    let last_reconciliation = metrics.last_reconciliation_time.to_rfc3339();

    // Calculate utilization percentage
    let utilization_percent = if monthly_limit_usd > 0.0 {
        (current_spending_usd / monthly_limit_usd) * 100.0
    } else {
        0.0
    };

    // Determine status
    let status = if utilization_percent >= 100.0 {
        "HardLimit"
    } else if utilization_percent >= budget_config.soft_limit_percent {
        "SoftLimit"
    } else {
        "Normal"
    };

    // Convert hard limit action to string
    let hard_limit_action = match budget_config.hard_limit_action {
        HardLimitAction::Warn => "Warn",
        HardLimitAction::BlockCloud => "BlockCloud",
        HardLimitAction::BlockAll => "BlockAll",
    };

    // Calculate next reset date (first day of next month)
    let next_reset_date =
        chrono::NaiveDate::parse_from_str(&format!("{}-01", billing_month), "%Y-%m-%d")
            .ok()
            .and_then(|date| {
                // Add one month
                date.checked_add_months(chrono::Months::new(1))
            })
            .map(|date| date.format("%Y-%m-%d").to_string());

    Some(BudgetStats {
        current_spending_usd,
        monthly_limit_usd: Some(monthly_limit_usd),
        utilization_percent,
        status: status.to_string(),
        billing_month,
        last_reconciliation,
        soft_limit_threshold: budget_config.soft_limit_percent,
        hard_limit_action: hard_limit_action.to_string(),
        next_reset_date,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_request_stats_empty() {
        let stats = compute_request_stats(&[]);
        assert_eq!(stats.total, 0);
        assert_eq!(stats.success, 0);
        assert_eq!(stats.errors, 0);
    }

    #[test]
    fn test_compute_request_stats_sums_backends() {
        let backends = vec![
            BackendStats {
                id: "b1".to_string(),
                name: "backend-1".to_string(),
                requests: 10,
                average_latency_ms: 5.0,
                pending: 0,
                error_rate_1h: None,
                avg_ttft_ms: None,
                success_rate_24h: None,
            },
            BackendStats {
                id: "b2".to_string(),
                name: "backend-2".to_string(),
                requests: 20,
                average_latency_ms: 3.0,
                pending: 1,
                error_rate_1h: None,
                avg_ttft_ms: None,
                success_rate_24h: None,
            },
        ];
        let stats = compute_request_stats(&backends);
        assert_eq!(stats.total, 30);
    }

    #[test]
    fn test_compute_backend_stats_empty() {
        let registry = crate::registry::Registry::new();
        let stats = compute_backend_stats(&registry, None);
        assert_eq!(stats.len(), 0);
    }

    #[test]
    fn test_compute_model_stats_empty() {
        let registry = crate::registry::Registry::new();
        let stats = compute_model_stats(&registry);
        assert_eq!(stats.len(), 0);
    }

    fn make_model(name: &str) -> crate::registry::Model {
        crate::registry::Model {
            id: name.to_string(),
            name: name.to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }
    }

    fn make_backend(
        id: &str,
        name: &str,
        models: Vec<crate::registry::Model>,
    ) -> crate::registry::Backend {
        let mut backend = crate::registry::Backend::new(
            id.to_string(),
            name.to_string(),
            "http://localhost:11434".to_string(),
            crate::registry::BackendType::Ollama,
            models,
            crate::registry::DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        backend.status = crate::registry::BackendStatus::Healthy;
        backend
    }

    #[test]
    fn test_compute_model_stats_single_backend() {
        let registry = crate::registry::Registry::new();
        let backend = make_backend(
            "b1",
            "backend-1",
            vec![make_model("llama3"), make_model("mistral")],
        );
        registry.add_backend(backend).unwrap();

        let stats = compute_model_stats(&registry);
        assert_eq!(stats.len(), 2);
        let mut names: Vec<_> = stats.iter().map(|s| s.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["llama3", "mistral"]);
    }

    #[test]
    fn test_compute_model_stats_deduplicates_across_backends() {
        let registry = crate::registry::Registry::new();
        let b1 = make_backend(
            "b1",
            "backend-1",
            vec![make_model("llama3"), make_model("mistral")],
        );
        let b2 = make_backend(
            "b2",
            "backend-2",
            vec![make_model("llama3"), make_model("phi3")],
        );
        registry.add_backend(b1).unwrap();
        registry.add_backend(b2).unwrap();

        let stats = compute_model_stats(&registry);
        assert_eq!(stats.len(), 3);
        let mut names: Vec<_> = stats.iter().map(|s| s.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["llama3", "mistral", "phi3"]);
    }

    #[test]
    fn test_compute_model_stats_excludes_unhealthy_backends() {
        let registry = crate::registry::Registry::new();
        let healthy = make_backend("b1", "healthy", vec![make_model("llama3")]);
        let mut unhealthy = make_backend("b2", "unhealthy", vec![make_model("mistral")]);
        unhealthy.status = crate::registry::BackendStatus::Unhealthy;
        registry.add_backend(healthy).unwrap();
        registry.add_backend(unhealthy).unwrap();

        let stats = compute_model_stats(&registry);
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].name, "llama3");
    }

    #[test]
    fn test_compute_backend_stats_single_backend() {
        let registry = crate::registry::Registry::new();
        let backend = make_backend("b1", "backend-1", vec![make_model("llama3")]);
        backend
            .total_requests
            .store(42, std::sync::atomic::Ordering::SeqCst);
        backend
            .avg_latency_ms
            .store(15, std::sync::atomic::Ordering::SeqCst);
        backend
            .pending_requests
            .store(3, std::sync::atomic::Ordering::SeqCst);
        registry.add_backend(backend).unwrap();

        let stats = compute_backend_stats(&registry, None);
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].id, "b1");
        assert_eq!(stats[0].name, "backend-1");
        assert_eq!(stats[0].requests, 42);
        assert_eq!(stats[0].average_latency_ms, 15.0);
        assert_eq!(stats[0].pending, 3);
    }

    #[test]
    fn test_compute_request_stats_single_backend_100() {
        let backends = vec![BackendStats {
            id: "b1".to_string(),
            name: "backend-1".to_string(),
            requests: 100,
            average_latency_ms: 10.0,
            pending: 0,
            error_rate_1h: None,
            avg_ttft_ms: None,
            success_rate_24h: None,
        }];
        let stats = compute_request_stats(&backends);
        assert_eq!(stats.total, 100);
        assert_eq!(stats.success, 100);
        assert_eq!(stats.errors, 0);
    }

    #[test]
    fn test_compute_request_stats_three_backends() {
        let backends = vec![
            BackendStats {
                id: "b1".to_string(),
                name: "backend-1".to_string(),
                requests: 50,
                average_latency_ms: 5.0,
                pending: 0,
                error_rate_1h: None,
                avg_ttft_ms: None,
                success_rate_24h: None,
            },
            BackendStats {
                id: "b2".to_string(),
                name: "backend-2".to_string(),
                requests: 120,
                average_latency_ms: 8.0,
                pending: 2,
                error_rate_1h: None,
                avg_ttft_ms: None,
                success_rate_24h: None,
            },
            BackendStats {
                id: "b3".to_string(),
                name: "backend-3".to_string(),
                requests: 30,
                average_latency_ms: 3.0,
                pending: 1,
                error_rate_1h: None,
                avg_ttft_ms: None,
                success_rate_24h: None,
            },
        ];
        let stats = compute_request_stats(&backends);
        assert_eq!(stats.total, 200);
        assert_eq!(stats.success, 200);
        assert_eq!(stats.errors, 0);
    }

    #[test]
    fn test_compute_budget_stats_no_config() {
        use crate::config::NexusConfig;
        use crate::registry::Registry;

        let registry = Arc::new(Registry::new());
        let config = Arc::new(NexusConfig::default());
        let state = AppState::new(registry, config);

        let result = compute_budget_stats(&state);
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_stats_handler_returns_json() {
        use crate::config::NexusConfig;
        use crate::registry::Registry;
        use axum::response::IntoResponse;

        let registry = Arc::new(Registry::new());
        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(registry, config));

        let response = stats_handler(State(state)).await.into_response();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_metrics_handler_returns_text() {
        use crate::config::NexusConfig;
        use crate::registry::Registry;
        use axum::response::IntoResponse;

        let registry = Arc::new(Registry::new());
        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(registry, config));

        let response = metrics_handler(State(state)).await.into_response();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }
}
