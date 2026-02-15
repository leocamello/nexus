//! Error types for health checking.

use thiserror::Error;

/// Errors that can occur during health checking.
#[derive(Debug, Clone, Error)]
pub enum HealthCheckError {
    /// Request timeout
    #[error("request timeout after {0}s")]
    Timeout(u64),

    /// Connection failed
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    /// DNS resolution failed
    #[error("DNS resolution failed: {0}")]
    DnsError(String),

    /// TLS certificate error
    #[error("TLS certificate error: {0}")]
    TlsError(String),

    /// HTTP error
    #[error("HTTP error: {0}")]
    HttpError(u16),

    /// Invalid response
    #[error("invalid response: {0}")]
    ParseError(String),

    /// Agent error (T034)
    #[error("agent error: {0}")]
    AgentError(String),
}

impl HealthCheckError {
    /// Convert AgentError to HealthCheckError (T034)
    pub fn from_agent_error(error: crate::agent::AgentError) -> Self {
        match error {
            crate::agent::AgentError::Network(msg) => Self::ConnectionFailed(msg),
            crate::agent::AgentError::Timeout(ms) => {
                // Convert milliseconds to seconds (round up)
                Self::Timeout(ms.div_ceil(1000))
            }
            crate::agent::AgentError::Upstream { status, message: _ } => {
                Self::HttpError(status)
            }
            crate::agent::AgentError::InvalidResponse(msg) => Self::ParseError(msg),
            crate::agent::AgentError::Unsupported(msg) => Self::AgentError(msg.to_string()),
            crate::agent::AgentError::Configuration(msg) => Self::AgentError(msg),
        }
    }
}
