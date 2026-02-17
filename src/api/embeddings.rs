//! Embeddings endpoint handler (F17: Embeddings API).

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

    // Delegate to agent.embeddings()
    let vectors = agent
        .embeddings(input_texts.clone())
        .await
        .map_err(ApiError::from_agent_error)?;

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

    Ok(Json(response).into_response())
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
}
