//! Models listing endpoint handler.

use crate::api::AppState;
use crate::registry::BackendStatus;
use axum::{extract::State, Json};
use serde::Serialize;
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

    let mut data: Vec<ModelObject> = Vec::new();

    for backend in healthy_backends {
        for model in &backend.models {
            data.push(ModelObject {
                id: model.id.clone(),
                object: "model".to_string(),
                created: chrono::Utc::now().timestamp(),
                owned_by: backend.name.clone(),
                context_length: Some(model.context_length),
                capabilities: Some(ModelCapabilities {
                    vision: model.supports_vision,
                    tools: model.supports_tools,
                    json_mode: model.supports_json_mode,
                }),
            });
        }
    }

    data.sort_by(|a, b| a.id.cmp(&b.id).then(a.owned_by.cmp(&b.owned_by)));

    Json(ModelsResponse {
        object: "list".to_string(),
        data,
    })
}
