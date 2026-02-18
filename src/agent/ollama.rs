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
    /// Privacy zone classification from config
    privacy_zone: PrivacyZone,
    /// Capability tier from config
    capability_tier: Option<u8>,
}

impl OllamaAgent {
    pub fn new(
        id: String,
        name: String,
        base_url: String,
        client: Arc<Client>,
        privacy_zone: PrivacyZone,
        capability_tier: Option<u8>,
    ) -> Self {
        Self {
            id,
            name,
            base_url,
            client,
            privacy_zone,
            capability_tier,
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
            privacy_zone: self.privacy_zone,
            capabilities: AgentCapabilities {
                embeddings: true,
                model_lifecycle: false,
                token_counting: false,
                resource_monitoring: false,
            },
            capability_tier: self.capability_tier,
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

        let mut req = self
            .client
            .post(&url)
            .json(&request)
            .timeout(Duration::from_secs(120));

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
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
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

        let mut req = self
            .client
            .post(&url)
            .json(&request)
            .timeout(Duration::from_secs(120));

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
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
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

    /// Generate embeddings via Ollama's POST /api/embed endpoint.
    async fn embeddings(
        &self,
        model: &str,
        input: Vec<String>,
    ) -> Result<Vec<Vec<f32>>, AgentError> {
        let mut results = Vec::with_capacity(input.len());

        for text in &input {
            let url = format!("{}/api/embed", self.base_url);
            let body = serde_json::json!({
                "model": model,
                "input": text,
            });

            let response = self
                .client
                .post(&url)
                .json(&body)
                .timeout(Duration::from_secs(60))
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() {
                        AgentError::Timeout(60000)
                    } else {
                        AgentError::Network(e.to_string())
                    }
                })?;

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let error_body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                return Err(AgentError::Upstream {
                    status,
                    message: error_body,
                });
            }

            let body: serde_json::Value = response.json().await.map_err(|e| {
                AgentError::InvalidResponse(format!("Failed to parse Ollama embed response: {}", e))
            })?;

            // Ollama returns { "embeddings": [[...]] }
            let embeddings = body["embeddings"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|v| v.as_array())
                .ok_or_else(|| {
                    AgentError::InvalidResponse(
                        "Missing embeddings array in Ollama response".to_string(),
                    )
                })?;

            let vector: Vec<f32> = embeddings
                .iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect();

            results.push(vector);
        }

        Ok(results)
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

