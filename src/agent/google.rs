//! Google AI agent implementation.

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

/// Google AI agent implementation.
///
/// Handles Google Generative AI API with automatic request/response translation
/// to/from OpenAI format:
/// - Health check via GET /v1beta/models?key={key}
/// - Model listing from health check response
/// - Chat completion via POST /v1beta/models/{model}:generateContent?key={key}
/// - System message to systemInstruction field
/// - Role mapping: assistant <-> model
/// - Streaming via newline-delimited JSON (alt=sse)
pub struct GoogleAIAgent {
    /// Unique agent ID
    id: String,
    /// Human-readable name
    name: String,
    /// Base URL (e.g., "https://generativelanguage.googleapis.com")
    base_url: String,
    /// API key for query parameter authentication
    api_key: String,
    /// Shared HTTP client for connection pooling
    client: Arc<Client>,
    /// Pricing table for cost estimation
    #[allow(dead_code)]
    pricing: Arc<PricingTable>,
}

impl GoogleAIAgent {
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

    /// Translate OpenAI request to Google format.
    fn translate_request(&self, request: &ChatCompletionRequest) -> GoogleRequest {
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

        // Combine system messages into systemInstruction
        let system_instruction = if system_messages.is_empty() {
            None
        } else {
            Some(GoogleSystemInstruction {
                parts: vec![GooglePart {
                    text: system_messages.join("\n"),
                }],
            })
        };

        // Filter out system messages, convert roles
        let contents: Vec<GoogleContent> = request
            .messages
            .iter()
            .filter_map(|msg| {
                if msg.role == "system" {
                    None
                } else {
                    // Map OpenAI "assistant" to Google "model"
                    let role = if msg.role == "assistant" {
                        "model".to_string()
                    } else {
                        msg.role.clone()
                    };

                    let text = match &msg.content {
                        MessageContent::Text { content } => content.clone(),
                        MessageContent::Parts { .. } => String::new(), // Skip multimodal for now
                    };

                    Some(GoogleContent {
                        role,
                        parts: vec![GooglePart { text }],
                    })
                }
            })
            .collect();

        let mut generation_config = GoogleGenerationConfig::default();
        if let Some(temp) = request.temperature {
            generation_config.temperature = Some(temp);
        }
        if let Some(max_tokens) = request.max_tokens {
            generation_config.max_output_tokens = Some(max_tokens as i32);
        }

        GoogleRequest {
            contents,
            system_instruction,
            generation_config: Some(generation_config),
        }
    }

    /// Translate Google response to OpenAI format.
    fn translate_response(&self, response: GoogleResponse, model: &str) -> ChatCompletionResponse {
        let candidate = response.candidates.first();

        let (content, finish_reason) = if let Some(candidate) = candidate {
            // Extract text from parts
            let text = candidate
                .content
                .parts
                .iter()
                .map(|part| part.text.clone())
                .collect::<Vec<String>>()
                .join("");

            // Map finish_reason
            let finish = match candidate.finish_reason.as_deref() {
                Some("STOP") => "stop",
                Some("MAX_TOKENS") => "length",
                Some("SAFETY") => "content_filter",
                Some("RECITATION") => "content_filter",
                _ => "stop",
            }
            .to_string();

            (text, finish)
        } else {
            (String::new(), "stop".to_string())
        };

        // Extract usage metadata
        let usage = response.usage_metadata.map(|u| Usage {
            prompt_tokens: u.prompt_token_count.unwrap_or(0) as u32,
            completion_tokens: u.candidates_token_count.unwrap_or(0) as u32,
            total_tokens: u.total_token_count.unwrap_or(0) as u32,
        });

        ChatCompletionResponse {
            id: format!("chatcmpl-{}", uuid::Uuid::new_v4()),
            object: "chat.completion".to_string(),
            created: chrono::Utc::now().timestamp(),
            model: model.to_string(),
            choices: vec![Choice {
                index: 0,
                message: ChatMessage {
                    role: "assistant".to_string(),
                    content: MessageContent::Text { content },
                    name: None,
                    function_call: None,
                },
                finish_reason: Some(finish_reason),
            }],
            usage,
            extra: std::collections::HashMap::new(),
        }
    }

