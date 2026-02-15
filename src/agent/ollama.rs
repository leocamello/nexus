//! Ollama agent implementation.

use super::{
    AgentCapabilities, AgentError, AgentProfile, HealthStatus, InferenceAgent, ModelCapability,
    PrivacyZone, StreamChunk,
};
use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};
use async_trait::async_trait;
use axum::http::HeaderMap;
use futures_util::stream::BoxStream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// Ollama agent implementation.
///
/// Handles Ollama-specific API calls:
/// - Health check via GET /api/tags
/// - Model listing via GET /api/tags + POST /api/show enrichment
/// - Chat completion via POST /v1/chat/completions (OpenAI-compatible)
pub struct OllamaAgent {
    /// Unique agent ID
    id: String,
    /// Human-readable name
    name: String,
    /// Base URL (e.g., "http://localhost:11434")
    base_url: String,
    /// Shared HTTP client for connection pooling
    client: Arc<Client>,
}

impl OllamaAgent {
    pub fn new(id: String, name: String, base_url: String, client: Arc<Client>) -> Self {
        Self {
            id,
            name,
            base_url,
            client,
        }
    }
}

/// Ollama /api/tags response format
#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
}

/// Ollama /api/show response format (per-model detail)
#[derive(Deserialize, Serialize)]
struct OllamaShowResponse {
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    model_info: serde_json::Value,
}

#[async_trait]
impl InferenceAgent for OllamaAgent {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn profile(&self) -> AgentProfile {
        AgentProfile {
            backend_type: "ollama".to_string(),
            version: None, // TODO: Extract from backend in future
            privacy_zone: PrivacyZone::Restricted,
            capabilities: AgentCapabilities {
                embeddings: false,
                model_lifecycle: false, // Phase 1: Not implemented
                token_counting: false,
                resource_monitoring: false,
            },
        }
    }

    async fn health_check(&self) -> Result<HealthStatus, AgentError> {
        let url = format!("{}/api/tags", self.base_url);
        
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
            return Ok(HealthStatus::Unhealthy);
        }

        let body = response.text().await.map_err(|e| {
            AgentError::InvalidResponse(format!("Failed to read response body: {}", e))
        })?;

        let tags: OllamaTagsResponse = serde_json::from_str(&body).map_err(|e| {
            AgentError::InvalidResponse(format!("Failed to parse Ollama tags response: {}", e))
        })?;

        Ok(HealthStatus::Healthy {
            model_count: tags.models.len(),
        })
    }

    async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
        let url = format!("{}/api/tags", self.base_url);
        
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

        let tags: OllamaTagsResponse = serde_json::from_str(&body).map_err(|e| {
            AgentError::InvalidResponse(format!("Failed to parse Ollama tags response: {}", e))
        })?;

        // Create basic models
        let mut models: Vec<ModelCapability> = tags
            .models
            .into_iter()
            .map(|m| ModelCapability {
                id: m.name.clone(),
                name: m.name,
                context_length: 4096,
                supports_vision: false,
                supports_tools: false,
                supports_json_mode: false,
                max_output_tokens: None,
                capability_tier: None,
            })
            .collect();

        // Enrich with /api/show data
        self.enrich_models(&mut models).await;

        Ok(models)
    }

    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
        headers: Option<&HeaderMap>,
    ) -> Result<ChatCompletionResponse, AgentError> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        let mut req = self.client.post(&url).json(&request).timeout(Duration::from_secs(120));

        // Forward Authorization header if present
        if let Some(headers) = headers {
            if let Some(auth) = headers.get("authorization") {
                req = req.header("authorization", auth);
            }
        }

        let response = req.send().await.map_err(|e| {
            if e.is_timeout() {
                AgentError::Timeout(120000)
            } else {
                AgentError::Network(e.to_string())
            }
        })?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AgentError::Upstream {
                status: status.as_u16(),
                message: error_body,
            });
        }

        let completion: ChatCompletionResponse = response.json().await.map_err(|e| {
            AgentError::InvalidResponse(format!("Failed to parse completion response: {}", e))
        })?;

        Ok(completion)
    }

    async fn chat_completion_stream(
        &self,
        request: ChatCompletionRequest,
        headers: Option<&HeaderMap>,
    ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
        use futures_util::stream::StreamExt;

        let url = format!("{}/v1/chat/completions", self.base_url);

        let mut req = self.client.post(&url).json(&request).timeout(Duration::from_secs(120));

        // Forward Authorization header if present
        if let Some(headers) = headers {
            if let Some(auth) = headers.get("authorization") {
                req = req.header("authorization", auth);
            }
        }

        let response = req.send().await.map_err(|e| {
            if e.is_timeout() {
                AgentError::Timeout(120000)
            } else {
                AgentError::Network(e.to_string())
            }
        })?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AgentError::Upstream {
                status: status.as_u16(),
                message: error_body,
            });
        }

        // Convert byte stream to SSE chunks
        let stream = response.bytes_stream().map(|result| {
            result
                .map(|bytes| StreamChunk {
                    data: String::from_utf8_lossy(&bytes).to_string(),
                })
                .map_err(|e| AgentError::Network(e.to_string()))
        });

        Ok(Box::pin(stream))
    }
}

