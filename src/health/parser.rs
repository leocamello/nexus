//! Response parsing for different backend types.

use super::error::HealthCheckError;
use crate::registry::Model;
use serde::Deserialize;
use std::time::Duration;

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
#[derive(Deserialize)]
struct OllamaShowResponse {
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    model_info: serde_json::Value,
}

/// Parse Ollama /api/tags response into Model structs with basic defaults.
/// Capabilities are populated later via `enrich_ollama_models`.
pub fn parse_ollama_response(body: &str) -> Result<Vec<Model>, HealthCheckError> {
    let response: OllamaTagsResponse =
        serde_json::from_str(body).map_err(|e| HealthCheckError::ParseError(e.to_string()))?;

    Ok(response
        .models
        .into_iter()
        .map(|m| Model {
            id: m.name.clone(),
            name: m.name,
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        })
        .collect())
}

/// Enrich Ollama models with real capabilities from /api/show endpoint.
///
/// Makes one POST /api/show per model to fetch capabilities array and
/// context_length from model_info. Falls back to name-based heuristics
/// if the call fails.
pub async fn enrich_ollama_models(
    models: &mut [Model],
    base_url: &str,
    client: &reqwest::Client,
    timeout: Duration,
) {
    for model in models.iter_mut() {
        let url = format!("{}/api/show", base_url);
        let body = serde_json::json!({"name": model.id});

        match client.post(&url).json(&body).timeout(timeout).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(text) = resp.text().await {
                    if let Ok(show) = serde_json::from_str::<OllamaShowResponse>(&text) {
                        model.supports_vision = show.capabilities.iter().any(|c| c == "vision");
                        model.supports_tools = show.capabilities.iter().any(|c| c == "tools");

                        // Extract context_length from model_info
                        // Keys vary by architecture: llama.context_length, etc.
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
        apply_name_heuristics(model);
    }
}

/// Apply name-based heuristics for capability detection.
/// Used as fallback when backend APIs don't expose structured capability data.
pub fn apply_name_heuristics(model: &mut Model) {
    let name = model.id.to_lowercase();

    // Vision-capable model families
    model.supports_vision = model.supports_vision
        || name.contains("llava")
        || name.contains("vision")
        || name.contains("llama4")
        || (name.contains("gemma")
            && (name.contains("-4b") || name.contains("-12b") || name.contains("-27b")))
        || name.contains("pixtral")
        || name.contains("moondream")
        || name.contains("bakllava")
        || name.contains("minicpm-v");

    // Tool-use-capable model families
    model.supports_tools = model.supports_tools
        || name.contains("mistral")
        || name.contains("llama3.1")
        || name.contains("llama3.2")
        || name.contains("llama3.3")
        || name.contains("llama4")
        || name.contains("qwen2.5")
        || name.contains("qwen3")
        || name.contains("command-r")
        || name.contains("firefunction");
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

/// Parse OpenAI /v1/models response into Model structs.
/// Used by vLLM, Exo, OpenAI, LM Studio, and Generic backends.
/// Applies name-based heuristics for capability detection.
pub fn parse_openai_response(body: &str) -> Result<Vec<Model>, HealthCheckError> {
    let response: OpenAIModelsResponse =
        serde_json::from_str(body).map_err(|e| HealthCheckError::ParseError(e.to_string()))?;

    Ok(response
        .data
        .into_iter()
        .map(|m| {
            let mut model = Model {
                id: m.id.clone(),
                name: m.id,
                context_length: 4096,
                supports_vision: false,
                supports_tools: false,
                supports_json_mode: false,
                max_output_tokens: None,
            };
            apply_name_heuristics(&mut model);
            model
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
