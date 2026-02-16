//! API format translation between OpenAI and cloud provider formats.
//!
//! This module provides bidirectional translation for chat completion requests
//! and responses between OpenAI's format (Nexus's API standard) and provider-specific
//! formats (Anthropic Messages API, Google Generative AI).
//!
//! ## Translation Requirements
//!
//! - **No data loss**: All message content must be preserved
//! - **Role mapping**: Convert between OpenAI and provider role systems
//! - **Streaming**: Support both non-streaming and streaming responses
//! - **OpenAI compatibility**: Translated responses must be byte-identical to native OpenAI format
//!
//! ## Supported Providers
//!
//! - **Anthropic**: Messages API v1 (system message as parameter, SSE streaming)
//! - **Google**: Generative AI API (contents/parts structure, newline-delimited JSON streaming)
//!
//! ## Example Usage
//!
//! ```rust
//! use nexus::agent::translation::AnthropicTranslator;
//!
//! let translator = AnthropicTranslator::new();
//! let anthropic_req = translator.openai_to_anthropic(openai_request)?;
//! // Send to Anthropic API...
//! let openai_resp = translator.anthropic_to_openai(anthropic_response)?;
//! ```

use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};
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

/// Anthropic-specific translation (stub for Phase 1).
///
/// Full implementation in Phase 6 (User Story 3).
pub struct AnthropicTranslator;

impl AnthropicTranslator {
    pub fn new() -> Self {
        Self
    }

    /// Translate OpenAI request to Anthropic Messages API format.
    ///
    /// **Phase 1 stub**: Returns error. Implemented in T084.
    pub fn openai_to_anthropic(
        &self,
        _req: ChatCompletionRequest,
    ) -> Result<serde_json::Value, TranslationError> {
        Err(TranslationError::InvalidFormat(
            "Anthropic translation not yet implemented (Phase 6)".to_string(),
        ))
    }

    /// Translate Anthropic response to OpenAI format.
    ///
    /// **Phase 1 stub**: Returns error. Implemented in T085.
    pub fn anthropic_to_openai(
        &self,
        _resp: serde_json::Value,
    ) -> Result<ChatCompletionResponse, TranslationError> {
        Err(TranslationError::InvalidFormat(
            "Anthropic translation not yet implemented (Phase 6)".to_string(),
        ))
    }

    /// Translate Anthropic streaming chunk to OpenAI format.
    ///
    /// **Phase 1 stub**: Returns error. Implemented in T086.
    pub fn translate_stream_chunk(
        &self,
        _chunk: &[u8],
    ) -> Result<Vec<String>, TranslationError> {
        Err(TranslationError::InvalidFormat(
            "Anthropic streaming translation not yet implemented (Phase 6)".to_string(),
        ))
    }
}

impl Default for AnthropicTranslator {
    fn default() -> Self {
        Self::new()
    }
}

/// Google-specific translation (stub for Phase 1).
///
/// Full implementation in Phase 6 (User Story 3).
pub struct GoogleTranslator;

impl GoogleTranslator {
    pub fn new() -> Self {
        Self
    }

    /// Translate OpenAI request to Google Generative AI format.
    ///
    /// **Phase 1 stub**: Returns error. Implemented in T099.
    pub fn openai_to_google(
        &self,
        _req: ChatCompletionRequest,
    ) -> Result<serde_json::Value, TranslationError> {
        Err(TranslationError::InvalidFormat(
            "Google translation not yet implemented (Phase 6)".to_string(),
        ))
    }

    /// Translate Google response to OpenAI format.
    ///
    /// **Phase 1 stub**: Returns error. Implemented in T100.
    pub fn google_to_openai(
        &self,
        _resp: serde_json::Value,
    ) -> Result<ChatCompletionResponse, TranslationError> {
        Err(TranslationError::InvalidFormat(
            "Google translation not yet implemented (Phase 6)".to_string(),
        ))
    }

    /// Translate Google streaming chunk to OpenAI format.
    ///
    /// **Phase 1 stub**: Returns error. Implemented in T101.
    pub fn translate_stream_chunk(
        &self,
        _chunk: &[u8],
    ) -> Result<Vec<String>, TranslationError> {
        Err(TranslationError::InvalidFormat(
            "Google streaming translation not yet implemented (Phase 6)".to_string(),
        ))
    }
}

impl Default for GoogleTranslator {
    fn default() -> Self {
        Self::new()
    }
}
