//! Anthropic Claude agent implementation.
//!
//! Translates between OpenAI-compatible format and Anthropic Messages API format.

use super::{
    AgentCapabilities, AgentError, AgentProfile, HealthStatus, InferenceAgent, ModelCapability,
    PrivacyZone, StreamChunk, TokenCount,
};
use crate::api::types::{
    ChatCompletionRequest, ChatCompletionResponse, ChatMessage, MessageContent,
};
use async_trait::async_trait;
use axum::http::HeaderMap;
use futures_util::stream::BoxStream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// Anthropic agent implementation.
///
/// Handles Anthropic Claude API calls with API key authentication:
/// - Health check via POST /v1/messages (lightweight request)
/// - Model listing (static list of Claude models)
/// - Chat completion via POST /v1/messages with x-api-key header
pub struct AnthropicAgent {
    /// Unique agent ID
    id: String,
    /// Human-readable name
    name: String,
    /// Base URL (e.g., "https://api.anthropic.com")
    base_url: String,
    /// API key for x-api-key authentication
    api_key: String,
    /// Shared HTTP client for connection pooling
    client: Arc<Client>,
}

impl AnthropicAgent {
    pub fn new(
        id: String,
        name: String,
        base_url: String,
        api_key: String,
        client: Arc<Client>,
    ) -> Self {
        Self {
            id,
            name,
            base_url,
            api_key,
            client,
        }
    }

    /// Extract system message from OpenAI messages array
    fn extract_system_message(messages: &[ChatMessage]) -> Option<String> {
        messages
            .iter()
            .find(|m| m.role == "system")
            .and_then(|m| match &m.content {
                MessageContent::Text { content } => Some(content.clone()),
                MessageContent::Parts { .. } => None, // System messages should be text only
            })
    }

    /// Filter out system messages from OpenAI messages array
    fn filter_non_system_messages(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
        messages
            .into_iter()
            .filter(|m| m.role != "system")
            .collect()
    }

    /// Translate OpenAI request to Anthropic format
    fn translate_request(&self, request: ChatCompletionRequest) -> AnthropicRequest {
        let system = Self::extract_system_message(&request.messages);
        let messages = Self::filter_non_system_messages(request.messages)
            .into_iter()
            .map(|m| {
                let text = match m.content {
                    MessageContent::Text { content } => content,
                    MessageContent::Parts { content } => {
                        // For now, just extract text from parts
                        content
                            .into_iter()
                            .filter_map(|p| p.text)
                            .collect::<Vec<_>>()
                            .join("\n")
                    }
                };
                AnthropicMessage {
                    role: m.role,
                    content: vec![AnthropicContent {
                        content_type: "text".to_string(),
                        text,
                    }],
                }
            })
            .collect();

        AnthropicRequest {
            model: request.model,
            messages,
            system,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature,
            top_p: request.top_p,
            stream: Some(request.stream),
        }
    }

    /// Translate Anthropic response to OpenAI format
    fn translate_response(
        &self,
        response: AnthropicResponse,
        model: &str,
    ) -> ChatCompletionResponse {
        let content = response
            .content
            .first()
            .map(|c| c.text.clone())
            .unwrap_or_default();

        let finish_reason = match response.stop_reason.as_deref() {
            Some("end_turn") => "stop",
            Some("max_tokens") => "length",
            Some("stop_sequence") => "stop",
            _ => "stop",
        };

        ChatCompletionResponse {
            id: response.id,
            object: "chat.completion".to_string(),
            created: chrono::Utc::now().timestamp(),
            model: model.to_string(),
            choices: vec![crate::api::types::Choice {
                index: 0,
                message: crate::api::types::ChatMessage {
                    role: "assistant".to_string(),
                    content: MessageContent::Text { content },
                    name: None,
                },
                finish_reason: Some(finish_reason.to_string()),
            }],
            usage: Some(crate::api::types::Usage {
                prompt_tokens: response.usage.input_tokens as u32,
                completion_tokens: response.usage.output_tokens as u32,
                total_tokens: (response.usage.input_tokens + response.usage.output_tokens) as u32,
            }),
        }
    }
}

/// Anthropic request format
#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Serialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}

/// Anthropic response format
#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    #[allow(dead_code)]
    #[serde(rename = "type")]
    response_type: String,
    #[allow(dead_code)]
    role: String,
    content: Vec<AnthropicContentResponse>,
    #[allow(dead_code)]
    model: String,
    stop_reason: Option<String>,
    #[allow(dead_code)]
    stop_sequence: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentResponse {
    #[allow(dead_code)]
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u64,
    output_tokens: u64,
}

#[async_trait]
impl InferenceAgent for AnthropicAgent {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn profile(&self) -> AgentProfile {
        AgentProfile {
            backend_type: "anthropic".to_string(),
            version: None,
            privacy_zone: PrivacyZone::Open, // Cloud service
            capabilities: AgentCapabilities {
                embeddings: false,
                model_lifecycle: false,
                token_counting: false,
                resource_monitoring: false,
            },
        }
    }