    /// Translate Google streaming chunk to OpenAI format.
    fn translate_stream_chunk(&self, data: &str, model: &str) -> Option<String> {
        // Parse Google streaming response (newline-delimited JSON)
        if let Ok(google_resp) = serde_json::from_str::<GoogleStreamChunk>(data) {
            if let Some(candidate) = google_resp.candidates.first() {
                // Check if this is a content chunk or finish chunk
                if !candidate.content.parts.is_empty() {
                    let text = candidate
                        .content
                        .parts
                        .iter()
                        .map(|part| part.text.clone())
                        .collect::<Vec<String>>()
                        .join("");

                    let chunk = serde_json::json!({
                        "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                        "object": "chat.completion.chunk",
                        "created": chrono::Utc::now().timestamp(),
                        "model": model,
                        "choices": [{
                            "index": 0,
                            "delta": {"content": text},
                            "finish_reason": null
                        }]
                    });
                    return Some(format!("data: {}\n\n", chunk));
                }

                // Check for finish_reason
                if let Some(finish_reason) = &candidate.finish_reason {
                    let finish = match finish_reason.as_str() {
                        "STOP" => "stop",
                        "MAX_TOKENS" => "length",
                        "SAFETY" => "content_filter",
                        "RECITATION" => "content_filter",
                        _ => "stop",
                    };

                    let chunk = serde_json::json!({
                        "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                        "object": "chat.completion.chunk",
                        "created": chrono::Utc::now().timestamp(),
                        "model": model,
                        "choices": [{
                            "index": 0,
                            "delta": {},
                            "finish_reason": finish
                        }]
                    });
                    return Some(format!("data: {}\n\n", chunk));
                }
            }
        }
        None
    }
}

/// Google Generative AI request format
#[derive(Debug, Serialize)]
struct GoogleRequest {
    contents: Vec<GoogleContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "systemInstruction")]
    system_instruction: Option<GoogleSystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "generationConfig")]
    generation_config: Option<GoogleGenerationConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GoogleContent {
    role: String,
    parts: Vec<GooglePart>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GooglePart {
    text: String,
}

#[derive(Debug, Serialize)]
struct GoogleSystemInstruction {
    parts: Vec<GooglePart>,
}

#[derive(Debug, Serialize, Default)]
struct GoogleGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: Option<i32>,
}

/// Google Generative AI response format
#[derive(Debug, Deserialize)]
struct GoogleResponse {
    candidates: Vec<GoogleCandidate>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GoogleUsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct GoogleCandidate {
    content: GoogleContent,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleUsageMetadata {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<i32>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<i32>,
    #[serde(rename = "totalTokenCount")]
    total_token_count: Option<i32>,
}

/// Google streaming chunk format
#[derive(Debug, Deserialize)]
struct GoogleStreamChunk {
    candidates: Vec<GoogleCandidate>,
}

/// Google models list response
#[derive(Debug, Deserialize)]
struct GoogleModelsResponse {
    models: Vec<GoogleModel>,
}

#[derive(Debug, Deserialize)]
struct GoogleModel {
    name: String,
    #[serde(rename = "supportedGenerationMethods")]
    #[serde(default)]
    supported_generation_methods: Vec<String>,
}

#[async_trait]
impl InferenceAgent for GoogleAIAgent {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn profile(&self) -> AgentProfile {
        AgentProfile {
            backend_type: "google".to_string(),
            version: None,
            privacy_zone: PrivacyZone::Open, // Cloud service
            capabilities: AgentCapabilities {
                embeddings: true, // Google supports embeddings
                model_lifecycle: false,
                token_counting: false,
                resource_monitoring: false,
            },
            capability_tier: None, // Will be set per-model in future
        }
    }

    async fn health_check(&self) -> Result<HealthStatus, AgentError> {
        let url = format!("{}/v1beta/models?key={}", self.base_url, self.api_key);

        tracing::debug!(
            backend = "google",
            agent_id = %self.id,
            "performing health check"
        );

        let start = std::time::Instant::now();
        let response = self
            .client
            .get(&url)
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
                backend = "google",
                agent_id = %self.id,
                latency_ms,
                status = %response.status(),
                "health check failed"
            );
            return Ok(HealthStatus::Unhealthy);
        }

        let body = response.text().await.map_err(|e| {
            AgentError::InvalidResponse(format!("Failed to read response body: {}", e))
        })?;

        let models: GoogleModelsResponse = serde_json::from_str(&body).map_err(|e| {
            AgentError::InvalidResponse(format!("Failed to parse Google models response: {}", e))
        })?;

        tracing::info!(
            backend = "google",
            agent_id = %self.id,
            latency_ms,
            model_count = models.models.len(),
            "health check succeeded"
        );

        Ok(HealthStatus::Healthy {
            model_count: models.models.len(),
        })
    }

