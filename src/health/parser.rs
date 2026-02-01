//! Response parsing for different backend types.

use super::error::HealthCheckError;
use crate::registry::Model;
use serde::Deserialize;

/// Ollama /api/tags response format
#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
}

/// Parse Ollama /api/tags response into Model structs
pub fn parse_ollama_response(body: &str) -> Result<Vec<Model>, HealthCheckError> {
    let response: OllamaTagsResponse =
        serde_json::from_str(body).map_err(|e| HealthCheckError::ParseError(e.to_string()))?;

    Ok(response
        .models
        .into_iter()
        .map(|m| {
            let name_lower = m.name.to_lowercase();
            let supports_vision = name_lower.contains("llava") || name_lower.contains("vision");
            let supports_tools = name_lower.contains("mistral");

            Model {
                id: m.name.clone(),
                name: m.name,
                context_length: 4096, // Ollama doesn't expose this, use default
                supports_vision,
                supports_tools,
                supports_json_mode: false,
                max_output_tokens: None,
            }
        })
        .collect())
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

/// Parse OpenAI /v1/models response into Model structs
/// Used by vLLM, Exo, OpenAI, and Generic backends
pub fn parse_openai_response(body: &str) -> Result<Vec<Model>, HealthCheckError> {
    let response: OpenAIModelsResponse =
        serde_json::from_str(body).map_err(|e| HealthCheckError::ParseError(e.to_string()))?;

    Ok(response
        .data
        .into_iter()
        .map(|m| {
            Model {
                id: m.id.clone(),
                name: m.id,
                context_length: 4096, // No standard way to detect, use default
                supports_vision: false,
                supports_tools: false,
                supports_json_mode: false,
                max_output_tokens: None,
            }
        })
        .collect())
}

/// LlamaCpp /health response format
#[derive(Deserialize)]
struct LlamaCppHealthResponse {
    status: String,
}

/// Parse LlamaCpp /health response
/// Returns true if healthy, false otherwise
pub fn parse_llamacpp_response(body: &str) -> Result<bool, HealthCheckError> {
    let response: LlamaCppHealthResponse =
        serde_json::from_str(body).map_err(|e| HealthCheckError::ParseError(e.to_string()))?;

    Ok(response.status == "ok")
}
