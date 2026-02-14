//! Field extraction helpers for structured logging

use crate::api::{ApiError, ChatCompletionResponse};

/// Extract token counts from a chat completion response
///
/// Returns a tuple of (prompt_tokens, completion_tokens, total_tokens).
/// Returns (0, 0, 0) if usage information is not available.
///
/// # Examples
///
/// ```no_run
/// use nexus::logging::extract_tokens;
/// use nexus::api::ChatCompletionResponse;
///
/// let response = ChatCompletionResponse {
///     // ... response fields ...
/// #   id: "test".to_string(),
/// #   object: "chat.completion".to_string(),
/// #   created: 0,
/// #   model: "test".to_string(),
/// #   choices: vec![],
/// #   usage: None,
/// };
/// let (prompt, completion, total) = extract_tokens(&response);
/// ```
pub fn extract_tokens(response: &ChatCompletionResponse) -> (u32, u32, u32) {
    if let Some(usage) = &response.usage {
        (
            usage.prompt_tokens,
            usage.completion_tokens,
            usage.total_tokens,
        )
    } else {
        (0, 0, 0)
    }
}

/// Extract status and error message from a Result
///
/// Returns a tuple of (status, error_message).
/// - For Ok results: ("success", None)
/// - For Err results: (error_type, Some(error_message))
///
/// # Examples
///
/// ```no_run
/// use nexus::logging::extract_status;
/// use nexus::api::ApiError;
/// use axum::response::Response;
///
/// let result: Result<Response, ApiError> = Err(ApiError::service_unavailable("Backend offline"));
/// let (status, error_msg) = extract_status(&result);
/// assert_eq!(status, "error");
/// assert!(error_msg.is_some());
/// ```
pub fn extract_status(
    result: &Result<axum::response::Response, ApiError>,
) -> (String, Option<String>) {
    match result {
        Ok(_) => ("success".to_string(), None),
        Err(e) => {
            let status = e.error.r#type.clone();
            let message = e.error.message.clone();
            (status, Some(message))
        }
    }
}

/// Truncate request prompt for logging preview (privacy-safe)
///
/// Extracts and truncates the first message from a chat completion request
/// for logging purposes. Returns None if content logging is disabled.
///
/// When enabled, returns the first ~100 characters of the first user message.
/// This provides context for debugging without logging entire conversations.
///
/// # Arguments
///
/// * `request` - The chat completion request
/// * `enable_content_logging` - Whether content logging is enabled
///
/// # Examples
///
/// ```no_run
/// use nexus::logging::truncate_prompt;
/// use nexus::api::{ChatCompletionRequest, ChatMessage, MessageContent};
/// use std::collections::HashMap;
///
/// let request = ChatCompletionRequest {
///     model: "gpt-4".to_string(),
///     messages: vec![ChatMessage {
///         role: "user".to_string(),
///         content: MessageContent::Text {
///             content: "Hello, world!".to_string(),
///         },
///         name: None,
///     }],
///     stream: false,
///     temperature: None,
///     max_tokens: None,
///     top_p: None,
///     stop: None,
///     presence_penalty: None,
///     frequency_penalty: None,
///     user: None,
///     extra: HashMap::new(),
/// };
///
/// let preview = truncate_prompt(&request, true);
/// assert!(preview.is_some());
/// ```
pub fn truncate_prompt(
    request: &crate::api::ChatCompletionRequest,
    enable_content_logging: bool,
) -> Option<String> {
    if !enable_content_logging {
        return None;
    }

    // Extract first message content
    if let Some(first_message) = request.messages.first() {
        let content = match &first_message.content {
            crate::api::MessageContent::Text { content } => content.as_str(),
            crate::api::MessageContent::Parts { content: parts } => {
                // For parts content, concatenate text parts
                let text: String = parts
                    .iter()
                    .filter_map(|part| {
                        if part.part_type == "text" {
                            part.text.as_deref()
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                return if text.is_empty() {
                    None
                } else {
                    Some(truncate_string(&text, 100))
                };
            }
        };

        if content.is_empty() {
            return None;
        }

        return Some(truncate_string(content, 100));
    }

    None
}

/// Helper function to truncate a string to a maximum length
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.min(s.len())])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tokens_with_usage() {
        let response = ChatCompletionResponse {
            id: "test".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "test".to_string(),
            choices: vec![],
            usage: Some(crate::api::Usage {
                prompt_tokens: 100,
                completion_tokens: 50,
                total_tokens: 150,
            }),
        };

        let (prompt, completion, total) = extract_tokens(&response);
        assert_eq!(prompt, 100);
        assert_eq!(completion, 50);
        assert_eq!(total, 150);
    }

    #[test]
    fn test_extract_tokens_without_usage() {
        let response = ChatCompletionResponse {
            id: "test".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "test".to_string(),
            choices: vec![],
            usage: None,
        };

        let (prompt, completion, total) = extract_tokens(&response);
        assert_eq!(prompt, 0);
        assert_eq!(completion, 0);
        assert_eq!(total, 0);
    }
}
