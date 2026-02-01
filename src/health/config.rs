//! Configuration for health checking.

use serde::{Deserialize, Serialize};

/// Configuration for backend health checking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HealthCheckConfig {
    /// Whether health checking is enabled
    pub enabled: bool,
    /// Seconds between health check cycles
    pub interval_seconds: u64,
    /// Timeout for each health check request
    pub timeout_seconds: u64,
    /// Consecutive failures before marking unhealthy
    pub failure_threshold: u32,
    /// Consecutive successes before marking healthy
    pub recovery_threshold: u32,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: 30,
            timeout_seconds: 5,
            failure_threshold: 3,
            recovery_threshold: 2,
        }
    }
}
