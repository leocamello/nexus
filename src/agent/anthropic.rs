//! Anthropic agent implementation.

use super::{
    AgentCapabilities, AgentError, AgentProfile, HealthStatus, InferenceAgent, ModelCapability,
    PrivacyZone, StreamChunk, TokenCount,
};
use crate::agent::pricing::PricingTable;
use crate::api::types::{
    ChatCompletionRequest, ChatCompletionResponse, ChatMessage, Choice, MessageContent, Usage,
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
/// Handles Anthropic Messages API v1 with automatic request/response translation
/// to/from OpenAI format:
/// - Health check via POST /v1/messages (minimal request)
/// - Model listing returns hardcoded Claude models
/// - Chat completion via POST /v1/messages with x-api-key authentication
/// - System message extraction to `system` parameter
/// - Streaming via SSE (Server-Sent Events)
pub struct AnthropicAgent {
    /// Unique agent ID
    id: String,
    /// Human-readable name
    name: String,
    /// Base URL (e.g., "https://api.anthropic.com")
    base_url: String,
    /// API key for x-api-key header
    api_key: String,
    /// Shared HTTP client for connection pooling
    client: Arc<Client>,
    /// Pricing table for cost estimation
    #[allow(dead_code)]
    pricing: Arc<PricingTable>,
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
            pricing: Arc::new(PricingTable::new()),
        }
    }

    /// Translate OpenAI request to Anthropic format.
    fn translate_request(&self, request: &ChatCompletionRequest) -> AnthropicRequest {
        // Extract system messages
        let system_messages: Vec<String> = request
            .messages
            .iter()
            .filter_map(|msg| {
                if msg.role == "system" {
                    match &msg.content {
                        MessageContent::Text { content } => Some(content.clone()),
                        MessageContent::Parts { .. } => None, // Skip multimodal for now
                    }
                } else {
                    None
                }
            })
            .collect();

        // Combine system messages into single system parameter
        let system = if system_messages.is_empty() {
            None
        } else {
            Some(system_messages.join("\n"))
        };

        // Filter out system messages, keep user/assistant
        let messages: Vec<AnthropicMessage> = request
            .messages
            .iter()
            .filter_map(|msg| {
                if msg.role == "system" {
                    None
                } else {
                    let content = match &msg.content {
                        MessageContent::Text { content } => content.clone(),
                        MessageContent::Parts { .. } => String::new(), // Skip multimodal for now
                    };
                    Some(AnthropicMessage {
                        role: msg.role.clone(),
                        content,
                    })
                }
            })
            .collect();

        AnthropicRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096), // Required by Anthropic
            system,
            temperature: request.temperature,
            stream: Some(request.stream),
        }
    }

    /// Translate Anthropic response to OpenAI format.
    fn translate_response(&self, response: AnthropicResponse) -> ChatCompletionResponse {
        // Extract text from content blocks
        let text = response
            .content
            .iter()
            .filter_map(|block| {
                if block.r#type == "text" {
                    block.text.clone()
                } else {
                    None
                }
            })
            .collect::<Vec<String>>()
            .join("");

        // Map stop_reason to finish_reason
        let finish_reason = match response.stop_reason.as_deref() {
            Some("end_turn") => "stop",
            Some("max_tokens") => "length",
            Some("stop_sequence") => "stop",
            _ => "stop",
        }
        .to_string();

        ChatCompletionResponse {
            id: response.id,
            object: "chat.completion".to_string(),
            created: chrono::Utc::now().timestamp(),
            model: response.model,
            choices: vec![Choice {
                index: 0,
                message: ChatMessage {
                    role: "assistant".to_string(),
                    content: MessageContent::Text { content: text },
                    name: None,
                    function_call: None,
                },
                finish_reason: Some(finish_reason),
            }],
            usage: Some(Usage {
                prompt_tokens: response.usage.input_tokens,
                completion_tokens: response.usage.output_tokens,
                total_tokens: response.usage.input_tokens + response.usage.output_tokens,
            }),
            extra: std::collections::HashMap::new(),
        }
    }

    /// Translate Anthropic streaming chunk to OpenAI format.
    fn translate_stream_chunk(&self, event: &str, data: &str) -> Option<String> {
        match event {
            "message_start" => {
                // Parse message_start to get id and model
                if let Ok(msg_start) = serde_json::from_str::<MessageStart>(data) {
                    let chunk = serde_json::json!({
                        "id": msg_start.message.id,
                        "object": "chat.completion.chunk",
                        "created": chrono::Utc::now().timestamp(),
                        "model": msg_start.message.model,
                        "choices": [{
                            "index": 0,
                            "delta": {"role": "assistant"},
                            "finish_reason": null
                        }]
                    });
                    Some(format!("data: {}\n\n", chunk))
                } else {
                    None
                }
            }
            "content_block_delta" => {
                // Parse content block delta
                if let Ok(delta) = serde_json::from_str::<ContentBlockDelta>(data) {
                    if delta.delta.r#type == "text_delta" {
                        let chunk = serde_json::json!({
                            "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                            "object": "chat.completion.chunk",
                            "created": chrono::Utc::now().timestamp(),
                            "model": "claude",
                            "choices": [{
                                "index": 0,
                                "delta": {"content": delta.delta.text},
                                "finish_reason": null
                            }]
                        });
                        return Some(format!("data: {}\n\n", chunk));
                    }
                }
                None
            }
            "message_delta" => {
                // Parse message delta for stop_reason
                if let Ok(msg_delta) = serde_json::from_str::<MessageDelta>(data) {
                    let finish_reason = match msg_delta.delta.stop_reason.as_deref() {
                        Some("end_turn") => "stop",
                        Some("max_tokens") => "length",
                        Some("stop_sequence") => "stop",
                        _ => "stop",
                    };
                    let chunk = serde_json::json!({
                        "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                        "object": "chat.completion.chunk",
                        "created": chrono::Utc::now().timestamp(),
                        "model": "claude",
                        "choices": [{
                            "index": 0,
                            "delta": {},
                            "finish_reason": finish_reason
                        }]
                    });
                    return Some(format!("data: {}\n\n", chunk));
                }
                None
            }
            "message_stop" => Some("data: [DONE]\n\n".to_string()),
            _ => None,
        }
    }
}

