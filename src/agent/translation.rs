//! API format translation types and errors.
//!
//! Translation between OpenAI format and cloud provider formats (Anthropic Messages API,
//! Google Generative AI) is implemented directly in each agent:
//!
//! - `AnthropicAgent` (`src/agent/anthropic.rs`): OpenAI ↔ Anthropic Messages API v1
//! - `GoogleAIAgent` (`src/agent/google.rs`): OpenAI ↔ Google Generative AI API
//!
//! This module provides shared error types used by agents during translation.

use thiserror::Error;

/// Translation errors.
#[derive(Debug, Error)]
pub enum TranslationError {
    #[error("Unsupported message role: {0}")]
    UnsupportedRole(String),

    #[error("Missing required field: {0}")]
    MissingField(&'static str),

    #[error("Invalid format: {0}")]
    InvalidFormat(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}
