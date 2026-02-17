//! # Metrics Types
//!
//! Data structures for JSON stats API responses.

use serde::Serialize;

/// JSON response for GET /v1/stats endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct StatsResponse {
    /// Gateway uptime in seconds since startup
    pub uptime_seconds: u64,
    /// Aggregate request statistics
    pub requests: RequestStats,
    /// Per-backend breakdown
    pub backends: Vec<BackendStats>,
    /// Per-model breakdown
    pub models: Vec<ModelStats>,
    /// Budget management statistics (F14 - optional, present when budget is configured)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<BudgetStats>,
}

/// Aggregate request statistics.
#[derive(Debug, Clone, Serialize)]
pub struct RequestStats {
    /// Total requests processed
    pub total: u64,
    /// Successful requests (2xx status)
    pub success: u64,
    /// Failed requests (errors)
    pub errors: u64,
}

/// Per-backend statistics.
#[derive(Debug, Clone, Serialize)]
pub struct BackendStats {
    /// Backend identifier
    pub id: String,
    /// Backend display name
    pub name: String,
    /// Total requests sent to this backend
    pub requests: u64,
    /// Average latency in milliseconds
    pub average_latency_ms: f64,
    /// Current pending requests (queue depth)
    pub pending: usize,
    /// Error rate over last 1 hour (0.0–1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_rate_1h: Option<f32>,
    /// Average time to first token in milliseconds (last 1 hour)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_ttft_ms: Option<u32>,
    /// Success rate over last 24 hours (0.0–1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success_rate_24h: Option<f32>,
}

/// Per-model statistics.
#[derive(Debug, Clone, Serialize)]
pub struct ModelStats {
    /// Model name
    pub name: String,
    /// Total requests for this model
    pub requests: u64,
    /// Average request duration in milliseconds
    pub average_duration_ms: f64,
}

/// Budget management statistics (F14).
///
/// Provides real-time visibility into inference budget status including:
/// - Current spending and monthly limit
/// - Utilization percentage and enforcement status
/// - Billing cycle information and next reset date
/// - Configured soft limit threshold and hard limit action
#[derive(Debug, Clone, Serialize)]
pub struct BudgetStats {
    /// Current month spending in USD
    pub current_spending_usd: f64,
    /// Configured monthly budget limit in USD (null means no limit)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monthly_limit_usd: Option<f64>,
    /// Budget utilization percentage (can exceed 100%)
    pub utilization_percent: f64,
    /// Current budget enforcement status
    pub status: String,
    /// Current billing month in YYYY-MM format
    pub billing_month: String,
    /// ISO 8601 timestamp of last budget reconciliation
    pub last_reconciliation: String,
    /// Soft limit threshold percentage (from config)
    pub soft_limit_threshold: f64,
    /// Action taken when hard limit is reached (from config)
    pub hard_limit_action: String,
    /// Next billing cycle reset date (first day of next month, optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_reset_date: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_response_serialization() {
        let response = StatsResponse {
            uptime_seconds: 3600,
            requests: RequestStats {
                total: 1000,
                success: 950,
                errors: 50,
            },
            backends: vec![BackendStats {
                id: "ollama-local".to_string(),
                name: "ollama-local".to_string(),
                requests: 500,
                average_latency_ms: 1250.5,
                pending: 2,
                error_rate_1h: None,
                avg_ttft_ms: None,
                success_rate_24h: None,
            }],
            models: vec![ModelStats {
                name: "llama3:70b".to_string(),
                requests: 300,
                average_duration_ms: 5000.0,
            }],
            budget: None,
        };

        let json = serde_json::to_string(&response).expect("Failed to serialize");
        assert!(json.contains("uptime_seconds"));
        assert!(json.contains("3600"));
        assert!(json.contains("ollama-local"));
        assert!(json.contains("llama3:70b"));
        // budget should not appear in JSON when None
        assert!(!json.contains("budget"));
    }

    #[test]
    fn test_budget_stats_serialization() {
        let budget = BudgetStats {
            current_spending_usd: 42.50,
            monthly_limit_usd: Some(100.0),
            utilization_percent: 42.5,
            status: "Normal".to_string(),
            billing_month: "2026-02".to_string(),
            last_reconciliation: "2026-02-16T21:00:00Z".to_string(),
            soft_limit_threshold: 75.0,
            hard_limit_action: "warn".to_string(),
            next_reset_date: Some("2026-03-01".to_string()),
        };

        let json = serde_json::to_string(&budget).expect("Failed to serialize");
        assert!(json.contains("42.5"));
        assert!(json.contains("\"status\":\"Normal\""));
        assert!(json.contains("2026-02"));
        assert!(json.contains("soft_limit_threshold"));
    }

    #[test]
    fn test_budget_stats_no_limit() {
        let budget = BudgetStats {
            current_spending_usd: 0.0,
            monthly_limit_usd: None,
            utilization_percent: 0.0,
            status: "Normal".to_string(),
            billing_month: "2026-02".to_string(),
            last_reconciliation: "2026-02-16T21:00:00Z".to_string(),
            soft_limit_threshold: 75.0,
            hard_limit_action: "warn".to_string(),
            next_reset_date: None,
        };

        let json = serde_json::to_string(&budget).expect("Failed to serialize");
        // monthly_limit_usd and next_reset_date should be omitted when None
        assert!(!json.contains("monthly_limit_usd"));
        assert!(!json.contains("next_reset_date"));
    }

    #[test]
    fn test_stats_response_with_budget() {
        let response = StatsResponse {
            uptime_seconds: 100,
            requests: RequestStats {
                total: 10,
                success: 10,
                errors: 0,
            },
            backends: vec![],
            models: vec![],
            budget: Some(BudgetStats {
                current_spending_usd: 80.0,
                monthly_limit_usd: Some(100.0),
                utilization_percent: 80.0,
                status: "SoftLimit".to_string(),
                billing_month: "2026-02".to_string(),
                last_reconciliation: "2026-02-16T21:00:00Z".to_string(),
                soft_limit_threshold: 75.0,
                hard_limit_action: "block_cloud".to_string(),
                next_reset_date: Some("2026-03-01".to_string()),
            }),
        };

        let json = serde_json::to_string(&response).expect("Failed to serialize");
        assert!(json.contains("budget"));
        assert!(json.contains("SoftLimit"));
        assert!(json.contains("80.0"));
    }
}