/// Anthropic Messages API request format
#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

/// Anthropic Messages API response format
#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    #[allow(dead_code)]
    r#type: String,
    #[allow(dead_code)]
    role: String,
    content: Vec<ContentBlock>,
    model: String,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

/// Streaming message_start event
#[derive(Debug, Deserialize)]
struct MessageStart {
    message: MessageInfo,
}

#[derive(Debug, Deserialize)]
struct MessageInfo {
    id: String,
    model: String,
}

/// Streaming content_block_delta event
#[derive(Debug, Deserialize)]
struct ContentBlockDelta {
    delta: TextDelta,
}

#[derive(Debug, Deserialize)]
struct TextDelta {
    r#type: String,
    #[serde(default)]
    text: String,
}

/// Streaming message_delta event
#[derive(Debug, Deserialize)]
struct MessageDelta {
    delta: StopReasonDelta,
}

#[derive(Debug, Deserialize)]
struct StopReasonDelta {
    stop_reason: Option<String>,
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
            capability_tier: None,  // Will be set per-model in future
        }
    }

    async fn health_check(&self) -> Result<HealthStatus, AgentError> {
        // Health check via minimal request
        let url = format!("{}/v1/messages", self.base_url);

        tracing::debug!(
            backend = "anthropic",
            agent_id = %self.id,
            "performing health check"
        );

        let request = AnthropicRequest {
            model: "claude-3-haiku-20240307".to_string(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
            }],
            max_tokens: 1,
            system: None,
            temperature: None,
            stream: None,
        };

        let start = std::time::Instant::now();
        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
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

        let latency_ms = start.elapsed().as_millis();

        if !response.status().is_success() {
            tracing::info!(
                backend = "anthropic",
                agent_id = %self.id,
                latency_ms,
                status = %response.status(),
                "health check failed"
            );
            return Ok(HealthStatus::Unhealthy);
        }

        tracing::info!(
            backend = "anthropic",
            agent_id = %self.id,
            latency_ms,
            "health check succeeded"
        );

        Ok(HealthStatus::Healthy { model_count: 4 })
    }

    async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
        // Return hardcoded list of Claude models
        Ok(vec![
            ModelCapability {
                id: "claude-3-opus-20240229".to_string(),
                name: "claude-3-opus-20240229".to_string(),
                context_length: 200000,
                supports_vision: true,
                supports_tools: true,
                supports_json_mode: false,
                max_output_tokens: Some(4096),
                capability_tier: Some(3), // Premium tier
            },
            ModelCapability {
                id: "claude-3-sonnet-20240229".to_string(),
                name: "claude-3-sonnet-20240229".to_string(),
                context_length: 200000,
                supports_vision: true,
                supports_tools: true,
                supports_json_mode: false,
                max_output_tokens: Some(4096),
                capability_tier: Some(2), // Standard tier
            },
            ModelCapability {
                id: "claude-3-haiku-20240307".to_string(),
                name: "claude-3-haiku-20240307".to_string(),
                context_length: 200000,
                supports_vision: true,
                supports_tools: true,
                supports_json_mode: false,
                max_output_tokens: Some(4096),
                capability_tier: Some(1), // Fast tier
            },
            ModelCapability {
                id: "claude-3-5-sonnet-20241022".to_string(),
                name: "claude-3-5-sonnet-20241022".to_string(),
                context_length: 200000,
                supports_vision: true,
                supports_tools: true,
                supports_json_mode: false,
                max_output_tokens: Some(8192),
                capability_tier: Some(3), // Premium tier
            },
        ])
    }

    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
        headers: Option<&HeaderMap>,
    ) -> Result<ChatCompletionResponse, AgentError> {
        let url = format!("{}/v1/messages", self.base_url);

        tracing::debug!(
            backend = "anthropic",
            agent_id = %self.id,
            model = %request.model,
            "initiating chat completion"
        );

        // Prefer config API key, but allow header override
        let api_key = if let Some(headers) = headers {
            headers
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
                .unwrap_or_else(|| self.api_key.clone())
        } else {
            self.api_key.clone()
        };

        // Translate request
        let translation_start = std::time::Instant::now();
        let anthropic_request = self.translate_request(&request);
        let translation_time_us = translation_start.elapsed().as_micros();
        tracing::debug!(translation_time_us, "request translation completed");

        let start = std::time::Instant::now();
        let response = self
            .client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&anthropic_request)
            .timeout(Duration::from_secs(120))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AgentError::Timeout(120000)
                } else {
                    AgentError::Network(e.to_string())
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            tracing::info!(
                backend = "anthropic",
                agent_id = %self.id,
                model = %request.model,
                status = %status,
                latency_ms = start.elapsed().as_millis(),
                "chat completion failed"
            );
            return Err(AgentError::Upstream {
                status: status.as_u16(),
                message: error_body,
            });
        }

        let anthropic_response: AnthropicResponse = response.json().await.map_err(|e| {
            AgentError::InvalidResponse(format!("Failed to parse Anthropic response: {}", e))
        })?;

        let latency_ms = start.elapsed().as_millis();
        let input_tokens = anthropic_response.usage.input_tokens;
        let output_tokens = anthropic_response.usage.output_tokens;

        tracing::info!(
            backend = "anthropic",
            agent_id = %self.id,
            model = %request.model,
            latency_ms,
            input_tokens,
            output_tokens,
            "chat completion succeeded"
        );

        // Translate response
        let translation_start = std::time::Instant::now();
        let result = self.translate_response(anthropic_response);
        let translation_time_us = translation_start.elapsed().as_micros();
        tracing::debug!(translation_time_us, "response translation completed");
        Ok(result)
    }

    async fn chat_completion_stream(
        &self,
        request: ChatCompletionRequest,
        headers: Option<&HeaderMap>,
    ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
        use futures_util::stream::StreamExt;

        let url = format!("{}/v1/messages", self.base_url);

        tracing::debug!(
            backend = "anthropic",
            agent_id = %self.id,
            model = %request.model,
            "initiating streaming chat completion"
        );

        // Prefer config API key, but allow header override
        let api_key = if let Some(headers) = headers {
            headers
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
                .unwrap_or_else(|| self.api_key.clone())
        } else {
            self.api_key.clone()
        };

        // Translate request and enable streaming
        let mut anthropic_request = self.translate_request(&request);
        anthropic_request.stream = Some(true);

        let response = self
            .client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&anthropic_request)
            .timeout(Duration::from_secs(120))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AgentError::Timeout(120000)
                } else {
                    AgentError::Network(e.to_string())
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            tracing::info!(
                backend = "anthropic",
                agent_id = %self.id,
                model = %request.model,
                status = %status,
                "streaming chat completion failed"
            );
            return Err(AgentError::Upstream {
                status: status.as_u16(),
                message: error_body,
            });
        }

        tracing::info!(
            backend = "anthropic",
            agent_id = %self.id,
            model = %request.model,
            "streaming chat completion started"
        );

        // Parse SSE stream
        let stream = response.bytes_stream().map(move |result| {
            result
                .map(|bytes| {
                    let text = String::from_utf8_lossy(&bytes).to_string();

                    // Parse SSE format: event: xxx\ndata: {...}\n\n
                    let mut event_type = "";
                    let mut data = "";

                    for line in text.lines() {
                        if let Some(evt) = line.strip_prefix("event: ") {
                            event_type = evt.trim();
                        } else if let Some(d) = line.strip_prefix("data: ") {
                            data = d.trim();
                        }
                    }

                    // Translate to OpenAI format
                    let translated = if !event_type.is_empty() && !data.is_empty() {
                        // Create a temporary agent to access translate_stream_chunk
                        // This is safe because we only need the method, not the state
                        let temp_agent = AnthropicAgent {
                            id: String::new(),
                            name: String::new(),
                            base_url: String::new(),
                            api_key: String::new(),
                            client: Arc::new(Client::new()),
                            pricing: Arc::new(PricingTable::new()),
                        };
                        temp_agent
                            .translate_stream_chunk(event_type, data)
                            .unwrap_or_else(|| text.clone())
                    } else {
                        text
                    };

                    StreamChunk { data: translated }
                })
                .map_err(|e| AgentError::Network(e.to_string()))
        });

        Ok(Box::pin(stream))
    }

    async fn count_tokens(&self, _model_id: &str, text: &str) -> TokenCount {
        // Anthropic doesn't provide a public tokenizer, use heuristic
        TokenCount::Heuristic((text.len() / 4) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    fn test_agent(base_url: String, api_key: String) -> AnthropicAgent {
        let client = Arc::new(Client::new());
        AnthropicAgent::new(
            "test-anthropic".to_string(),
            "Test Anthropic".to_string(),
            base_url,
            api_key,
            client,
        )
    }

    #[tokio::test]
    async fn test_health_check_success() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/messages")
            .match_header("x-api-key", "sk-ant-test123")
            .match_header("anthropic-version", "2023-06-01")
            .with_status(200)
            .with_body(
                r#"{"id":"msg_123","type":"message","role":"assistant","content":[{"type":"text","text":"Hi"}],"model":"claude-3-haiku-20240307","stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":5}}"#,
            )
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-ant-test123".to_string());
        let status = agent.health_check().await.unwrap();

        mock.assert_async().await;
        assert_eq!(status, HealthStatus::Healthy { model_count: 4 });
    }

    #[tokio::test]
    async fn test_list_models() {
        let agent = test_agent(
            "https://api.anthropic.com".to_string(),
            "sk-ant-test".to_string(),
        );
        let models = agent.list_models().await.unwrap();

        assert_eq!(models.len(), 4);
        assert!(models.iter().any(|m| m.id == "claude-3-opus-20240229"));
        assert!(models.iter().any(|m| m.id == "claude-3-5-sonnet-20241022"));

        // Check capabilities
        let opus = models
            .iter()
            .find(|m| m.id == "claude-3-opus-20240229")
            .unwrap();
        assert!(opus.supports_vision);
        assert!(opus.supports_tools);
        assert_eq!(opus.context_length, 200000);
    }

    #[tokio::test]
    async fn test_chat_completion_with_system_message() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/messages")
            .match_header("x-api-key", "sk-ant-test123")
            .with_status(200)
            .with_body(
                r#"{"id":"msg_456","type":"message","role":"assistant","content":[{"type":"text","text":"Hello!"}],"model":"claude-3-opus-20240229","stop_reason":"end_turn","usage":{"input_tokens":15,"output_tokens":8}}"#,
            )
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-ant-test123".to_string());
        let request = ChatCompletionRequest {
            model: "claude-3-opus-20240229".to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: MessageContent::Text {
                        content: "You are helpful".to_string(),
                    },
                    name: None,
                    function_call: None,
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: MessageContent::Text {
                        content: "Hi".to_string(),
                    },
                    name: None,
                    function_call: None,
                },
            ],
            stream: false,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            extra: std::collections::HashMap::new(),
        };

        let response = agent.chat_completion(request, None).await.unwrap();

        mock.assert_async().await;

        if let MessageContent::Text { content } = &response.choices[0].message.content {
            assert_eq!(content, "Hello!");
        } else {
            panic!("Expected text content");
        }
        assert_eq!(response.choices[0].finish_reason, Some("stop".to_string()));
    }

    #[tokio::test]
    async fn test_profile() {
        let agent = test_agent(
            "https://api.anthropic.com".to_string(),
            "sk-ant-test".to_string(),
        );
        let profile = agent.profile();

        assert_eq!(profile.backend_type, "anthropic");
        assert_eq!(profile.privacy_zone, PrivacyZone::Open);
        assert!(!profile.capabilities.embeddings);
        assert!(!profile.capabilities.token_counting);
    }

    // ── Translation Unit Tests ──────────────────────────────────────────

    fn make_request(messages: Vec<ChatMessage>, model: &str) -> ChatCompletionRequest {
        ChatCompletionRequest {
            model: model.to_string(),
            messages,
            stream: false,
            temperature: Some(0.7),
            max_tokens: Some(100),
            top_p: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            extra: std::collections::HashMap::new(),
        }
    }

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.to_string(),
            content: MessageContent::Text {
                content: content.to_string(),
            },
            name: None,
            function_call: None,
        }
    }

    #[test]
    fn test_translate_request_extracts_system_message() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let request = make_request(
            vec![msg("system", "You are helpful"), msg("user", "Hello")],
            "claude-3-opus",
        );

        let translated = agent.translate_request(&request);

        assert_eq!(translated.system, Some("You are helpful".to_string()));
        assert_eq!(translated.messages.len(), 1);
        assert_eq!(translated.messages[0].role, "user");
        assert_eq!(translated.messages[0].content, "Hello");
    }

    #[test]
    fn test_translate_request_no_system_message() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let request = make_request(vec![msg("user", "Hello")], "claude-3-opus");

        let translated = agent.translate_request(&request);

        assert!(translated.system.is_none());
        assert_eq!(translated.messages.len(), 1);
    }

    #[test]
    fn test_translate_request_multiple_system_messages() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let request = make_request(
            vec![
                msg("system", "Be concise"),
                msg("system", "Use formal tone"),
                msg("user", "Hello"),
            ],
            "claude-3-opus",
        );

        let translated = agent.translate_request(&request);

        assert_eq!(
            translated.system,
            Some("Be concise\nUse formal tone".to_string())
        );
        assert_eq!(translated.messages.len(), 1);
    }

    #[test]
    fn test_translate_request_sets_max_tokens_default() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let mut request = make_request(vec![msg("user", "Hello")], "claude-3-opus");
        request.max_tokens = None;

        let translated = agent.translate_request(&request);

        assert_eq!(translated.max_tokens, 4096);
    }

    #[test]
    fn test_translate_request_preserves_temperature() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let request = make_request(vec![msg("user", "Hello")], "claude-3-opus");

        let translated = agent.translate_request(&request);

        assert_eq!(translated.temperature, Some(0.7));
    }

    #[test]
    fn test_translate_response_maps_end_turn_to_stop() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let response = AnthropicResponse {
            id: "msg_123".to_string(),
            r#type: "message".to_string(),
            role: "assistant".to_string(),
            content: vec![ContentBlock {
                r#type: "text".to_string(),
                text: Some("Hello!".to_string()),
            }],
            model: "claude-3-opus-20240229".to_string(),
            stop_reason: Some("end_turn".to_string()),
            usage: AnthropicUsage {
                input_tokens: 10,
                output_tokens: 5,
            },
        };

        let translated = agent.translate_response(response);

        assert_eq!(
            translated.choices[0].finish_reason,
            Some("stop".to_string())
        );
        if let MessageContent::Text { content } = &translated.choices[0].message.content {
            assert_eq!(content, "Hello!");
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_translate_response_maps_max_tokens_to_length() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let response = AnthropicResponse {
            id: "msg_123".to_string(),
            r#type: "message".to_string(),
            role: "assistant".to_string(),
            content: vec![ContentBlock {
                r#type: "text".to_string(),
                text: Some("Truncated...".to_string()),
            }],
            model: "claude-3-opus-20240229".to_string(),
            stop_reason: Some("max_tokens".to_string()),
            usage: AnthropicUsage {
                input_tokens: 100,
                output_tokens: 4096,
            },
        };

        let translated = agent.translate_response(response);

        assert_eq!(
            translated.choices[0].finish_reason,
            Some("length".to_string())
        );
    }

    #[test]
    fn test_translate_response_maps_usage_fields() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let response = AnthropicResponse {
            id: "msg_123".to_string(),
            r#type: "message".to_string(),
            role: "assistant".to_string(),
            content: vec![ContentBlock {
                r#type: "text".to_string(),
                text: Some("Hi".to_string()),
            }],
            model: "claude-3-opus-20240229".to_string(),
            stop_reason: Some("end_turn".to_string()),
            usage: AnthropicUsage {
                input_tokens: 42,
                output_tokens: 17,
            },
        };

        let translated = agent.translate_response(response);

        let usage = translated.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 42);
        assert_eq!(usage.completion_tokens, 17);
        assert_eq!(usage.total_tokens, 59);
    }

    #[test]
    fn test_translate_response_multiple_content_blocks() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let response = AnthropicResponse {
            id: "msg_123".to_string(),
            r#type: "message".to_string(),
            role: "assistant".to_string(),
            content: vec![
                ContentBlock {
                    r#type: "text".to_string(),
                    text: Some("Part 1. ".to_string()),
                },
                ContentBlock {
                    r#type: "text".to_string(),
                    text: Some("Part 2.".to_string()),
                },
            ],
            model: "claude-3-opus-20240229".to_string(),
            stop_reason: Some("end_turn".to_string()),
            usage: AnthropicUsage {
                input_tokens: 10,
                output_tokens: 10,
            },
        };

        let translated = agent.translate_response(response);

        if let MessageContent::Text { content } = &translated.choices[0].message.content {
            assert_eq!(content, "Part 1. Part 2.");
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_translate_stream_chunk_message_start() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let data = r#"{"message":{"id":"msg_123","model":"claude-3-opus"}}"#;

        let result = agent.translate_stream_chunk("message_start", data);

        assert!(result.is_some());
        let chunk = result.unwrap();
        assert!(chunk.starts_with("data: "));
        let json: serde_json::Value =
            serde_json::from_str(chunk.trim_start_matches("data: ").trim()).unwrap();
        assert_eq!(json["choices"][0]["delta"]["role"], "assistant");
    }

    #[test]
    fn test_translate_stream_chunk_content_block_delta() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let data = r#"{"delta":{"type":"text_delta","text":"Hello"}}"#;

        let result = agent.translate_stream_chunk("content_block_delta", data);

        assert!(result.is_some());
        let chunk = result.unwrap();
        let json: serde_json::Value =
            serde_json::from_str(chunk.trim_start_matches("data: ").trim()).unwrap();
        assert_eq!(json["choices"][0]["delta"]["content"], "Hello");
    }

    #[test]
    fn test_translate_stream_chunk_message_delta_stop() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let data = r#"{"delta":{"stop_reason":"end_turn"}}"#;

        let result = agent.translate_stream_chunk("message_delta", data);

        assert!(result.is_some());
        let chunk = result.unwrap();
        let json: serde_json::Value =
            serde_json::from_str(chunk.trim_start_matches("data: ").trim()).unwrap();
        assert_eq!(json["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn test_translate_stream_chunk_message_stop_returns_done() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());

        let result = agent.translate_stream_chunk("message_stop", "");

        assert_eq!(result, Some("data: [DONE]\n\n".to_string()));
    }

    #[test]
    fn test_translate_stream_chunk_unknown_event_returns_none() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());

        let result = agent.translate_stream_chunk("ping", "{}");

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_heuristic_token_counting() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let count = agent
            .count_tokens("claude-3-opus", "Hello world test")
            .await;

        match count {
            TokenCount::Heuristic(n) => assert_eq!(n, 16 / 4), // 16 chars / 4
            TokenCount::Exact(_) => panic!("Expected heuristic for Anthropic"),
        }
    }
}
