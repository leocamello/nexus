//! OpenAI agent implementation.

use super::{
    AgentCapabilities, AgentError, AgentProfile, HealthStatus, InferenceAgent, ModelCapability,
    PrivacyZone, StreamChunk,
};
use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};
use async_trait::async_trait;
use axum::http::HeaderMap;
use futures_util::stream::BoxStream;
use reqwest::Client;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

/// OpenAI agent implementation.
///
/// Handles OpenAI cloud API calls with API key authentication:
/// - Health check via GET /v1/models
/// - Model listing via GET /v1/models
/// - Chat completion via POST /v1/chat/completions with Bearer token
pub struct OpenAIAgent {
    /// Unique agent ID
    id: String,
    /// Human-readable name
    name: String,
    /// Base URL (e.g., "https://api.openai.com")
    base_url: String,
    /// API key for Bearer authentication
    api_key: String,
    /// Shared HTTP client for connection pooling
    client: Arc<Client>,
}

impl OpenAIAgent {
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
}

/// OpenAI /v1/models response format
#[derive(Deserialize)]
struct OpenAIModelsResponse {
    data: Vec<OpenAIModel>,
}

#[derive(Deserialize)]
struct OpenAIModel {
    id: String,
}

#[async_trait]
impl InferenceAgent for OpenAIAgent {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn profile(&self) -> AgentProfile {
        AgentProfile {
            backend_type: "openai".to_string(),
            version: None,
            privacy_zone: PrivacyZone::Open, // Cloud service
            capabilities: AgentCapabilities {
                embeddings: false,   // Phase 1: Not implemented
                model_lifecycle: false,
                token_counting: false, // Phase 1: Heuristic only (tiktoken in F14)
                resource_monitoring: false,
            },
        }
    }

    async fn health_check(&self) -> Result<HealthStatus, AgentError> {
        let url = format!("{}/v1/models", self.base_url);

        let response = self
            .client
            .get(&url)
            .header("authorization", format!("Bearer {}", self.api_key))
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

        let models: OpenAIModelsResponse = serde_json::from_str(&body).map_err(|e| {
            AgentError::InvalidResponse(format!("Failed to parse OpenAI models response: {}", e))
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
            .header("authorization", format!("Bearer {}", self.api_key))
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

        let models: OpenAIModelsResponse = serde_json::from_str(&body).map_err(|e| {
            AgentError::InvalidResponse(format!("Failed to parse OpenAI models response: {}", e))
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

        // Prefer config API key, but allow header override
        let auth_header = if let Some(headers) = headers {
            headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        } else {
            None
        };

        let auth = auth_header.unwrap_or_else(|| format!("Bearer {}", self.api_key));

        let response = self
            .client
            .post(&url)
            .header("authorization", auth)
            .json(&request)
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

        // Prefer config API key, but allow header override
        let auth_header = if let Some(headers) = headers {
            headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        } else {
            None
        };

        let auth = auth_header.unwrap_or_else(|| format!("Bearer {}", self.api_key));

        let response = self
            .client
            .post(&url)
            .header("authorization", auth)
            .json(&request)
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

impl OpenAIAgent {
    /// Apply OpenAI-specific name heuristics for capability detection.
    fn apply_name_heuristics(model: &mut ModelCapability) {
        let name = model.id.to_lowercase();

        // Vision support
        if name.contains("vision") || name.contains("gpt-4o") {
            model.supports_vision = true;
        }

        // Tool calling (most modern OpenAI models support it)
        if name.contains("gpt-4")
            || name.contains("gpt-3.5-turbo")
            || name.starts_with("gpt-4o")
        {
            model.supports_tools = true;
            model.supports_json_mode = true;
        }

        // Context lengths
        if name.contains("gpt-4-turbo") || name.contains("gpt-4o") {
            model.context_length = 128000;
        } else if name.contains("gpt-4-32k") {
            model.context_length = 32768;
        } else if name.contains("gpt-4") {
            model.context_length = 8192;
        } else if name.contains("gpt-3.5-turbo-16k") {
            model.context_length = 16384;
        } else if name.contains("gpt-3.5") {
            model.context_length = 4096;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    fn test_agent(base_url: String, api_key: String) -> OpenAIAgent {
        let client = Arc::new(Client::new());
        OpenAIAgent::new(
            "test-openai".to_string(),
            "Test OpenAI".to_string(),
            base_url,
            api_key,
            client,
        )
    }

    #[tokio::test]
    async fn test_health_check_success() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .match_header("authorization", "Bearer sk-test123")
            .with_status(200)
            .with_body(r#"{"data":[{"id":"gpt-4"},{"id":"gpt-3.5-turbo"}]}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-test123".to_string());
        let status = agent.health_check().await.unwrap();

        mock.assert_async().await;
        assert_eq!(status, HealthStatus::Healthy { model_count: 2 });
    }

    #[tokio::test]
    async fn test_health_check_unauthorized() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(401)
            .create_async()
            .await;

        let agent = test_agent(server.url(), "invalid-key".to_string());
        let status = agent.health_check().await.unwrap();

        mock.assert_async().await;
        assert_eq!(status, HealthStatus::Unhealthy);
    }

    #[tokio::test]
    async fn test_list_models_with_heuristics() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(200)
            .with_body(r#"{"data":[{"id":"gpt-4o"},{"id":"gpt-3.5-turbo"}]}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-test".to_string());
        let models = agent.list_models().await.unwrap();

        mock.assert_async().await;
        assert_eq!(models.len(), 2);
        
        // GPT-4o should have vision
        let gpt4o = models.iter().find(|m| m.id == "gpt-4o").unwrap();
        assert!(gpt4o.supports_vision);
        assert_eq!(gpt4o.context_length, 128000);
    }

    #[tokio::test]
    async fn test_chat_completion_with_bearer_auth() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("authorization", "Bearer sk-test123")
            .with_status(200)
            .with_body(r#"{"id":"cmpl-1","object":"chat.completion","created":1234567890,"model":"gpt-4","choices":[{"index":0,"message":{"role":"assistant","content":"Hi"},"finish_reason":"stop"}]}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-test123".to_string());
        let request = ChatCompletionRequest {
            model: "gpt-4".to_string(),
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
        assert_eq!(response.model, "gpt-4");
    }

    #[tokio::test]
    async fn test_profile() {
        let agent = test_agent("https://api.openai.com".to_string(), "sk-test".to_string());
        let profile = agent.profile();

        assert_eq!(profile.backend_type, "openai");
        assert_eq!(profile.privacy_zone, PrivacyZone::Open);
    }

    #[tokio::test]
    async fn test_network_error() {
        let agent = test_agent("http://invalid:9999".to_string(), "sk-test".to_string());
        let result = agent.health_check().await;

        assert!(result.is_err());
        matches!(result.unwrap_err(), AgentError::Network(_));
    }
}
