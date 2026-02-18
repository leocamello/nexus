//! Embeddings endpoint handler (F17: Embeddings API).

use crate::api::headers::{NexusTransparentHeaders, RouteReason};
use crate::api::{ApiError, AppState};
use crate::routing::RequestRequirements;
use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, instrument};

/// Input format for embedding requests — string or array of strings.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    Single(String),
    Batch(Vec<String>),
}

impl EmbeddingInput {
    /// Convert to a Vec<String> regardless of variant.
    pub fn into_vec(self) -> Vec<String> {
        match self {
            EmbeddingInput::Single(s) => vec![s],
            EmbeddingInput::Batch(v) => v,
        }
    }
}

/// Embedding request matching OpenAI format.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddingRequest {
    pub model: String,
    pub input: EmbeddingInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding_format: Option<String>,
}

/// A single embedding object in the response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddingObject {
    pub object: String,
    pub embedding: Vec<f32>,
    pub index: usize,
}

/// Token usage for embedding requests.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddingUsage {
    pub prompt_tokens: u32,
    pub total_tokens: u32,
}

/// Embedding response matching OpenAI format.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddingResponse {
    pub object: String,
    pub data: Vec<EmbeddingObject>,
    pub model: String,
    pub usage: EmbeddingUsage,
}

/// Maximum batch size for embedding requests (matches OpenAI's limit).
const MAX_EMBEDDING_BATCH_SIZE: usize = 2048;

