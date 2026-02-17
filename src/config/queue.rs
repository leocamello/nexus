//! Request queue configuration

use serde::{Deserialize, Serialize};

/// Configuration for request queuing when backends are saturated.
///
/// The request queue holds incoming requests when all backends are at capacity,
/// preventing immediate 503 rejections for burst traffic.
///
/// # Example
///
/// ```toml
/// [queue]
/// enabled = true
/// max_size = 100
/// max_wait_seconds = 30
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QueueConfig {
    /// Whether request queuing is enabled.
    ///
    /// Default: true
    /// When false, saturated requests immediately return 503.
    pub enabled: bool,

    /// Maximum number of queued requests.
    ///
    /// Default: 100
    /// When max_size is 0, queuing is disabled (equivalent to enabled=false).
    /// When queue is full, new requests immediately return 503.
    pub max_size: u32,

    /// Maximum wait time for queued requests in seconds.
    ///
    /// Default: 30 seconds
    /// Requests exceeding this timeout return 503 with retry_after header.
    pub max_wait_seconds: u64,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_size: 100,
            max_wait_seconds: 30,
        }
    }
}

impl QueueConfig {
    /// Check if queuing is effectively enabled.
    ///
    /// Queuing is disabled if either enabled=false or max_size=0.
    pub fn is_enabled(&self) -> bool {
        self.enabled && self.max_size > 0
    }
}
