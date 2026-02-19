//! Fleet intelligence configuration

use serde::{Deserialize, Serialize};

/// Configuration for fleet intelligence and pre-warming recommendations.
///
/// Controls pattern analysis parameters and recommendation thresholds.
///
/// # Example
///
/// ```toml
/// [fleet]
/// enabled = true
/// min_sample_days = 7
/// min_request_count = 100
/// analysis_interval_seconds = 3600
/// max_recommendations = 5
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FleetConfig {
    /// Whether fleet intelligence analysis is enabled.
    ///
    /// Default: false
    /// When false, no pattern analysis or recommendations are generated.
    pub enabled: bool,

    /// Minimum number of days of request history required for pattern analysis.
    ///
    /// Default: 7 days
    /// Patterns with less historical data are considered insufficient for recommendations.
    pub min_sample_days: u32,

    /// Minimum request count required for a model to be analyzed.
    ///
    /// Default: 100 requests
    /// Models with fewer requests are excluded from pattern analysis.
    pub min_request_count: u32,

    /// Interval between fleet analysis runs in seconds.
    ///
    /// Default: 3600 seconds (1 hour)
    /// Background analysis loop runs at this interval.
    pub analysis_interval_seconds: u64,

    /// Maximum number of recommendations to generate per analysis cycle.
    ///
    /// Default: 5
    /// Limits recommendation output to top N suggestions by confidence score.
    pub max_recommendations: u32,
}

impl Default for FleetConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_sample_days: 7,
            min_request_count: 100,
            analysis_interval_seconds: 3600,
            max_recommendations: 5,
        }
    }
}