impl OllamaAgent {
    /// Enrich models with real capabilities from /api/show endpoint.
    async fn enrich_models(&self, models: &mut [ModelCapability]) {
        for model in models.iter_mut() {
            let url = format!("{}/api/show", self.base_url);
            let body = serde_json::json!({"name": model.id});

            match self
                .client
                .post(&url)
                .json(&body)
                .timeout(Duration::from_secs(5))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(text) = resp.text().await {
                        if let Ok(show) = serde_json::from_str::<OllamaShowResponse>(&text) {
                            model.supports_vision = show.capabilities.iter().any(|c| c == "vision");
                            model.supports_tools = show.capabilities.iter().any(|c| c == "tools");

                            // Extract context_length from model_info
                            if let Some(obj) = show.model_info.as_object() {
                                for (k, v) in obj {
                                    if k.ends_with(".context_length") {
                                        if let Some(ctx) = v.as_u64() {
                                            model.context_length = ctx as u32;
                                        }
                                        break;
                                    }
                                }
                            }

                            continue; // Got real data, skip heuristics
                        }
                    }
                }
                _ => {}
            }

            // Fallback to name-based heuristics if /api/show failed
            Self::apply_name_heuristics(model);
        }
    }

    /// Apply name-based heuristics for capability detection.
    fn apply_name_heuristics(model: &mut ModelCapability) {
        let name = model.id.to_lowercase();

        // Vision support heuristics
        if name.contains("vision")
            || name.contains("llava")
            || name.contains("bakllava")
            || name.contains("gpt-4-vision")
        {
            model.supports_vision = true;
        }

        // Tool calling heuristics
        if name.contains("command")
            || name.contains("functionary")
            || name.contains("hermes")
            || name.contains("gpt-4")
            || name.contains("gpt-3.5-turbo")
        {
            model.supports_tools = true;
        }

        // Context length heuristics by size
        if name.contains("32k") {
            model.context_length = 32768;
        } else if name.contains("16k") {
            model.context_length = 16384;
        } else if name.contains("8k") {
            model.context_length = 8192;
        } else if name.contains("4k") {
            model.context_length = 4096;
        } else if name.contains("128k") {
            model.context_length = 131072;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Matcher, Server};

    fn test_agent(base_url: String) -> OllamaAgent {
        let client = Arc::new(Client::new());
        OllamaAgent::new(
            "test-ollama".to_string(),
            "Test Ollama".to_string(),
            base_url,
            client,
        )
    }

    #[tokio::test]
    async fn test_health_check_success() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/api/tags")
            .with_status(200)
            .with_body(r#"{"models":[{"name":"llama3:70b"},{"name":"mistral:7b"}]}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let status = agent.health_check().await.unwrap();

        mock.assert_async().await;
        assert_eq!(status, HealthStatus::Healthy { model_count: 2 });
    }

    #[tokio::test]
    async fn test_health_check_failure() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/api/tags")
            .with_status(500)
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let status = agent.health_check().await.unwrap();

        mock.assert_async().await;
        assert_eq!(status, HealthStatus::Unhealthy);
    }

    #[tokio::test]
    async fn test_health_check_network_error() {
        let agent = test_agent("http://invalid-host-that-does-not-exist:9999".to_string());
        let result = agent.health_check().await;

        assert!(result.is_err());
        match result {
            Err(AgentError::Network(_)) => {}
            _ => panic!("Expected Network error"),
        }
    }

    #[tokio::test]
    async fn test_list_models_with_enrichment() {
        let mut server = Server::new_async().await;
        
        // Mock /api/tags
        let tags_mock = server
            .mock("GET", "/api/tags")
            .with_status(200)
            .with_body(r#"{"models":[{"name":"llama3:70b"}]}"#)
            .create_async()
            .await;

        // Mock /api/show for enrichment
        let show_mock = server
            .mock("POST", "/api/show")
            .match_body(Matcher::Json(serde_json::json!({"name": "llama3:70b"})))
            .with_status(200)
            .with_body(r#"{
                "capabilities": ["vision", "tools"],
                "model_info": {
                    "llama.context_length": 131072
                }
            }"#)
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let models = agent.list_models().await.unwrap();

        tags_mock.assert_async().await;
        show_mock.assert_async().await;

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "llama3:70b");
        assert_eq!(models[0].context_length, 131072);
        assert!(models[0].supports_vision);
        assert!(models[0].supports_tools);
    }

    #[tokio::test]
    async fn test_list_models_fallback_heuristics() {
        let mut server = Server::new_async().await;
        
        // Mock /api/tags
        let tags_mock = server
            .mock("GET", "/api/tags")
            .with_status(200)
            .with_body(r#"{"models":[{"name":"llava:13b"}]}"#)
            .create_async()
            .await;

        // Mock /api/show returns error (triggers heuristics fallback)
        let show_mock = server
            .mock("POST", "/api/show")
            .with_status(500)
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let models = agent.list_models().await.unwrap();

        tags_mock.assert_async().await;
        show_mock.assert_async().await;

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "llava:13b");
        // Heuristics should detect vision from name
        assert!(models[0].supports_vision);
    }

    #[tokio::test]
    async fn test_profile() {
        let agent = test_agent("http://localhost:11434".to_string());
        let profile = agent.profile();

        assert_eq!(profile.backend_type, "ollama");
        assert_eq!(profile.privacy_zone, PrivacyZone::Restricted);
        assert!(!profile.capabilities.embeddings);
        assert!(!profile.capabilities.model_lifecycle);
    }

    #[tokio::test]
    async fn test_chat_completion_success() {
        let mut server = Server::new_async().await;
        
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_body(Matcher::JsonString(
                r#"{"model":"llama3:70b","messages":[{"role":"user","content":"Hello"}],"stream":false}"#.to_string(),
            ))
            .with_status(200)
            .with_body(r#"{
                "id": "chatcmpl-123",
                "object": "chat.completion",
                "created": 1234567890,
                "model": "llama3:70b",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "Hi there!"
                        },
                        "finish_reason": "stop"
                    }
                ]
            }"#)
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let request = ChatCompletionRequest {
            model: "llama3:70b".to_string(),
            messages: vec![crate::api::types::ChatMessage {
                role: "user".to_string(),
                content: crate::api::types::MessageContent::Text {
                    content: "Hello".to_string(),
                },
                name: None,
            }],
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
        assert_eq!(response.id, "chatcmpl-123");
        assert_eq!(response.model, "llama3:70b");
        assert_eq!(response.choices.len(), 1);
    }

    #[tokio::test]
    async fn test_chat_completion_upstream_error() {
        let mut server = Server::new_async().await;
        
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(500)
            .with_body("Internal server error")
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let request = ChatCompletionRequest {
            model: "llama3:70b".to_string(),
            messages: vec![],
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

        let result = agent.chat_completion(request, None).await;

        mock.assert_async().await;
        assert!(result.is_err());
        match result {
            Err(AgentError::Upstream { status, .. }) => {
                assert_eq!(status, 500);
            }
            _ => panic!("Expected Upstream error"),
        }
    }
}
