//! Models listing endpoint handler.

use crate::api::AppState;
use crate::registry::BackendStatus;
use axum::{extract::State, Json};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

/// Models list response in OpenAI format.
#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    pub object: String,
    pub data: Vec<ModelObject>,
}

/// Individual model object.
#[derive(Debug, Serialize)]
pub struct ModelObject {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub owned_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ModelCapabilities>,
}

/// Model capabilities metadata.
#[derive(Debug, Serialize)]
pub struct ModelCapabilities {
    pub vision: bool,
    pub tools: bool,
    pub json_mode: bool,
}

/// GET /v1/models - List all available models from healthy backends.
pub async fn handle(State(state): State<Arc<AppState>>) -> Json<ModelsResponse> {
    let backends = state.registry.get_all_backends();
    let healthy_backends: Vec<_> = backends
        .into_iter()
        .filter(|b| b.status == BackendStatus::Healthy)
        .collect();

    let mut models_map: HashMap<String, ModelObject> = HashMap::new();

    for backend in healthy_backends {
        for model in &backend.models {
            models_map
                .entry(model.id.clone())
                .or_insert_with(|| ModelObject {
                    id: model.id.clone(),
                    object: "model".to_string(),
                    created: chrono::Utc::now().timestamp(),
                    owned_by: "nexus".to_string(),
                    context_length: Some(model.context_length),
                    capabilities: Some(ModelCapabilities {
                        vision: model.supports_vision,
                        tools: model.supports_tools,
                        json_mode: model.supports_json_mode,
                    }),
                });
        }
    }

    let mut data: Vec<_> = models_map.into_values().collect();
    data.sort_by(|a, b| a.id.cmp(&b.id));

    Json(ModelsResponse {
        object: "list".to_string(),
        data,
    })
}