/// POST /v1/embeddings — Handle embedding requests.
#[instrument(
    skip(state, _headers, request),
    fields(model = %request.model)
)]
pub async fn handle(
    State(state): State<Arc<AppState>>,
    _headers: HeaderMap,
    Json(request): Json<EmbeddingRequest>,
) -> Result<Response, ApiError> {
    info!(model = %request.model, "Embedding request");

    let input_texts = request.input.into_vec();
    if input_texts.is_empty() {
        return Err(ApiError::bad_request("Input must not be empty"));
    }

    if input_texts.len() > MAX_EMBEDDING_BATCH_SIZE {
        return Err(ApiError::bad_request(&format!(
            "Batch size {} exceeds maximum of {}",
            input_texts.len(),
            MAX_EMBEDDING_BATCH_SIZE
        )));
    }

    // Estimate tokens for routing
    let estimated_tokens: u32 = input_texts.iter().map(|s| s.len() as u32 / 4).sum();

    // Build requirements for routing (embedding requests have no special caps)
    let requirements = RequestRequirements {
        model: request.model.clone(),
        estimated_tokens,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
        prefers_streaming: false,
    };

    let routing_result = state
        .router
        .select_backend(&requirements, None)
        .map_err(|e| match e {
            crate::routing::RoutingError::ModelNotFound { model } => {
                ApiError::model_not_found(&model, &[])
            }
            crate::routing::RoutingError::NoHealthyBackend { model } => {
                ApiError::service_unavailable(&format!(
                    "No healthy backend available for model '{}'",
                    model
                ))
            }
            _ => ApiError::bad_gateway(&format!("Routing error: {}", e)),
        })?;

    let backend = &routing_result.backend;

    // Get agent for this backend
    let agent = state.registry.get_agent(&backend.id).ok_or_else(|| {
        ApiError::bad_gateway(&format!("No agent registered for backend '{}'", backend.id))
    })?;

    // Check that the agent supports embeddings (T020)
    if !agent.profile().capabilities.embeddings {
        return Err(ApiError::service_unavailable(&format!(
            "Backend '{}' does not support embeddings",
            backend.id
        )));
    }

    // Track pending request for load-aware routing
    let _ = state.registry.increment_pending(&backend.id);

    // Delegate to agent.embeddings()
    let vectors = agent
        .embeddings(&request.model, input_texts)
        .await
        .map_err(|e| {
            let _ = state.registry.decrement_pending(&backend.id);
            ApiError::from_agent_error(e)
        })?;

    let _ = state.registry.decrement_pending(&backend.id);

    // Build OpenAI-compatible response
    let data: Vec<EmbeddingObject> = vectors
        .into_iter()
        .enumerate()
        .map(|(i, embedding)| EmbeddingObject {
            object: "embedding".to_string(),
            embedding,
            index: i,
        })
        .collect();

    let prompt_tokens = estimated_tokens;
    let response = EmbeddingResponse {
        object: "list".to_string(),
        data,
        model: request.model,
        usage: EmbeddingUsage {
            prompt_tokens,
            total_tokens: prompt_tokens,
        },
    };

    let mut resp = Json(response).into_response();

    // Inject X-Nexus-* transparent headers (Constitution Principle III)
    let privacy_zone = agent.profile().privacy_zone;
    let nexus_headers = NexusTransparentHeaders::new(
        backend.id.clone(),
        backend.backend_type,
        RouteReason::CapabilityMatch,
        privacy_zone,
        routing_result.cost_estimated,
    );
    nexus_headers.inject_into_response(&mut resp);

    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ====================================================================
    // T014: Unit tests for embedding types
    // ====================================================================

    #[test]
    fn embedding_request_deserialize_single_input() {
        let json = r#"{"model":"text-embedding-ada-002","input":"hello world"}"#;
        let req: EmbeddingRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "text-embedding-ada-002");
        match &req.input {
            EmbeddingInput::Single(s) => assert_eq!(s, "hello world"),
            _ => panic!("Expected Single variant"),
        }
        assert!(req.encoding_format.is_none());
    }

    #[test]
    fn embedding_request_deserialize_batch_input() {
        let json = r#"{
            "model": "text-embedding-ada-002",
            "input": ["hello", "world"]
        }"#;
        let req: EmbeddingRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "text-embedding-ada-002");
        match &req.input {
            EmbeddingInput::Batch(v) => {
                assert_eq!(v.len(), 2);
                assert_eq!(v[0], "hello");
                assert_eq!(v[1], "world");
            }
            _ => panic!("Expected Batch variant"),
        }
    }

    #[test]
    fn embedding_request_with_encoding_format() {
        let json = r#"{
            "model": "text-embedding-3-small",
            "input": "test",
            "encoding_format": "float"
        }"#;
        let req: EmbeddingRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.encoding_format.as_deref(), Some("float"));
    }

    #[test]
    fn embedding_input_into_vec_single() {
        let input = EmbeddingInput::Single("hello".to_string());
        let v = input.into_vec();
        assert_eq!(v, vec!["hello".to_string()]);
    }

    #[test]
    fn embedding_input_into_vec_batch() {
        let input = EmbeddingInput::Batch(vec!["a".to_string(), "b".to_string()]);
        let v = input.into_vec();
        assert_eq!(v, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn embedding_response_serialization_matches_openai() {
        let response = EmbeddingResponse {
            object: "list".to_string(),
            data: vec![
                EmbeddingObject {
                    object: "embedding".to_string(),
                    embedding: vec![0.1, 0.2, 0.3],
                    index: 0,
                },
                EmbeddingObject {
                    object: "embedding".to_string(),
                    embedding: vec![0.4, 0.5, 0.6],
                    index: 1,
                },
            ],
            model: "text-embedding-ada-002".to_string(),
            usage: EmbeddingUsage {
                prompt_tokens: 10,
                total_tokens: 10,
            },
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["object"], "list");
        assert_eq!(json["model"], "text-embedding-ada-002");
        assert_eq!(json["data"].as_array().unwrap().len(), 2);
        assert_eq!(json["data"][0]["object"], "embedding");
        assert_eq!(json["data"][0]["index"], 0);
        assert_eq!(json["data"][1]["index"], 1);
        assert_eq!(json["usage"]["prompt_tokens"], 10);
        assert_eq!(json["usage"]["total_tokens"], 10);

        // Verify embedding vectors
        let emb0: Vec<f32> = json["data"][0]["embedding"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect();
        assert_eq!(emb0, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn embedding_response_roundtrip() {
        let response = EmbeddingResponse {
            object: "list".to_string(),
            data: vec![EmbeddingObject {
                object: "embedding".to_string(),
                embedding: vec![1.0, 2.0],
                index: 0,
            }],
            model: "test-model".to_string(),
            usage: EmbeddingUsage {
                prompt_tokens: 5,
                total_tokens: 5,
            },
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: EmbeddingResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.object, "list");
        assert_eq!(deserialized.data.len(), 1);
        assert_eq!(deserialized.data[0].embedding, vec![1.0, 2.0]);
        assert_eq!(deserialized.model, "test-model");
        assert_eq!(deserialized.usage.prompt_tokens, 5);
    }

    #[test]
    fn embedding_object_serialization() {
        let obj = EmbeddingObject {
            object: "embedding".to_string(),
            embedding: vec![0.0; 1536],
            index: 0,
        };
        let json = serde_json::to_value(&obj).unwrap();
        assert_eq!(json["object"], "embedding");
        assert_eq!(json["index"], 0);
        assert_eq!(json["embedding"].as_array().unwrap().len(), 1536);
    }

    #[test]
    fn test_max_embedding_batch_size_constant() {
        assert_eq!(MAX_EMBEDDING_BATCH_SIZE, 2048);
    }

    #[test]
    fn test_embedding_response_serialization() {
        let response = EmbeddingResponse {
            object: "list".to_string(),
            data: vec![EmbeddingObject {
                object: "embedding".to_string(),
                embedding: vec![0.1, 0.2, 0.3],
                index: 0,
            }],
            model: "test-model".to_string(),
            usage: EmbeddingUsage {
                prompt_tokens: 5,
                total_tokens: 5,
            },
        };
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["object"], "list");
        assert_eq!(json["data"][0]["embedding"].as_array().unwrap().len(), 3);
        assert_eq!(json["usage"]["prompt_tokens"], 5);
    }

    #[test]
    fn test_embedding_request_deserialization_single() {
        let json = r#"{"model": "text-embedding-ada-002", "input": "Hello world"}"#;
        let req: EmbeddingRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "text-embedding-ada-002");
        match req.input {
            EmbeddingInput::Single(s) => assert_eq!(s, "Hello world"),
            _ => panic!("Expected Single variant"),
        }
    }

    #[test]
    fn test_embedding_request_deserialization_batch() {
        let json = r#"{"model": "test", "input": ["Hello", "World"]}"#;
        let req: EmbeddingRequest = serde_json::from_str(json).unwrap();
        match req.input {
            EmbeddingInput::Batch(v) => {
                assert_eq!(v.len(), 2);
                assert_eq!(v[0], "Hello");
                assert_eq!(v[1], "World");
            }
            _ => panic!("Expected Batch variant"),
        }
    }

    #[test]
    fn test_embedding_request_with_encoding_format() {
        let json = r#"{"model": "test", "input": "hello", "encoding_format": "float"}"#;
        let req: EmbeddingRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.encoding_format, Some("float".to_string()));
    }

    #[tokio::test]
    async fn test_handle_oversized_batch() {
        use crate::api::{create_router, AppState};
        use crate::config::NexusConfig;
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model, Registry};
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use std::sync::Arc;
        use tower::Service;

        let registry = Arc::new(Registry::new());
        let backend = crate::registry::Backend::new(
            "emb-batch".to_string(),
            "Emb Batch".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        registry.add_backend(backend).unwrap();
        registry
            .update_status("emb-batch", BackendStatus::Healthy, None)
            .unwrap();
        registry
            .update_models(
                "emb-batch",
                vec![Model {
                    id: "emb-model".to_string(),
                    name: "emb-model".to_string(),
                    context_length: 4096,
                    supports_vision: false,
                    supports_tools: false,
                    supports_json_mode: false,
                    max_output_tokens: None,
                }],
            )
            .unwrap();

        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(registry, config));
        let mut app = create_router(state);

        // Create a batch larger than MAX_EMBEDDING_BATCH_SIZE (2048)
        let inputs: Vec<String> = (0..2049).map(|i| format!("input {}", i)).collect();
        let body = serde_json::json!({
            "model": "emb-model",
            "input": inputs
        });

        let request = Request::builder()
            .method("POST")
            .uri("/v1/embeddings")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let json = body_json(response).await;
        let msg = json["error"]["message"].as_str().unwrap();
        assert!(
            msg.contains("2049") && msg.contains("2048"),
            "Expected batch size error, got: {}",
            msg
        );
    }

    #[tokio::test]
    async fn test_handle_embedding_success_with_headers() {
        use crate::agent::types::{AgentCapabilities, AgentProfile, PrivacyZone};
        use crate::agent::{AgentError, HealthStatus, InferenceAgent, ModelCapability};
        use crate::api::{create_router, AppState, ChatCompletionRequest, ChatCompletionResponse};
        use crate::config::NexusConfig;
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model, Registry};
        use async_trait::async_trait;
        use axum::body::Body;
        use axum::http::{HeaderMap, Request, StatusCode};
        use futures_util::stream::BoxStream;
        use std::sync::Arc;
        use tower::Service;

        /// A mock agent that supports embeddings.
        struct EmbeddingAgent;

        #[async_trait]
        impl InferenceAgent for EmbeddingAgent {
            fn id(&self) -> &str {
                "emb-success"
            }
            fn name(&self) -> &str {
                "Embedding Agent"
            }
            fn profile(&self) -> AgentProfile {
                AgentProfile {
                    backend_type: "ollama".to_string(),
                    version: None,
                    privacy_zone: PrivacyZone::Restricted,
                    capabilities: AgentCapabilities {
                        embeddings: true,
                        model_lifecycle: false,
                        token_counting: false,
                        resource_monitoring: false,
                    },
                    capability_tier: None,
                }
            }
            async fn health_check(&self) -> Result<HealthStatus, AgentError> {
                Ok(HealthStatus::Healthy { model_count: 1 })
            }
            async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
                Ok(vec![])
            }
            async fn chat_completion(
                &self,
                _request: ChatCompletionRequest,
                _headers: Option<&HeaderMap>,
            ) -> Result<ChatCompletionResponse, AgentError> {
                Err(AgentError::Unsupported("chat_completion"))
            }
            async fn chat_completion_stream(
                &self,
                _request: ChatCompletionRequest,
                _headers: Option<&HeaderMap>,
            ) -> Result<BoxStream<'static, Result<crate::agent::StreamChunk, AgentError>>, AgentError>
            {
                Err(AgentError::Unsupported("chat_completion_stream"))
            }
            async fn embeddings(
                &self,
                _model: &str,
                _inputs: Vec<String>,
            ) -> Result<Vec<Vec<f32>>, AgentError> {
                Ok(vec![vec![0.1, 0.2, 0.3]])
            }
        }

        let registry = Arc::new(Registry::new());
        let backend = crate::registry::Backend::new(
            "emb-success".to_string(),
            "Emb Success".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        let agent: Arc<dyn InferenceAgent> = Arc::new(EmbeddingAgent);
        registry.add_backend_with_agent(backend, agent).unwrap();
        registry
            .update_status("emb-success", BackendStatus::Healthy, None)
            .unwrap();
        registry
            .update_models(
                "emb-success",
                vec![Model {
                    id: "emb-model".to_string(),
                    name: "emb-model".to_string(),
                    context_length: 4096,
                    supports_vision: false,
                    supports_tools: false,
                    supports_json_mode: false,
                    max_output_tokens: None,
                }],
            )
            .unwrap();

        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(registry, config));
        let mut app = create_router(state);

        let body = serde_json::json!({
            "model": "emb-model",
            "input": "hello world"
        });

        let request = Request::builder()
            .method("POST")
            .uri("/v1/embeddings")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Verify X-Nexus-* headers are present
        assert!(
            response.headers().contains_key("x-nexus-backend"),
            "Missing x-nexus-backend header"
        );
        assert_eq!(
            response.headers().get("x-nexus-backend").unwrap(),
            "emb-success"
        );
        assert!(
            response.headers().contains_key("x-nexus-backend-type"),
            "Missing x-nexus-backend-type header"
        );
        assert!(
            response.headers().contains_key("x-nexus-privacy-zone"),
            "Missing x-nexus-privacy-zone header"
        );

        // Verify response body is valid embedding response
        let json = body_json(response).await;
        assert_eq!(json["object"], "list");
        assert_eq!(json["data"][0]["embedding"][0], 0.1);
    }

    // ====================================================================
    // Integration-style handler tests via full axum router
    // ====================================================================

    /// Helper: read response body as JSON.
    async fn body_json(response: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn test_handle_empty_input() {
        use crate::api::{create_router, AppState};
        use crate::config::NexusConfig;
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model, Registry};
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use std::sync::Arc;
        use tower::Service;

        // Register a backend with the model so routing succeeds
        let registry = Arc::new(Registry::new());
        let backend = crate::registry::Backend::new(
            "emb-backend".to_string(),
            "Emb Backend".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        registry.add_backend(backend).unwrap();
        registry
            .update_status("emb-backend", BackendStatus::Healthy, None)
            .unwrap();
        registry
            .update_models(
                "emb-backend",
                vec![Model {
                    id: "emb-model".to_string(),
                    name: "emb-model".to_string(),
                    context_length: 4096,
                    supports_vision: false,
                    supports_tools: false,
                    supports_json_mode: false,
                    max_output_tokens: None,
                }],
            )
            .unwrap();

        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(registry, config));
        let mut app = create_router(state);

        // Empty batch input
        let body = serde_json::json!({
            "model": "emb-model",
            "input": []
        });

        let request = Request::builder()
            .method("POST")
            .uri("/v1/embeddings")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let json = body_json(response).await;
        let msg = json["error"]["message"].as_str().unwrap();
        assert!(msg.contains("empty"), "msg was: {}", msg);
    }

    #[tokio::test]
    async fn test_handle_model_not_found_embeddings() {
        use crate::api::{create_router, AppState};
        use crate::config::NexusConfig;
        use crate::registry::Registry;
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use std::sync::Arc;
        use tower::Service;

        let registry = Arc::new(Registry::new());
        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(registry, config));
        let mut app = create_router(state);

        let body = serde_json::json!({
            "model": "nonexistent-embedding-model",
            "input": "hello"
        });

        let request = Request::builder()
            .method("POST")
            .uri("/v1/embeddings")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let json = body_json(response).await;
        let msg = json["error"]["message"].as_str().unwrap();
        assert!(
            msg.contains("nonexistent-embedding-model"),
            "msg was: {}",
            msg
        );
    }

    #[tokio::test]
    async fn test_handle_unsupported_backend() {
        use crate::agent::types::{AgentCapabilities, AgentProfile, PrivacyZone};
        use crate::agent::{AgentError, HealthStatus, InferenceAgent, ModelCapability};
        use crate::api::{create_router, AppState, ChatCompletionRequest, ChatCompletionResponse};
        use crate::config::NexusConfig;
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model, Registry};
        use async_trait::async_trait;
        use axum::body::Body;
        use axum::http::{HeaderMap, Request, StatusCode};
        use futures_util::stream::BoxStream;
        use std::sync::Arc;
        use tower::Service;

        /// A mock agent that does NOT support embeddings.
        struct NoEmbeddingsAgent;

        #[async_trait]
        impl InferenceAgent for NoEmbeddingsAgent {
            fn id(&self) -> &str {
                "no-emb"
            }
            fn name(&self) -> &str {
                "No Embeddings Agent"
            }
            fn profile(&self) -> AgentProfile {
                AgentProfile {
                    backend_type: "ollama".to_string(),
                    version: None,
                    privacy_zone: PrivacyZone::Restricted,
                    capabilities: AgentCapabilities {
                        embeddings: false,
                        model_lifecycle: false,
                        token_counting: false,
                        resource_monitoring: false,
                    },
                    capability_tier: None,
                }
            }
            async fn health_check(&self) -> Result<HealthStatus, AgentError> {
                Ok(HealthStatus::Healthy { model_count: 1 })
            }
            async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
                Ok(vec![])
            }
            async fn chat_completion(
                &self,
                _request: ChatCompletionRequest,
                _headers: Option<&HeaderMap>,
            ) -> Result<ChatCompletionResponse, AgentError> {
                Err(AgentError::Unsupported("chat_completion"))
            }
            async fn chat_completion_stream(
                &self,
                _request: ChatCompletionRequest,
                _headers: Option<&HeaderMap>,
            ) -> Result<BoxStream<'static, Result<crate::agent::StreamChunk, AgentError>>, AgentError>
            {
                Err(AgentError::Unsupported("chat_completion_stream"))
            }
        }

        let registry = Arc::new(Registry::new());
        let backend = crate::registry::Backend::new(
            "no-emb".to_string(),
            "No Emb".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        let agent: Arc<dyn InferenceAgent> = Arc::new(NoEmbeddingsAgent);
        registry.add_backend_with_agent(backend, agent).unwrap();
        registry
            .update_status("no-emb", BackendStatus::Healthy, None)
            .unwrap();
        registry
            .update_models(
                "no-emb",
                vec![Model {
                    id: "emb-model".to_string(),
                    name: "emb-model".to_string(),
                    context_length: 4096,
                    supports_vision: false,
                    supports_tools: false,
                    supports_json_mode: false,
                    max_output_tokens: None,
                }],
            )
            .unwrap();

        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(registry, config));
        let mut app = create_router(state);

        let body = serde_json::json!({
            "model": "emb-model",
            "input": "hello world"
        });

        let request = Request::builder()
            .method("POST")
            .uri("/v1/embeddings")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let json = body_json(response).await;
        let msg = json["error"]["message"].as_str().unwrap();
        assert!(
            msg.contains("does not support embeddings"),
            "msg was: {}",
            msg
        );
    }

    #[tokio::test]
    async fn test_handle_embedding_no_agent_registered() {
        use crate::api::{create_router, AppState};
        use crate::config::NexusConfig;
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model, Registry};
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use std::sync::Arc;
        use tower::Service;

        let registry = Arc::new(Registry::new());
        // Add backend WITHOUT agent
        let backend = crate::registry::Backend::new(
            "no-agent-emb".to_string(),
            "No Agent Emb".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        registry.add_backend(backend).unwrap();
        registry
            .update_status("no-agent-emb", BackendStatus::Healthy, None)
            .unwrap();
        registry
            .update_models(
                "no-agent-emb",
                vec![Model {
                    id: "emb-model".to_string(),
                    name: "emb-model".to_string(),
                    context_length: 4096,
                    supports_vision: false,
                    supports_tools: false,
                    supports_json_mode: false,
                    max_output_tokens: None,
                }],
            )
            .unwrap();

        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(registry, config));
        let mut app = create_router(state);

        let body = serde_json::json!({
            "model": "emb-model",
            "input": "hello"
        });

        let request = Request::builder()
            .method("POST")
            .uri("/v1/embeddings")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

        let json = body_json(response).await;
        let msg = json["error"]["message"].as_str().unwrap();
        assert!(msg.contains("No agent registered"), "msg was: {}", msg);
    }
}