    async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
        let url = format!("{}/v1beta/models?key={}", self.base_url, self.api_key);

        let response = self
            .client
            .get(&url)
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

        if !response.status().is_success() {
            return Err(AgentError::Upstream {
                status: response.status().as_u16(),
                message: format!("Failed to list models: {}", response.status()),
            });
        }

        let body = response.text().await.map_err(|e| {
            AgentError::InvalidResponse(format!("Failed to read response body: {}", e))
        })?;

        let models_response: GoogleModelsResponse = serde_json::from_str(&body).map_err(|e| {
            AgentError::InvalidResponse(format!("Failed to parse Google models response: {}", e))
        })?;

        let models = models_response
            .models
            .into_iter()
            .filter(|m| {
                m.supported_generation_methods
                    .contains(&"generateContent".to_string())
            })
            .map(|m| {
                // Extract model ID from name (e.g., "models/gemini-1.5-pro" -> "gemini-1.5-pro")
                let id = m
                    .name
                    .strip_prefix("models/")
                    .unwrap_or(&m.name)
                    .to_string();

                let mut model = ModelCapability {
                    id: id.clone(),
                    name: id.clone(),
                    context_length: 32768, // Default
                    supports_vision: false,
                    supports_tools: false,
                    supports_json_mode: false,
                    max_output_tokens: None,
                    capability_tier: None,
                };

                // Apply name heuristics
                Self::apply_name_heuristics(&mut model);
                model
            })
            .collect();

        Ok(models)
    }

    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
        _headers: Option<&HeaderMap>,
    ) -> Result<ChatCompletionResponse, AgentError> {
        let model = &request.model;
        let url = format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            self.base_url, model, self.api_key
        );

        tracing::debug!(
            backend = "google",
            agent_id = %self.id,
            model = %model,
            "initiating chat completion"
        );

        // Translate request
        let translation_start = std::time::Instant::now();
        let google_request = self.translate_request(&request);
        let translation_time_us = translation_start.elapsed().as_micros();
        tracing::debug!(translation_time_us, "request translation completed");

        let start = std::time::Instant::now();
        let response = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&google_request)
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
                backend = "google",
                agent_id = %self.id,
                model = %model,
                status = %status,
                latency_ms = start.elapsed().as_millis(),
                "chat completion failed"
            );
            return Err(AgentError::Upstream {
                status: status.as_u16(),
                message: error_body,
            });
        }

        let google_response: GoogleResponse = response.json().await.map_err(|e| {
            AgentError::InvalidResponse(format!("Failed to parse Google response: {}", e))
        })?;

        let latency_ms = start.elapsed().as_millis();
        let input_tokens = google_response
            .usage_metadata
            .as_ref()
            .and_then(|u| u.prompt_token_count)
            .unwrap_or(0);
        let output_tokens = google_response
            .usage_metadata
            .as_ref()
            .and_then(|u| u.candidates_token_count)
            .unwrap_or(0);

        tracing::info!(
            backend = "google",
            agent_id = %self.id,
            model = %model,
            latency_ms,
            input_tokens,
            output_tokens,
            "chat completion succeeded"
        );

        // Translate response
        let translation_start = std::time::Instant::now();
        let result = self.translate_response(google_response, model);
        let translation_time_us = translation_start.elapsed().as_micros();
        tracing::debug!(translation_time_us, "response translation completed");
        Ok(result)
    }

    async fn chat_completion_stream(
        &self,
        request: ChatCompletionRequest,
        _headers: Option<&HeaderMap>,
    ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
        use futures_util::stream::StreamExt;

        let model = request.model.clone();
        let url = format!(
            "{}/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            self.base_url, model, self.api_key
        );

        tracing::debug!(
            backend = "google",
            agent_id = %self.id,
            model = %model,
            "initiating streaming chat completion"
        );

        // Translate request
        let google_request = self.translate_request(&request);

        let response = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&google_request)
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
                backend = "google",
                agent_id = %self.id,
                model = %model,
                status = %status,
                "streaming chat completion failed"
            );
            return Err(AgentError::Upstream {
                status: status.as_u16(),
                message: error_body,
            });
        }

        tracing::info!(
            backend = "google",
            agent_id = %self.id,
            model = %model,
            "streaming chat completion started"
        );

        // Parse streaming response (newline-delimited JSON or SSE)
        let stream = response.bytes_stream().map(move |result| {
            result
                .map(|bytes| {
                    let text = String::from_utf8_lossy(&bytes).to_string();

                    // Try to parse as SSE format first
                    let mut data = "";
                    for line in text.lines() {
                        if let Some(d) = line.strip_prefix("data: ") {
                            data = d.trim();
                            break;
                        }
                    }

                    // If no SSE format, assume raw JSON
                    if data.is_empty() {
                        data = text.trim();
                    }

                    // Check for [DONE] marker
                    if data == "[DONE]" {
                        return StreamChunk {
                            data: "data: [DONE]\n\n".to_string(),
                        };
                    }

                    // Translate to OpenAI format
                    let model_clone = model.clone();
                    let temp_agent = GoogleAIAgent {
                        id: String::new(),
                        name: String::new(),
                        base_url: String::new(),
                        api_key: String::new(),
                        client: Arc::new(Client::new()),
                        pricing: Arc::new(PricingTable::new()),
                    };

                    let translated = temp_agent
                        .translate_stream_chunk(data, &model_clone)
                        .unwrap_or_else(|| text.clone());

                    StreamChunk { data: translated }
                })
                .map_err(|e| AgentError::Network(e.to_string()))
        });

        Ok(Box::pin(stream))
    }

    async fn count_tokens(&self, _model_id: &str, text: &str) -> TokenCount {
        // Google doesn't provide a public tokenizer, use heuristic
        TokenCount::Heuristic((text.len() / 4) as u32)
    }
}

