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
}
