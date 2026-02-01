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
}