impl GoogleAIAgent {
    /// Apply Google-specific name heuristics for capability detection.
    fn apply_name_heuristics(model: &mut ModelCapability) {
        let name = model.id.to_lowercase();

        // Gemini 1.5 models
        if name.contains("gemini-1.5-pro") || name.contains("gemini-1.5-flash") {
            model.supports_vision = true;
            model.supports_tools = true;
            model.context_length = 1048576; // 1M tokens
        }
        // Gemini 1.0 models
        else if name.contains("gemini-1.0-pro") || name.contains("gemini-pro") {
            model.supports_vision = false;
            model.supports_tools = true;
            model.context_length = 32768;
        }
        // Gemini Pro Vision
        else if name.contains("gemini-pro-vision") {
            model.supports_vision = true;
            model.supports_tools = false;
            model.context_length = 16384;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    fn test_agent(base_url: String, api_key: String) -> GoogleAIAgent {
        let client = Arc::new(Client::new());
        GoogleAIAgent::new(
            "test-google".to_string(),
            "Test Google".to_string(),
            base_url,
            api_key,
            client,
        )
    }

    #[tokio::test]
    async fn test_health_check_success() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/v1beta/models?key=test-key-123")
            .with_status(200)
            .with_body(r#"{"models":[{"name":"models/gemini-1.5-pro","supportedGenerationMethods":["generateContent"]},{"name":"models/gemini-1.5-flash","supportedGenerationMethods":["generateContent"]}]}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url(), "test-key-123".to_string());
        let status = agent.health_check().await.unwrap();

        mock.assert_async().await;
        assert_eq!(status, HealthStatus::Healthy { model_count: 2 });
    }

    #[tokio::test]
    async fn test_list_models() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/v1beta/models?key=test-key-123")
            .with_status(200)
            .with_body(r#"{"models":[{"name":"models/gemini-1.5-pro","supportedGenerationMethods":["generateContent"]},{"name":"models/gemini-1.0-pro","supportedGenerationMethods":["generateContent","embedContent"]}]}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url(), "test-key-123".to_string());
        let models = agent.list_models().await.unwrap();

        mock.assert_async().await;
        assert_eq!(models.len(), 2);

        // Check Gemini 1.5 Pro capabilities
        let gemini_15 = models.iter().find(|m| m.id == "gemini-1.5-pro").unwrap();
        assert!(gemini_15.supports_vision);
        assert!(gemini_15.supports_tools);
        assert_eq!(gemini_15.context_length, 1048576);

        // Check Gemini 1.0 Pro capabilities
        let gemini_10 = models.iter().find(|m| m.id == "gemini-1.0-pro").unwrap();
        assert!(!gemini_10.supports_vision);
        assert!(gemini_10.supports_tools);
        assert_eq!(gemini_10.context_length, 32768);
    }

    #[tokio::test]
    async fn test_chat_completion_with_system_message() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1beta/models/gemini-1.5-pro:generateContent?key=test-key-123")
            .with_status(200)
            .with_body(r#"{"candidates":[{"content":{"parts":[{"text":"Hello there!"}],"role":"model"},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":15,"candidatesTokenCount":8,"totalTokenCount":23}}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url(), "test-key-123".to_string());
        let request = ChatCompletionRequest {
            model: "gemini-1.5-pro".to_string(),
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
            assert_eq!(content, "Hello there!");
        } else {
            panic!("Expected text content");
        }
        assert_eq!(response.choices[0].finish_reason, Some("stop".to_string()));
        assert_eq!(response.usage.as_ref().unwrap().prompt_tokens, 15);
        assert_eq!(response.usage.as_ref().unwrap().completion_tokens, 8);
    }

    #[tokio::test]
    async fn test_profile() {
        let agent = test_agent(
            "https://generativelanguage.googleapis.com".to_string(),
            "test-key".to_string(),
        );
        let profile = agent.profile();

        assert_eq!(profile.backend_type, "google");
        assert_eq!(profile.privacy_zone, PrivacyZone::Open);
        assert!(profile.capabilities.embeddings);
        assert!(!profile.capabilities.token_counting);
    }

    // ── Translation Unit Tests ──────────────────────────────────────────

    fn make_request(messages: Vec<ChatMessage>, model: &str) -> ChatCompletionRequest {
        ChatCompletionRequest {
            model: model.to_string(),
            messages,
            stream: false,
            temperature: Some(0.5),
            max_tokens: Some(200),
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
    fn test_translate_request_role_mapping_assistant_to_model() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let request = make_request(
            vec![msg("user", "Hello"), msg("assistant", "Hi there")],
            "gemini-1.5-pro",
        );

        let translated = agent.translate_request(&request);

        assert_eq!(translated.contents.len(), 2);
        assert_eq!(translated.contents[0].role, "user");
        assert_eq!(translated.contents[1].role, "model");
    }

    #[test]
    fn test_translate_request_system_instruction() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let request = make_request(
            vec![msg("system", "Be helpful"), msg("user", "Hello")],
            "gemini-1.5-pro",
        );

        let translated = agent.translate_request(&request);

        assert!(translated.system_instruction.is_some());
        let si = translated.system_instruction.unwrap();
        assert_eq!(si.parts[0].text, "Be helpful");
        // System message should NOT be in contents
        assert_eq!(translated.contents.len(), 1);
        assert_eq!(translated.contents[0].role, "user");
    }

    #[test]
    fn test_translate_request_no_system_instruction() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let request = make_request(vec![msg("user", "Hello")], "gemini-1.5-pro");

        let translated = agent.translate_request(&request);

        assert!(translated.system_instruction.is_none());
    }

    #[test]
    fn test_translate_request_generation_config() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let request = make_request(vec![msg("user", "Hello")], "gemini-1.5-pro");

        let translated = agent.translate_request(&request);

        let config = translated.generation_config.unwrap();
        assert_eq!(config.temperature, Some(0.5));
        assert_eq!(config.max_output_tokens, Some(200));
    }

