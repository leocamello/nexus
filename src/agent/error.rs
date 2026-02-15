//! Error types for agent operations.

use thiserror::Error;

/// Errors that can occur during agent operations.
#[derive(Error, Debug)]
pub enum AgentError {
    /// Network connectivity error (DNS, connection refused, etc.).
    #[error("Network error: {0}")]
    Network(String),

    /// Request exceeded deadline.
    #[error("Request timeout after {0}ms")]
    Timeout(u64),

    /// Backend returned an error response (4xx, 5xx).
    #[error("Backend error {status}: {message}")]
    Upstream { status: u16, message: String },

    /// Method not supported by this agent implementation.
    #[error("Method '{0}' not supported by this agent")]
    Unsupported(&'static str),

    /// Backend response doesn't match expected format.
    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    /// Agent configuration error.
    #[error("Configuration error: {0}")]
    Configuration(String),
}
