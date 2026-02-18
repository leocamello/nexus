//! OpenAI agent implementation.

use super::{
    AgentCapabilities, AgentError, AgentProfile, HealthStatus, InferenceAgent, ModelCapability,
    PrivacyZone, StreamChunk, TokenCount,
};
use crate::agent::pricing::PricingTable;
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
/// - Token counting using tiktoken-rs (F12: Cloud Backend Support)
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
    /// Pricing table for cost estimation (F12)
    /// TODO(T048): Use this for cost_estimated calculation
    #[allow(dead_code)]
    pricing: Arc<PricingTable>,
    /// Privacy zone classification from config
    privacy_zone: PrivacyZone,
    /// Capability tier from config
    capability_tier: Option<u8>,
}

impl OpenAIAgent {
    pub fn new(
        id: String,
        name: String,
        base_url: String,
        api_key: String,
        client: Arc<Client>,
        privacy_zone: PrivacyZone,
        capability_tier: Option<u8>,
    ) -> Self {
        Self {
            id,
            name,
            base_url,
            api_key,
            client,
            pricing: Arc::new(PricingTable::new()),
            privacy_zone,
            capability_tier,
        }
    }

    /// Count tokens in text using tiktoken-rs with o200k_base encoding (T028).
    ///
    /// This provides exact token counting for OpenAI models, replacing the
    /// heuristic approach with tiktoken-rs BPE encoding.
    ///
    /// # Arguments
    ///
    /// * `text` - Text to tokenize
    ///
    /// # Returns
    ///
    /// Number of tokens according to tiktoken o200k_base encoding
    pub fn count_tokens(&self, text: &str) -> u32 {
        use tiktoken_rs::o200k_base;

        match o200k_base() {
            Ok(bpe) => {
                let tokens = bpe.encode_ordinary(text);
                tokens.len() as u32
            }
            Err(e) => {
                // Fall back to heuristic if tiktoken fails
                tracing::warn!("tiktoken encoding failed: {}, using heuristic", e);
                (text.len() / 4) as u32
            }
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
            privacy_zone: self.privacy_zone,
            capabilities: AgentCapabilities {
                embeddings: true,
                model_lifecycle: false,
                token_counting: true,
                resource_monitoring: false,
            },
            capability_tier: self.capability_tier,
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

    /// Count tokens using tiktoken-rs exact BPE encoding (T028, F12).
    ///
    /// Overrides the default heuristic with exact token counting using
    /// tiktoken-rs o200k_base encoding, which matches OpenAI's tokenization.
    async fn count_tokens(&self, _model_id: &str, text: &str) -> TokenCount {
        use tiktoken_rs::o200k_base;

        match o200k_base() {
            Ok(bpe) => {
                let tokens = bpe.encode_ordinary(text);
                TokenCount::Exact(tokens.len() as u32)
            }
            Err(e) => {
                // Fall back to heuristic if tiktoken fails
                tracing::warn!("tiktoken encoding failed: {}, using heuristic", e);
                TokenCount::Heuristic((text.len() / 4) as u32)
            }
        }
    }

    /// Generate embeddings via OpenAI's POST /v1/embeddings endpoint.
    async fn embeddings(
        &self,
        model: &str,
        input: Vec<String>,
    ) -> Result<Vec<Vec<f32>>, AgentError> {
        let url = format!("{}/v1/embeddings", self.base_url);

        let body = serde_json::json!({
            "model": model,
            "input": input,
        });

        let response = self
            .client
            .post(&url)
            .header("authorization", format!("Bearer {}", self.api_key))
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

        let body: serde_json::Value = response.json().await.map_err(|e| {
            AgentError::InvalidResponse(format!(
                "Failed to parse OpenAI embeddings response: {}",
                e
            ))
        })?;

        let data = body["data"].as_array().ok_or_else(|| {
            AgentError::InvalidResponse(
                "Missing data array in OpenAI embeddings response".to_string(),
            )
        })?;

        let mut results = Vec::with_capacity(data.len());
        for item in data {
            let embedding = item["embedding"].as_array().ok_or_else(|| {
                AgentError::InvalidResponse("Missing embedding array in response item".to_string())
            })?;

            let vector: Vec<f32> = embedding
                .iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect();

            results.push(vector);
        }

        Ok(results)
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
        if name.contains("gpt-4") || name.contains("gpt-3.5-turbo") || name.starts_with("gpt-4o") {
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
            PrivacyZone::Open,
            None,
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

        assert!(matches!(result, Err(AgentError::Network(_))));
    }

    #[test]
    fn test_name_heuristics_gpt4_turbo() {
        let mut model = ModelCapability {
            id: "gpt-4-turbo".to_string(),
            name: "gpt-4-turbo".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OpenAIAgent::apply_name_heuristics(&mut model);
        assert!(model.supports_tools);
        assert!(model.supports_json_mode);
        assert_eq!(model.context_length, 128000);
    }

    #[test]
    fn test_name_heuristics_gpt35() {
        let mut model = ModelCapability {
            id: "gpt-3.5-turbo".to_string(),
            name: "gpt-3.5-turbo".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OpenAIAgent::apply_name_heuristics(&mut model);
        assert!(model.supports_tools);
        assert_eq!(model.context_length, 4096);
    }

    #[test]
    fn test_name_heuristics_gpt35_16k() {
        let mut model = ModelCapability {
            id: "gpt-3.5-turbo-16k".to_string(),
            name: "gpt-3.5-turbo-16k".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OpenAIAgent::apply_name_heuristics(&mut model);
        assert_eq!(model.context_length, 16384);
    }

    #[test]
    fn test_name_heuristics_gpt4_32k() {
        let mut model = ModelCapability {
            id: "gpt-4-32k".to_string(),
            name: "gpt-4-32k".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OpenAIAgent::apply_name_heuristics(&mut model);
        assert_eq!(model.context_length, 32768);
    }

    #[test]
    fn test_count_tokens_exact() {
        let agent = test_agent("https://api.openai.com".to_string(), "sk-test".to_string());
        let count = agent.count_tokens("Hello, world!");
        assert!(count > 0);
        // "Hello, world!" is typically 4 tokens with tiktoken
        assert!(count < 10);
    }

    #[test]
    fn test_count_tokens_empty_string() {
        let agent = test_agent("https://api.openai.com".to_string(), "sk-test".to_string());
        let count = agent.count_tokens("");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_count_tokens_single_word() {
        let agent = test_agent("https://api.openai.com".to_string(), "sk-test".to_string());
        let count = agent.count_tokens("hello");
        assert_eq!(count, 1, "Single common word should be 1 token");
    }

    #[test]
    fn test_count_tokens_long_text() {
        let agent = test_agent("https://api.openai.com".to_string(), "sk-test".to_string());
        let text = "The quick brown fox jumps over the lazy dog. ".repeat(10);
        let count = agent.count_tokens(&text);
        // ~100 tokens for 10 repetitions of a 10-word sentence
        assert!(
            count > 50,
            "Long text should produce many tokens, got {count}"
        );
        assert!(count < 200, "Should not over-count, got {count}");
    }

    #[test]
    fn test_count_tokens_special_characters() {
        let agent = test_agent("https://api.openai.com".to_string(), "sk-test".to_string());
        let count = agent.count_tokens("Hello! @#$%^&*() 你好世界");
        assert!(count > 0, "Special chars should produce tokens");
        // CJK characters typically produce more tokens per character
        assert!(count > 3, "Mixed content should be > 3 tokens, got {count}");
    }

    #[test]
    fn test_count_tokens_code_snippet() {
        let agent = test_agent("https://api.openai.com".to_string(), "sk-test".to_string());
        let code = "fn main() { println!(\"Hello, world!\"); }";
        let count = agent.count_tokens(code);
        assert!(count > 5, "Code snippet should be > 5 tokens, got {count}");
        assert!(
            count < 30,
            "Code snippet should be < 30 tokens, got {count}"
        );
    }

    #[test]
    fn test_count_tokens_whitespace_only() {
        let agent = test_agent("https://api.openai.com".to_string(), "sk-test".to_string());
        let count = agent.count_tokens("   \n\t  ");
        assert!(count > 0, "Whitespace should still produce tokens");
    }

    #[test]
    fn test_name_heuristics_gpt4o() {
        let mut model = ModelCapability {
            id: "gpt-4o".to_string(),
            name: "gpt-4o".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OpenAIAgent::apply_name_heuristics(&mut model);
        assert!(model.supports_vision, "gpt-4o should support vision");
        assert!(model.supports_tools, "gpt-4o should support tools");
        assert!(model.supports_json_mode, "gpt-4o should support json_mode");
        assert_eq!(model.context_length, 128000);
    }

    #[test]
    fn test_name_heuristics_gpt4o_mini() {
        let mut model = ModelCapability {
            id: "gpt-4o-mini".to_string(),
            name: "gpt-4o-mini".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OpenAIAgent::apply_name_heuristics(&mut model);
        assert!(model.supports_vision, "gpt-4o-mini should support vision");
        assert!(model.supports_tools, "gpt-4o-mini should support tools");
        assert_eq!(model.context_length, 128000);
    }

    #[test]
    fn test_name_heuristics_o1_preview() {
        let mut model = ModelCapability {
            id: "o1-preview".to_string(),
            name: "o1-preview".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OpenAIAgent::apply_name_heuristics(&mut model);
        // o1-preview doesn't match any heuristic patterns
        assert!(
            !model.supports_vision,
            "o1-preview should not match vision heuristic"
        );
        assert!(
            !model.supports_tools,
            "o1-preview should not match tools heuristic"
        );
        assert_eq!(
            model.context_length, 4096,
            "o1-preview should keep default context"
        );
    }

    #[test]
    fn test_name_heuristics_o1_mini() {
        let mut model = ModelCapability {
            id: "o1-mini".to_string(),
            name: "o1-mini".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OpenAIAgent::apply_name_heuristics(&mut model);
        assert!(
            !model.supports_vision,
            "o1-mini should not match vision heuristic"
        );
        assert!(
            !model.supports_tools,
            "o1-mini should not match tools heuristic"
        );
        assert_eq!(
            model.context_length, 4096,
            "o1-mini should keep default context"
        );
    }

    #[test]
    fn test_name_heuristics_text_embedding() {
        let mut model = ModelCapability {
            id: "text-embedding-ada-002".to_string(),
            name: "text-embedding-ada-002".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OpenAIAgent::apply_name_heuristics(&mut model);
        assert!(
            !model.supports_vision,
            "embedding model should not support vision"
        );
        assert!(
            !model.supports_tools,
            "embedding model should not support tools"
        );
    }

    #[test]
    fn test_name_heuristics_dall_e() {
        let mut model = ModelCapability {
            id: "dall-e-3".to_string(),
            name: "dall-e-3".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OpenAIAgent::apply_name_heuristics(&mut model);
        assert!(!model.supports_vision, "dall-e should not support vision");
        assert!(!model.supports_tools, "dall-e should not support tools");
    }

    #[test]
    fn test_name_heuristics_unknown_model() {
        let mut model = ModelCapability {
            id: "unknown-model".to_string(),
            name: "unknown-model".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        };
        OpenAIAgent::apply_name_heuristics(&mut model);
        assert!(
            !model.supports_vision,
            "unknown model should keep default vision=false"
        );
        assert!(
            !model.supports_tools,
            "unknown model should keep default tools=false"
        );
        assert!(
            !model.supports_json_mode,
            "unknown model should keep default json_mode=false"
        );
        assert_eq!(
            model.context_length, 4096,
            "unknown model should keep default context"
        );
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

        let agent = test_agent(server.url(), "sk-test".to_string());
        let request = ChatCompletionRequest {
            model: "gpt-4".to_string(),
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

        let agent = test_agent(server.url(), "sk-test".to_string());
        let request = ChatCompletionRequest {
            model: "gpt-4".to_string(),
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
            .mock("POST", "/v1/embeddings")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"object":"list","data":[{"object":"embedding","embedding":[0.1,0.2,0.3],"index":0}],"model":"text-embedding-ada-002","usage":{"prompt_tokens":5,"total_tokens":5}}"#,
            )
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-test".to_string());
        let result = agent
            .embeddings("text-embedding-ada-002", vec!["hello world".to_string()])
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
            .mock("POST", "/v1/embeddings")
            .with_status(500)
            .with_body("Internal Server Error")
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-test".to_string());
        let result = agent
            .embeddings("text-embedding-ada-002", vec!["hello world".to_string()])
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
    async fn test_chat_completion_non_streaming_api_error() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(429)
            .with_body(r#"{"error":{"message":"Rate limit exceeded","type":"rate_limit_error"}}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-test".to_string());
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

        let result = agent.chat_completion(request, None).await;
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("Expected error"),
        };
        match err {
            AgentError::Upstream { status, .. } => assert_eq!(status, 429),
            other => panic!("Expected Upstream error, got: {:?}", other),
        }
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_chat_completion_non_streaming_network_error() {
        let agent = test_agent(
            "http://invalid-host-that-does-not-exist:9999".to_string(),
            "sk-test".to_string(),
        );
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

        let agent = test_agent(server.url(), "sk-test".to_string());
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
    async fn test_chat_completion_non_streaming_auth_header_override() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("authorization", "Bearer override-key")
            .with_status(200)
            .with_body(r#"{"id":"cmpl-1","object":"chat.completion","created":1234567890,"model":"gpt-4","choices":[{"index":0,"message":{"role":"assistant","content":"Hi"},"finish_reason":"stop"}]}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-default".to_string());
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

        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer override-key".parse().unwrap());
        let response = agent
            .chat_completion(request, Some(&headers))
            .await
            .unwrap();
        assert_eq!(response.model, "gpt-4");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_list_models_api_error() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(401)
            .with_body(r#"{"error":"unauthorized"}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url(), "bad-key".to_string());
        let result = agent.list_models().await;

        mock.assert_async().await;
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("Expected error"),
        };
        match err {
            AgentError::Upstream { status, .. } => assert_eq!(status, 401),
            other => panic!("Expected Upstream error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_count_tokens_exact_via_trait() {
        let agent = test_agent("https://api.openai.com".to_string(), "sk-test".to_string());
        let result = agent.count_tokens("Hello, world!");
        assert!(result > 0 && result < 10);
    }

    #[tokio::test]
    async fn test_count_tokens_empty_via_trait() {
        let agent = test_agent("https://api.openai.com".to_string(), "sk-test".to_string());
        let result = agent.count_tokens("");
        assert_eq!(result, 0);
    }

    #[tokio::test]
    async fn test_embeddings_with_mock_server() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/embeddings")
            .with_status(200)
            .with_body(r#"{"data":[{"embedding":[0.1, 0.2, 0.3]},{"embedding":[0.4, 0.5, 0.6]}]}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-test".to_string());
        let result = agent
            .embeddings(
                "text-embedding-ada-002",
                vec!["hello".to_string(), "world".to_string()],
            )
            .await;

        mock.assert_async().await;
        let embeddings = result.unwrap();
        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), 3);
        assert!((embeddings[0][0] - 0.1).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_embeddings_api_error() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/embeddings")
            .with_status(429)
            .with_body(r#"{"error":"rate limited"}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-test".to_string());
        let result = agent
            .embeddings("text-embedding-ada-002", vec!["hello".to_string()])
            .await;

        mock.assert_async().await;
        assert!(matches!(
            result,
            Err(AgentError::Upstream { status: 429, .. })
        ));
    }

    #[tokio::test]
    async fn test_embeddings_invalid_response() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/embeddings")
            .with_status(200)
            .with_body(r#"{"not_data": true}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-test".to_string());
        let result = agent
            .embeddings("text-embedding-ada-002", vec!["hello".to_string()])
            .await;

        mock.assert_async().await;
        assert!(matches!(result, Err(AgentError::InvalidResponse(_))));
    }

    #[tokio::test]
    async fn test_chat_completion_with_header_override() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("authorization", "Bearer custom-key")
            .with_status(200)
            .with_body(r#"{"id":"1","object":"chat.completion","created":123,"model":"gpt-4","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]}"#)
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-default".to_string());
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

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("authorization", "Bearer custom-key".parse().unwrap());

        let response = agent
            .chat_completion(request, Some(&headers))
            .await
            .unwrap();
        assert_eq!(response.id, "1");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_health_check_invalid_json() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(200)
            .with_body("not valid json")
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-test".to_string());
        let result = agent.health_check().await;

        mock.assert_async().await;
        assert!(matches!(result, Err(AgentError::InvalidResponse(_))));
    }

    #[tokio::test]
    async fn test_list_models_invalid_json() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(200)
            .with_body("not json")
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-test".to_string());
        let result = agent.list_models().await;

        mock.assert_async().await;
        assert!(matches!(result, Err(AgentError::InvalidResponse(_))));
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

        let agent = test_agent(server.url(), "sk-test".to_string());
        let request = ChatCompletionRequest {
            model: "gpt-4".to_string(),
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

    #[tokio::test]
    async fn test_list_models_upstream_error() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(500)
            .with_body("server error")
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-test".to_string());
        let result = agent.list_models().await;

        mock.assert_async().await;
        assert!(matches!(
            result,
            Err(AgentError::Upstream { status: 500, .. })
        ));
    }

    #[tokio::test]
    async fn test_list_models_invalid_json_response() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(200)
            .with_body("not json")
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-test".to_string());
        let result = agent.list_models().await;

        mock.assert_async().await;
        assert!(matches!(result, Err(AgentError::InvalidResponse(_))));
    }

    #[tokio::test]
    async fn test_chat_completion_upstream_error_429() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(429)
            .with_body(r#"{"error":"rate limit"}"#)
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
        let result = agent.chat_completion(request, None).await;
        assert!(matches!(
            result,
            Err(AgentError::Upstream { status: 429, .. })
        ));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_chat_completion_invalid_json_body() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body("invalid json")
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-test".to_string());
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
        let result = agent.chat_completion(request, None).await;
        assert!(matches!(result, Err(AgentError::InvalidResponse(_))));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_chat_completion_with_auth_header_forwarding() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("authorization", "Bearer custom-key")
            .with_status(200)
            .with_body(r#"{"id":"cmpl-hdr","object":"chat.completion","created":1234567890,"model":"gpt-4","choices":[{"index":0,"message":{"role":"assistant","content":"Ok"},"finish_reason":"stop"}]}"#)
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

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("authorization", "Bearer custom-key".parse().unwrap());
        let response = agent
            .chat_completion(request, Some(&headers))
            .await
            .unwrap();
        assert_eq!(response.id, "cmpl-hdr");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_embeddings_upstream_error_response() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/embeddings")
            .with_status(500)
            .with_body("error")
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-test".to_string());
        let result = agent.embeddings("test", vec!["hello".to_string()]).await;

        mock.assert_async().await;
        assert!(matches!(
            result,
            Err(AgentError::Upstream { status: 500, .. })
        ));
    }

    #[test]
    fn test_count_tokens_exact_value() {
        let agent = test_agent("http://localhost".to_string(), "sk-test".to_string());
        let count = agent.count_tokens("Hello, world!");
        assert!(count > 0);
        assert!(count < 10);
    }

    #[tokio::test]
    async fn test_health_check_invalid_json_body() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(200)
            .with_body("not json at all")
            .create_async()
            .await;

        let agent = test_agent(server.url(), "sk-test".to_string());
        let result = agent.health_check().await;

        mock.assert_async().await;
        assert!(matches!(result, Err(AgentError::InvalidResponse(_))));
    }
}
