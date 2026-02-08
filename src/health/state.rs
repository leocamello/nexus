//! Per-backend health state tracking.

use super::config::HealthCheckConfig;
use crate::registry::{BackendStatus, Model};
use chrono::{DateTime, Utc};

/// Tracks health check state for a single backend.
#[derive(Debug, Clone)]
pub struct BackendHealthState {
    /// Count of consecutive failed checks
    pub consecutive_failures: u32,
    /// Count of consecutive successful checks
    pub consecutive_successes: u32,
    /// When last check completed
    pub last_check_time: Option<DateTime<Utc>>,
    /// Last known status (for detecting transitions)
    pub last_status: BackendStatus,
    /// Last known model list (preserved on parse errors)
    pub last_models: Vec<Model>,
}

impl Default for BackendHealthState {
    fn default() -> Self {
        Self {
            consecutive_failures: 0,
            consecutive_successes: 0,
            last_check_time: None,
            last_status: BackendStatus::Unknown,
            last_models: Vec::new(),
        }
    }
}

/// Result of a health check
#[derive(Debug, Clone)]
pub enum HealthCheckResult {
    /// Backend responded successfully with valid model list
    Success { latency_ms: u32, models: Vec<Model> },
    /// Backend responded with HTTP 200 but invalid/unparseable JSON.
    /// Treated as healthy (backend is responding) but models are preserved from last check.
    SuccessWithParseError {
        latency_ms: u32,
        parse_error: String,
    },
    /// Backend failed to respond or returned error status
    Failure {
        error: super::error::HealthCheckError,
    },
}

impl BackendHealthState {
    /// Apply a health check result and determine if status should transition.
    /// Returns Some(new_status) if transition should occur, None otherwise.
    pub fn apply_result(
        &mut self,
        result: &HealthCheckResult,
        config: &HealthCheckConfig,
    ) -> Option<BackendStatus> {
        match result {
            // Both Success and SuccessWithParseError count as successful health checks
            // (backend is responding, even if JSON is malformed)
            HealthCheckResult::Success { .. } | HealthCheckResult::SuccessWithParseError { .. } => {
                self.consecutive_failures = 0;
                self.consecutive_successes += 1;

                match self.last_status {
                    BackendStatus::Unknown => Some(BackendStatus::Healthy),
                    BackendStatus::Unhealthy
                        if self.consecutive_successes >= config.recovery_threshold =>
                    {
                        Some(BackendStatus::Healthy)
                    }
                    _ => None,
                }
            }
            HealthCheckResult::Failure { .. } => {
                self.consecutive_successes = 0;
                self.consecutive_failures += 1;

                match self.last_status {
                    BackendStatus::Unknown => Some(BackendStatus::Unhealthy),
                    BackendStatus::Healthy
                        if self.consecutive_failures >= config.failure_threshold =>
                    {
                        Some(BackendStatus::Unhealthy)
                    }
                    _ => None,
                }
            }
        }
    }
}
