//! Quality tracking configuration

use serde::{Deserialize, Serialize};

/// Configuration for quality tracking and metrics.
///
/// Quality tracking monitors backend reliability (error rates, TTFT) and
/// enables the QualityReconciler to filter unreliable backends and the
/// SchedulerReconciler to penalize slow backends.
///
/// # Example
///
/// ```toml
/// [quality]
/// error_rate_threshold = 0.5
/// ttft_penalty_threshold_ms = 3000
/// metrics_interval_seconds = 30
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QualityConfig {
    /// Maximum acceptable error rate (1h window) before excluding an agent.
    ///
    /// Default: 0.5 (50% errors)
    /// Range: 0.0 (exclude any errors) to 1.0 (never exclude)
    pub error_rate_threshold: f32,

    /// TTFT threshold in milliseconds above which agents are penalized in scoring.
    ///
    /// Default: 3000ms (3 seconds)
    /// Agents with avg_ttft_ms above this value receive lower routing scores.
    pub ttft_penalty_threshold_ms: u32,

    /// Interval between quality metric computations in seconds.
    ///
    /// Default: 30 seconds
    /// The background quality loop runs at this interval to update metrics.
    pub metrics_interval_seconds: u64,
}

impl Default for QualityConfig {
    fn default() -> Self {
        Self {
            error_rate_threshold: 0.5,
            ttft_penalty_threshold_ms: 3000,
            metrics_interval_seconds: 30,
        }
    }
}