    #[test]
    fn test_translate_response_finish_reason_stop() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let response = GoogleResponse {
            candidates: vec![GoogleCandidate {
                content: GoogleContent {
                    role: "model".to_string(),
                    parts: vec![GooglePart {
                        text: "Hello!".to_string(),
                    }],
                },
                finish_reason: Some("STOP".to_string()),
            }],
            usage_metadata: Some(GoogleUsageMetadata {
                prompt_token_count: Some(10),
                candidates_token_count: Some(5),
                total_token_count: Some(15),
            }),
        };

        let translated = agent.translate_response(response, "gemini-1.5-pro");

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
    fn test_translate_response_finish_reason_max_tokens() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let response = GoogleResponse {
            candidates: vec![GoogleCandidate {
                content: GoogleContent {
                    role: "model".to_string(),
                    parts: vec![GooglePart {
                        text: "Truncated...".to_string(),
                    }],
                },
                finish_reason: Some("MAX_TOKENS".to_string()),
            }],
            usage_metadata: None,
        };

        let translated = agent.translate_response(response, "gemini-1.5-pro");

        assert_eq!(
            translated.choices[0].finish_reason,
            Some("length".to_string())
        );
    }

    #[test]
    fn test_translate_response_finish_reason_safety() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let response = GoogleResponse {
            candidates: vec![GoogleCandidate {
                content: GoogleContent {
                    role: "model".to_string(),
                    parts: vec![GooglePart {
                        text: "".to_string(),
                    }],
                },
                finish_reason: Some("SAFETY".to_string()),
            }],
            usage_metadata: None,
        };

        let translated = agent.translate_response(response, "gemini-1.5-pro");

        assert_eq!(
            translated.choices[0].finish_reason,
            Some("content_filter".to_string())
        );
    }

    #[test]
    fn test_translate_response_usage_metadata_mapping() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let response = GoogleResponse {
            candidates: vec![GoogleCandidate {
                content: GoogleContent {
                    role: "model".to_string(),
                    parts: vec![GooglePart {
                        text: "Hi".to_string(),
                    }],
                },
                finish_reason: Some("STOP".to_string()),
            }],
            usage_metadata: Some(GoogleUsageMetadata {
                prompt_token_count: Some(42),
                candidates_token_count: Some(17),
                total_token_count: Some(59),
            }),
        };

        let translated = agent.translate_response(response, "gemini-1.5-pro");

        let usage = translated.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 42);
        assert_eq!(usage.completion_tokens, 17);
        assert_eq!(usage.total_tokens, 59);
    }

    #[test]
    fn test_translate_stream_chunk_content() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let data =
            r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"Hello world"}]}}]}"#;

        let result = agent.translate_stream_chunk(data, "gemini-1.5-pro");

        assert!(result.is_some());
        let chunk = result.unwrap();
        assert!(chunk.starts_with("data: "));
        let json: serde_json::Value =
            serde_json::from_str(chunk.trim_start_matches("data: ").trim()).unwrap();
        assert_eq!(json["choices"][0]["delta"]["content"], "Hello world");
        assert_eq!(json["object"], "chat.completion.chunk");
    }

    #[test]
    fn test_translate_stream_chunk_finish_reason() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let data =
            r#"{"candidates":[{"content":{"role":"model","parts":[]},"finishReason":"STOP"}]}"#;

        let result = agent.translate_stream_chunk(data, "gemini-1.5-pro");

        assert!(result.is_some());
        let chunk = result.unwrap();
        let json: serde_json::Value =
            serde_json::from_str(chunk.trim_start_matches("data: ").trim()).unwrap();
        assert_eq!(json["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn test_translate_stream_chunk_invalid_json_returns_none() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());

        let result = agent.translate_stream_chunk("not json", "gemini-1.5-pro");

        assert!(result.is_none());
    }

    #[test]
    fn test_translate_stream_chunk_safety_filter() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let data =
            r#"{"candidates":[{"content":{"role":"model","parts":[]},"finishReason":"SAFETY"}]}"#;

        let result = agent.translate_stream_chunk(data, "gemini-1.5-pro");

        assert!(result.is_some());
        let chunk = result.unwrap();
        let json: serde_json::Value =
            serde_json::from_str(chunk.trim_start_matches("data: ").trim()).unwrap();
        assert_eq!(json["choices"][0]["finish_reason"], "content_filter");
    }

    #[tokio::test]
    async fn test_heuristic_token_counting() {
        let agent = test_agent("http://localhost".to_string(), "key".to_string());
        let count = agent
            .count_tokens("gemini-1.5-pro", "Hello world test")
            .await;

        match count {
            TokenCount::Heuristic(n) => assert_eq!(n, 16 / 4), // 16 chars / 4
            TokenCount::Exact(_) => panic!("Expected heuristic for Google"),
        }
    }
}