    async fn health_check(&self) -> Result<HealthStatus, AgentError> {
        // Anthropic doesn't have a dedicated health endpoint
        // Use a minimal request to /v1/messages as health check
        let url = format!("{}/v1/messages", self.base_url);

        let test_request = AnthropicRequest {
            model: "claude-3-haiku-20240307".to_string(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: vec![AnthropicContent {
                    content_type: "text".to_string(),
                    text: "ping".to_string(),
                }],
            }],
            system: None,
            max_tokens: 10,
            temperature: None,
            top_p: None,
            stream: Some(false),
        };

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&test_request)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AgentError::Timeout(5000)
                } else {
                    AgentError::Network(e.to_string())
                }
            })?;

        let status = response.status();

        if status.is_success() {
            // Successfully got a response, backend is healthy
            Ok(HealthStatus::Healthy { model_count: 5 }) // Static count for Claude models
        } else if status.as_u16() == 401 || status.as_u16() == 403 {
            // Authentication failure - backend reachable but credentials invalid
            tracing::warn!(status = %status, "Anthropic authentication failed");
            Ok(HealthStatus::Unhealthy)
        } else {
            // Other error
            let body = response.text().await.unwrap_or_default();
            tracing::warn!(status = %status, body = %body, "Anthropic health check failed");
            Ok(HealthStatus::Unhealthy)
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
        // Anthropic doesn't have a public models endpoint
        // Return static list of known Claude models
        Ok(vec![
            ModelCapability {
                id: "claude-3-opus-20240229".to_string(),
                name: "Claude 3 Opus".to_string(),
                context_length: 200_000,
                supports_vision: true,
                supports_tools: true,
                supports_json_mode: false,
                max_output_tokens: Some(4096),
                capability_tier: Some(4), // Tier 4 - highest capability
            },
            ModelCapability {
                id: "claude-3-sonnet-20240229".to_string(),
                name: "Claude 3 Sonnet".to_string(),
                context_length: 200_000,
                supports_vision: true,
                supports_tools: true,
                supports_json_mode: false,
                max_output_tokens: Some(4096),
                capability_tier: Some(4),
            },
            ModelCapability {
                id: "claude-3-haiku-20240307".to_string(),
                name: "Claude 3 Haiku".to_string(),
                context_length: 200_000,
                supports_vision: true,
                supports_tools: true,
                supports_json_mode: false,
                max_output_tokens: Some(4096),
                capability_tier: Some(3), // Tier 3 - fast but capable
            },
            ModelCapability {
                id: "claude-2.1".to_string(),
                name: "Claude 2.1".to_string(),
                context_length: 200_000,
                supports_vision: false,
                supports_tools: false,
                supports_json_mode: false,
                max_output_tokens: Some(4096),
                capability_tier: Some(3),
            },
            ModelCapability {
                id: "claude-2.0".to_string(),
                name: "Claude 2.0".to_string(),
                context_length: 100_000,
                supports_vision: false,
                supports_tools: false,
                supports_json_mode: false,
                max_output_tokens: Some(4096),
                capability_tier: Some(3),
            },
        ])
    }

    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
        _headers: Option<&HeaderMap>,
    ) -> Result<ChatCompletionResponse, AgentError> {
        let url = format!("{}/v1/messages", self.base_url);
        let model = request.model.clone();

        // Translate request
        let anthropic_request = self.translate_request(request);

        // Send request
        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&anthropic_request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AgentError::Timeout(60000)
                } else {
                    AgentError::Network(e.to_string())
                }
            })?;

        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AgentError::Upstream {
                status: status.as_u16(),
                message: body,
            });
        }

        // Parse and translate response
        let anthropic_response: AnthropicResponse = response.json().await.map_err(|e| {
            AgentError::InvalidResponse(format!("Failed to parse Anthropic response: {}", e))
        })?;

        Ok(self.translate_response(anthropic_response, &model))
    }

    async fn chat_completion_stream(
        &self,
        _request: ChatCompletionRequest,
        _headers: Option<&HeaderMap>,
    ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
        // Streaming not implemented in MVP
        Err(AgentError::Unsupported("streaming"))
    }

    async fn count_tokens(&self, _model_id: &str, text: &str) -> TokenCount {
        // Use heuristic for now (Anthropic doesn't provide public tokenizer)
        // Anthropic's tokenizer is roughly 1 token per 4 characters
        TokenCount::Heuristic((text.len() / 4) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::MessageContent;

    #[test]
    fn test_extract_system_message() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text {
                    content: "You are a helpful assistant".to_string(),
                },
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text {
                    content: "Hello".to_string(),
                },
                name: None,
            },
        ];

        let system = AnthropicAgent::extract_system_message(&messages);
        assert_eq!(system, Some("You are a helpful assistant".to_string()));
    }

    #[test]
    fn test_filter_non_system_messages() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text {
                    content: "System prompt".to_string(),
                },
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text {
                    content: "Hello".to_string(),
                },
                name: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: MessageContent::Text {
                    content: "Hi there!".to_string(),
                },
                name: None,
            },
        ];

        let filtered = AnthropicAgent::filter_non_system_messages(messages);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].role, "user");
        assert_eq!(filtered[1].role, "assistant");
    }
}
