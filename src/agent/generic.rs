//! Generic OpenAI-compatible agent implementation.
//!
//! Handles VLLM, LlamaCpp, Exo, and Generic backend types that provide
//! OpenAI-compatible APIs at /v1/models and /v1/chat/completions.

use super::{
    AgentCapabilities, AgentError, AgentProfile, HealthStatus, InferenceAgent, ModelCapability,
    PrivacyZone, StreamChunk,
};
use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};
use crate::registry::BackendType;
use async_trait::async_trait;
use axum::http::HeaderMap;
use futures_util::stream::BoxStream;
use reqwest::Client;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

/// Generic OpenAI-compatible agent implementation.
///
/// Used for backend types that implement OpenAI-compatible endpoints:
/// - VLLM
/// - LlamaCpp
/// - Exo
/// - Generic
pub struct GenericOpenAIAgent {
    /// Unique agent ID
    id: String,
    /// Human-readable name
    name: String,
    /// Backend type for profile reporting
    backend_type: BackendType,
    /// Base URL (e.g., "http://localhost:8000")
    base_url: String,
    /// Shared HTTP client for connection pooling
    client: Arc<Client>,
}

impl GenericOpenAIAgent {
    pub fn new(
        id: String,
        name: String,
        backend_type: BackendType,
        base_url: String,
        client: Arc<Client>,
    ) -> Self {
        Self {
            id,
            name,
            backend_type,
            base_url,
            client,
        }
    }
}

/// OpenAI-compatible /v1/models response format
#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<ModelData>,
}

#[derive(Deserialize)]
struct ModelData {
    id: String,
}

#[async_trait]
impl InferenceAgent for GenericOpenAIAgent {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn profile(&self) -> AgentProfile {
        let backend_type = match self.backend_type {
            BackendType::VLLM => "vllm",
            BackendType::LlamaCpp => "llamacpp",
            BackendType::Exo => "exo",
            BackendType::Generic => "generic",
            _ => "generic", // Fallback
        };

        AgentProfile {
            backend_type: backend_type.to_string(),
            version: None,
            privacy_zone: PrivacyZone::Restricted, // All generic backends are local
            capabilities: AgentCapabilities {
                embeddings: false,
                model_lifecycle: false,
                token_counting: false,
                resource_monitoring: false,
            },
        }
    }

    async fn health_check(&self) -> Result<HealthStatus, AgentError> {
        let url = format!("{}/v1/models", self.base_url);

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

        let models: ModelsResponse = serde_json::from_str(&body).map_err(|e| {
            AgentError::InvalidResponse(format!("Failed to parse models response: {}", e))
        })?;

        Ok(HealthStatus::Healthy {
            model_count: models.data.len(),
        })
    }

    async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
        let url = format!("{}/v1/models", self.base_url);

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

        let models: ModelsResponse = serde_json::from_str(&body).map_err(|e| {
            AgentError::InvalidResponse(format!("Failed to parse models response: {}", e))
        })?;

        let models = models
            .data
            .into_iter()
            .map(|m| {
                let mut model = ModelCapability {
                    id: m.id.clone(),
                    name: m.id,
                    context_length: 4096, // Default
                    supports_vision: false,
                    supports_tools: false,
                    supports_json_mode: false,
                    max_output_tokens: None,
                    capability_tier: None,
                };
                Self::apply_name_heuristics(&mut model);
                model
            })
            .collect();

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
}

impl GenericOpenAIAgent {
    /// Apply name-based heuristics for capability detection.
    fn apply_name_heuristics(model: &mut ModelCapability) {
        let name = model.id.to_lowercase();

        // Vision support heuristics
        if name.contains("vision") || name.contains("llava") || name.contains("bakllava") {
            model.supports_vision = true;
        }

        // Tool calling heuristics
        if name.contains("hermes")
            || name.contains("functionary")
            || name.contains("command")
            || name.contains("tool")
        {
            model.supports_tools = true;
            model.supports_json_mode = true;
        }

        // Context length heuristics by size
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
    use mockito::Server;

    fn test_agent(base_url: String, backend_type: BackendType) -> GenericOpenAIAgent {
        let client = Arc::new(Client::new());
        GenericOpenAIAgent::new(
            "test-generic".to_string(),
            "Test Generic".to_string(),
            backend_type,
            base_url,
            client,
        )
    }

    #[tokio::test]
    async fn test_vllm_profile() {
        let agent = test_agent("http://localhost:8000".to_string(), BackendType::VLLM);
        let profile = agent.profile();

        assert_eq!(profile.backend_type, "vllm");
        assert_eq!(profile.privacy_zone, PrivacyZone::Restricted);
    }

    #[tokio::test]
    async fn test_llamacpp_profile() {
        let agent = test_agent("http://localhost:8080".to_string(), BackendType::LlamaCpp);
        let profile = agent.profile();

        assert_eq!(profile.backend_type, "llamacpp");
    }

    #[tokio::test]
    async fn test_exo_profile() {
        let agent = test_agent("http://localhost:52415".to_string(), BackendType::Exo);
        let profile = agent.profile();

        assert_eq!(profile.backend_type, "exo");
    }

    #[tokio::test]
    async fn test_generic_profile() {
        let agent = test_agent("http://localhost:9000".to_string(), BackendType::Generic);
        let profile = agent.profile();

        assert_eq!(profile.backend_type, "generic");
    }

    #[tokio::test]
    async fn test_health_check() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(200)
            .with_body(r#"{"data":[{"id":"meta-llama/Llama-2-7b"}]}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url(), BackendType::VLLM);
        let status = agent.health_check().await.unwrap();

        mock.assert_async().await;
        assert_eq!(status, HealthStatus::Healthy { model_count: 1 });
    }

    #[tokio::test]
    async fn test_list_models() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(200)
            .with_body(r#"{"data":[{"id":"llama-2-7b-chat"}]}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url(), BackendType::LlamaCpp);
        let models = agent.list_models().await.unwrap();

        mock.assert_async().await;
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "llama-2-7b-chat");
    }

    #[tokio::test]
    async fn test_chat_completion() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body(r#"{"id":"1","object":"chat.completion","created":123,"model":"test","choices":[{"index":0,"message":{"role":"assistant","content":"Response"},"finish_reason":"stop"}]}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url(), BackendType::VLLM);
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

        let response = agent.chat_completion(request, None).await.unwrap();

        mock.assert_async().await;
        assert_eq!(response.id, "1");
    }

    #[tokio::test]
    async fn test_error_handling() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(503)
            .with_body("Service unavailable")
            .create_async()
            .await;

        let agent = test_agent(server.url(), BackendType::Generic);
        let result = agent.health_check().await;

        mock.assert_async().await;
        assert_eq!(result.unwrap(), HealthStatus::Unhealthy);
    }
}