        // Context length heuristics by size (check larger values first)
        if name.contains("128k") {
            model.context_length = 131072;
        } else if name.contains("32k") {
            model.context_length = 32768;
        } else if name.contains("16k") {
            model.context_length = 16384;
        } else if name.contains("8k") {
            model.context_length = 8192;
        } else if name.contains("4k") {
            model.context_length = 4096;
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
            PrivacyZone::Restricted,
            None,
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

        assert!(matches!(result, Err(AgentError::Network(_))));
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
            .with_body(
                r#"{
                "capabilities": ["vision", "tools"],
                "model_info": {
                    "llama.context_length": 131072
                }
            }"#,
            )
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
        assert!(profile.capabilities.embeddings);
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
                function_call: None,
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
        assert!(matches!(
            result,
            Err(AgentError::Upstream { status: 500, .. })
        ));
    }

    #[test]
    fn test_name_heuristics_vision_model() {
        let mut model = ModelCapability {
            id: "llava:13b".to_string(),
            name: "llava:13b".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        assert!(model.supports_vision);
    }

    #[test]
    fn test_name_heuristics_tool_model() {
        let mut model = ModelCapability {
            id: "command-r:35b".to_string(),
            name: "command-r:35b".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        assert!(model.supports_tools);
    }

    #[test]
    fn test_name_heuristics_context_length_128k() {
        let mut model = ModelCapability {
            id: "llama3:128k".to_string(),
            name: "llama3:128k".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        assert_eq!(model.context_length, 131072);
    }

    #[test]
    fn test_name_heuristics_no_special_name() {
        let mut model = ModelCapability {
            id: "phi:2.7b".to_string(),
            name: "phi:2.7b".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        assert!(!model.supports_vision);
        assert!(!model.supports_tools);
        assert_eq!(model.context_length, 4096);
    }

    #[test]
    fn test_name_heuristics_llava_vision() {
        let mut model = ModelCapability {
            id: "llava:7b".to_string(),
            name: "llava:7b".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        assert!(model.supports_vision);
    }

    #[test]
    fn test_name_heuristics_codellama_no_tools() {
        let mut model = ModelCapability {
            id: "codellama:34b".to_string(),
            name: "codellama:34b".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        assert!(!model.supports_tools);
    }

    #[test]
    fn test_name_heuristics_mixtral_no_context_marker() {
        let mut model = ModelCapability {
            id: "mixtral:8x22b".to_string(),
            name: "mixtral:8x22b".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        // No context length marker in name, so original value is preserved
        assert_eq!(model.context_length, 4096);
    }

    #[test]
    fn test_name_heuristics_phi3_128k_context() {
        let mut model = ModelCapability {
            id: "phi3:128k".to_string(),
            name: "phi3:128k".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        assert_eq!(model.context_length, 131072);
    }

    #[test]
    fn test_name_heuristics_random_model_defaults() {
        let mut model = ModelCapability {
            id: "some-random-model".to_string(),
            name: "some-random-model".to_string(),
            context_length: 8192,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        assert!(!model.supports_vision);
        assert!(!model.supports_tools);
        assert_eq!(model.context_length, 8192);
    }

    #[test]
    fn test_name_heuristics_empty_name_defaults() {
        let mut model = ModelCapability {
            id: String::new(),
            name: String::new(),
            context_length: 8192,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        assert!(!model.supports_vision);
        assert!(!model.supports_tools);
        assert_eq!(model.context_length, 8192);
    }

    #[tokio::test]
    async fn test_chat_completion_stream_success() {
        use futures_util::stream::StreamExt;

        let mut server = Server::new_async().await;
        let sse_body = "data: {\"id\":\"chatcmpl-123\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n";
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(sse_body)
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let request = ChatCompletionRequest {
            model: "llama3".to_string(),
            messages: vec![crate::api::types::ChatMessage {
                role: "user".to_string(),
                content: crate::api::types::MessageContent::Text {
                    content: "Hi".to_string(),
                },
                name: None,
                function_call: None,
            }],
            stream: true,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            extra: std::collections::HashMap::new(),
        };
        let result = agent.chat_completion_stream(request, None).await;
        assert!(result.is_ok());
        let mut stream = result.unwrap();
        let chunk = stream.next().await;
        assert!(chunk.is_some());
        let chunk = chunk.unwrap().unwrap();
        assert!(chunk.data.contains("Hello"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_chat_completion_stream_error() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(500)
            .with_body("Internal Server Error")
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let request = ChatCompletionRequest {
            model: "llama3".to_string(),
            messages: vec![],
            stream: true,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            extra: std::collections::HashMap::new(),
        };
        let result = agent.chat_completion_stream(request, None).await;
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("Expected error"),
        };
        match err {
            AgentError::Upstream { status, .. } => assert_eq!(status, 500),
            other => panic!("Expected Upstream error, got: {:?}", other),
        }
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_embeddings_success() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/api/embed")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"embeddings":[[0.1,0.2,0.3]]}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let result = agent
            .embeddings("llama3", vec!["hello world".to_string()])
            .await;
        assert!(result.is_ok());
        let embeddings = result.unwrap();
        assert_eq!(embeddings.len(), 1);
        assert_eq!(embeddings[0].len(), 3);
        assert!((embeddings[0][0] - 0.1).abs() < f32::EPSILON);
        assert!((embeddings[0][1] - 0.2).abs() < f32::EPSILON);
        assert!((embeddings[0][2] - 0.3).abs() < f32::EPSILON);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_embeddings_error() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/api/embed")
            .with_status(500)
            .with_body("Internal Server Error")
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let result = agent
            .embeddings("llama3", vec!["hello world".to_string()])
            .await;
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("Expected error"),
        };
        match err {
            AgentError::Upstream { status, .. } => assert_eq!(status, 500),
            other => panic!("Expected Upstream error, got: {:?}", other),
        }
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_chat_completion_non_streaming_network_error() {
        let agent = test_agent("http://invalid-host-that-does-not-exist:9999".to_string());
        let request = ChatCompletionRequest {
            model: "llama3".to_string(),
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
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("Expected error"),
        };
        assert!(
            matches!(err, AgentError::Network(_) | AgentError::Timeout(_)),
            "Expected Network or Timeout error, got: {:?}",
            err
        );
    }

    #[tokio::test]
    async fn test_chat_completion_non_streaming_invalid_json() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("not valid json")
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let request = ChatCompletionRequest {
            model: "llama3".to_string(),
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
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("Expected error"),
        };
        assert!(
            matches!(err, AgentError::InvalidResponse(_)),
            "Expected InvalidResponse error, got: {:?}",
            err
        );
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_list_models_api_error() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/api/tags")
            .with_status(500)
            .with_body("Internal error")
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let result = agent.list_models().await;

        mock.assert_async().await;
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("Expected error"),
        };
        match err {
            AgentError::Upstream { status, .. } => assert_eq!(status, 500),
            other => panic!("Expected Upstream error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_embeddings_single_input() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/api/embed")
            .with_status(500)
            .with_body("model not found")
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let result = agent
            .embeddings("nomic-embed-text", vec!["hello".to_string()])
            .await;

        mock.assert_async().await;
        assert!(matches!(
            result,
            Err(AgentError::Upstream { status: 500, .. })
        ));
    }

    #[tokio::test]
    async fn test_embeddings_invalid_response() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/api/embed")
            .with_status(200)
            .with_body(r#"{"not_embeddings": true}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let result = agent
            .embeddings("nomic-embed-text", vec!["hello".to_string()])
            .await;

        mock.assert_async().await;
        assert!(matches!(result, Err(AgentError::InvalidResponse(_))));
    }

    #[tokio::test]
    async fn test_embeddings_multiple_inputs() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/api/embed")
            .with_status(200)
            .with_body(r#"{"embeddings":[[0.1, 0.2]]}"#)
            .expect(2)
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let result = agent
            .embeddings(
                "nomic-embed-text",
                vec!["hello".to_string(), "world".to_string()],
            )
            .await;

        mock.assert_async().await;
        let embeddings = result.unwrap();
        assert_eq!(embeddings.len(), 2);
    }

    #[tokio::test]
    async fn test_health_check_invalid_json() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/api/tags")
            .with_status(200)
            .with_body("not valid json")
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let result = agent.health_check().await;

        mock.assert_async().await;
        assert!(matches!(result, Err(AgentError::InvalidResponse(_))));
    }

    #[tokio::test]
    async fn test_list_models_invalid_json() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/api/tags")
            .with_status(200)
            .with_body("not json")
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let result = agent.list_models().await;

        mock.assert_async().await;
        assert!(matches!(result, Err(AgentError::InvalidResponse(_))));
    }

    #[test]
    fn test_name_heuristics_context_128k() {
        let mut model = ModelCapability {
            id: "llama-128k".to_string(),
            name: "llama-128k".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        assert_eq!(model.context_length, 131072);
    }

    #[test]
    fn test_name_heuristics_context_32k() {
        let mut model = ModelCapability {
            id: "llama-32k".to_string(),
            name: "llama-32k".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        assert_eq!(model.context_length, 32768);
    }

    #[test]
    fn test_name_heuristics_context_16k() {
        let mut model = ModelCapability {
            id: "llama-16k".to_string(),
            name: "llama-16k".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        assert_eq!(model.context_length, 16384);
    }

    #[test]
    fn test_name_heuristics_context_4k() {
        let mut model = ModelCapability {
            id: "tiny-4k".to_string(),
            name: "tiny-4k".to_string(),
            context_length: 2048,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        assert_eq!(model.context_length, 4096);
    }

    #[test]
    fn test_name_heuristics_command_tools() {
        let mut model = ModelCapability {
            id: "command-r-plus".to_string(),
            name: "command-r-plus".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        assert!(model.supports_tools);
    }

    #[test]
    fn test_name_heuristics_functionary_tools() {
        let mut model = ModelCapability {
            id: "functionary-v2".to_string(),
            name: "functionary-v2".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        assert!(model.supports_tools);
    }

    #[test]
    fn test_name_heuristics_hermes_tools() {
        let mut model = ModelCapability {
            id: "hermes-3-llama".to_string(),
            name: "hermes-3-llama".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        assert!(model.supports_tools);
    }

    #[test]
    fn test_name_heuristics_bakllava_vision() {
        let mut model = ModelCapability {
            id: "bakllava:7b".to_string(),
            name: "bakllava:7b".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        assert!(model.supports_vision);
    }

    #[test]
    fn test_name_heuristics_context_8k() {
        let mut model = ModelCapability {
            id: "mistral-8k".to_string(),
            name: "mistral-8k".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OllamaAgent::apply_name_heuristics(&mut model);
        assert_eq!(model.context_length, 8192);
    }

    #[tokio::test]
    async fn test_chat_completion_stream_data_chunks() {
        use futures_util::stream::StreamExt;

        let mut server = Server::new_async().await;
        let sse_body =
            "data: {\"id\":\"chatcmpl-123\",\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n";
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(sse_body)
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let request = ChatCompletionRequest {
            model: "llama3:8b".to_string(),
            messages: vec![],
            stream: true,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            extra: std::collections::HashMap::new(),
        };

        let result = agent.chat_completion_stream(request, None).await;
        assert!(result.is_ok());
        let mut stream = result.unwrap();
        let chunk = stream.next().await;
        assert!(chunk.is_some());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_chat_completion_stream_upstream_error() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(500)
            .with_body("error")
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let request = ChatCompletionRequest {
            model: "llama3:8b".to_string(),
            messages: vec![],
            stream: true,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            extra: std::collections::HashMap::new(),
        };

        let result = agent.chat_completion_stream(request, None).await;
        assert!(matches!(
            result,
            Err(AgentError::Upstream { status: 500, .. })
        ));
        mock.assert_async().await;
    }

    #[test]
    fn test_profile_with_capability_tier() {
        let client = Arc::new(Client::new());
        let agent = OllamaAgent::new(
            "test".to_string(),
            "Test".to_string(),
            "http://localhost".to_string(),
            client,
            PrivacyZone::Open,
            Some(5),
        );
        let profile = agent.profile();
        assert_eq!(profile.capability_tier, Some(5));
        assert_eq!(profile.privacy_zone, PrivacyZone::Open);
    }

    #[tokio::test]
    async fn test_list_models_upstream_error() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/api/tags")
            .with_status(500)
            .with_body("server error")
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let result = agent.list_models().await;

        mock.assert_async().await;
        assert!(matches!(
            result,
            Err(AgentError::Upstream { status: 500, .. })
        ));
    }

    #[tokio::test]
    async fn test_chat_completion_with_auth_header() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("authorization", "Bearer ollama-key")
            .with_status(200)
            .with_body(r#"{"id":"1","object":"chat.completion","created":123,"model":"test","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let request = ChatCompletionRequest {
            model: "test".to_string(),
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

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("authorization", "Bearer ollama-key".parse().unwrap());
        let response = agent
            .chat_completion(request, Some(&headers))
            .await
            .unwrap();
        assert_eq!(response.id, "1");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_embeddings_api_success() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/api/embed")
            .with_status(200)
            .with_body(r#"{"embeddings":[[0.1,0.2,0.3]]}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let result = agent
            .embeddings("llama3:8b", vec!["hello".to_string()])
            .await
            .unwrap();

        mock.assert_async().await;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 3);
    }

    #[tokio::test]
    async fn test_embeddings_api_upstream_error() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/api/embed")
            .with_status(500)
            .with_body("fail")
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let result = agent.embeddings("test", vec!["hello".to_string()]).await;

        mock.assert_async().await;
        assert!(matches!(
            result,
            Err(AgentError::Upstream { status: 500, .. })
        ));
    }

    #[tokio::test]
    async fn test_embeddings_api_invalid_response() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/api/embed")
            .with_status(200)
            .with_body(r#"{"embeddings":"not_array"}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let result = agent.embeddings("test", vec!["hello".to_string()]).await;

        mock.assert_async().await;
        assert!(matches!(result, Err(AgentError::InvalidResponse(_))));
    }

    #[tokio::test]
    async fn test_chat_completion_stream_success_with_data() {
        use futures_util::stream::StreamExt;

        let mut server = Server::new_async().await;
        let sse_body = "data: {\"id\":\"chatcmpl-123\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n";
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(sse_body)
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let request = ChatCompletionRequest {
            model: "test".to_string(),
            messages: vec![],
            stream: true,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            extra: std::collections::HashMap::new(),
        };
        let mut stream = agent.chat_completion_stream(request, None).await.unwrap();
        let chunk = stream.next().await.unwrap().unwrap();
        assert!(chunk.data.contains("Hello"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_chat_completion_invalid_json_response() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body("not json")
            .create_async()
            .await;

        let agent = test_agent(server.url());
        let request = ChatCompletionRequest {
            model: "test".to_string(),
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
        assert!(matches!(result, Err(AgentError::InvalidResponse(_))));
        mock.assert_async().await;
    }
}
