//! Chat completions endpoint handler.

use crate::api::{ApiError, AppState, ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, ChunkChoice, ChunkDelta};
use crate::registry::{Backend, BackendStatus};
use axum::{
    extract::State,
    http::HeaderMap,
    response::{sse::{Event, Sse}, IntoResponse, Response},
    Json,
};
use futures::StreamExt;
use std::sync::Arc;
use tracing::{info, warn};

/// POST /v1/chat/completions - Handle chat completion requests.
pub async fn handle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Response, ApiError> {
    info!(model = %request.model, stream = request.stream, "Chat completion request");

    // For streaming requests, use streaming handler
    if request.stream {
        return handle_streaming(state, headers, request).await;
    }

    // Find backends that support this model
    let backends = state.registry.get_backends_for_model(&request.model);
    if backends.is_empty() {
        let available = available_models(&state);
        return Err(ApiError::model_not_found(&request.model, &available));
    }

    // Filter to healthy backends only
    let healthy: Vec<_> = backends
        .into_iter()
        .filter(|b| b.status == BackendStatus::Healthy)
        .collect();

    if healthy.is_empty() {
        return Err(ApiError::service_unavailable(
            "No healthy backends available for this model",
        ));
    }

    // Try backends with retry logic
    let max_retries = state.config.routing.max_retries as usize;
    let mut last_error = None;

    for (attempt, backend) in healthy.iter().take(max_retries + 1).enumerate() {
        info!(backend_id = %backend.id, attempt, "Trying backend");

        // Increment pending requests
        let _ = state.registry.increment_pending(&backend.id);

        match proxy_request(&state, backend, &headers, &request).await {
            Ok(response) => {
                let _ = state.registry.decrement_pending(&backend.id);
                info!(backend_id = %backend.id, "Request succeeded");
                return Ok(Json(response).into_response());
            }
            Err(e) => {
                let _ = state.registry.decrement_pending(&backend.id);
                warn!(backend_id = %backend.id, error = %e.error.message, "Backend request failed");
                last_error = Some(e);
            }
        }
    }

    // All retries failed
    Err(last_error.unwrap_or_else(|| ApiError::bad_gateway("All backends failed")))
}

/// Proxy request to backend.
async fn proxy_request(
    state: &Arc<AppState>,
    backend: &crate::registry::Backend,
    headers: &HeaderMap,
    request: &ChatCompletionRequest,
) -> Result<ChatCompletionResponse, ApiError> {
    let url = format!("{}/v1/chat/completions", backend.url);

    let mut req = state.http_client.post(&url).json(request);

    // Forward Authorization header if present
    if let Some(auth) = headers.get("authorization") {
        req = req.header("Authorization", auth);
    }

    let response = req
        .send()
        .await
        .map_err(|e| ApiError::bad_gateway(&format!("Backend connection failed: {}", e)))?;

    let status = response.status();

    if status == axum::http::StatusCode::GATEWAY_TIMEOUT {
        return Err(ApiError::gateway_timeout());
    }

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(ApiError::bad_gateway(&format!(
            "Backend returned {}: {}",
            status, body
        )));
    }

    response
        .json::<ChatCompletionResponse>()
        .await
        .map_err(|e| ApiError::bad_gateway(&format!("Invalid backend response: {}", e)))
}

/// Get list of available models for error messages.
fn available_models(state: &Arc<AppState>) -> Vec<String> {
    let backends = state.registry.get_all_backends();
    let mut models = std::collections::HashSet::new();

    for backend in backends {
        for model in &backend.models {
            models.insert(model.id.clone());
        }
    }

    models.into_iter().collect()
}

/// Handle streaming chat completion requests.
async fn handle_streaming(
    state: Arc<AppState>,
    headers: HeaderMap,
    request: ChatCompletionRequest,
) -> Result<Response, ApiError> {
    // Find backends for model
    let backends = state.registry.get_backends_for_model(&request.model);
    if backends.is_empty() {
        let available = available_models(&state);
        return Err(ApiError::model_not_found(&request.model, &available));
    }

    // Filter to healthy backends only
    let healthy: Vec<_> = backends
        .into_iter()
        .filter(|b| b.status == BackendStatus::Healthy)
        .collect();

    if healthy.is_empty() {
        return Err(ApiError::service_unavailable(
            "No healthy backends available for this model",
        ));
    }

    // For streaming, use first healthy backend (retry not possible mid-stream)
    let backend = healthy.into_iter().next().unwrap();
    let backend_id = backend.id.clone();

    // Increment pending requests
    let _ = state.registry.increment_pending(&backend_id);

    info!(backend_id = %backend_id, "Starting streaming request");

    // Create SSE stream
    let stream = create_sse_stream(state, backend, headers, request);

    Ok(Sse::new(stream).into_response())
}

/// Create an SSE stream that proxies chunks from the backend.
fn create_sse_stream(
    state: Arc<AppState>,
    backend: Backend,
    headers: HeaderMap,
    request: ChatCompletionRequest,
) -> impl futures::Stream<Item = Result<Event, std::convert::Infallible>> {
    async_stream::stream! {
        let url = format!("{}/v1/chat/completions", backend.url);
        let backend_id = backend.id.clone();

        let mut req = state.http_client.post(&url).json(&request);

        // Forward Authorization header if present
        if let Some(auth) = headers.get("authorization") {
            req = req.header("Authorization", auth);
        }

        // Send request to backend
        let response = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                warn!(backend_id = %backend_id, error = %e, "Backend connection failed");
                let _ = state.registry.decrement_pending(&backend_id);
                // Yield error as SSE event before closing
                let error_chunk = create_error_chunk(&format!("Backend connection failed: {}", e));
                yield Ok(Event::default().data(serde_json::to_string(&error_chunk).unwrap_or_default()));
                yield Ok(Event::default().data("[DONE]"));
                return;
            }
        };

        // Check for non-success status
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!(backend_id = %backend_id, status = %status, "Backend returned error");
            let _ = state.registry.decrement_pending(&backend_id);
            let error_chunk = create_error_chunk(&format!("Backend returned {}: {}", status, body));
            yield Ok(Event::default().data(serde_json::to_string(&error_chunk).unwrap_or_default()));
            yield Ok(Event::default().data("[DONE]"));
            return;
        }

        // Stream response body
        let mut byte_stream = response.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk_result) = byte_stream.next().await {
            match chunk_result {
                Ok(bytes) => {
                    buffer.push_str(&String::from_utf8_lossy(&bytes));

                    // Process complete lines
                    while let Some(pos) = buffer.find('\n') {
                        let line = buffer[..pos].to_string();
                        buffer = buffer[pos + 1..].to_string();

                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }

                        // Parse SSE data lines
                        if let Some(data) = line.strip_prefix("data: ") {
                            if data == "[DONE]" {
                                yield Ok(Event::default().data("[DONE]"));
                            } else {
                                // Forward the data as-is (already JSON)
                                yield Ok(Event::default().data(data));
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(backend_id = %backend_id, error = %e, "Stream read error");
                    break;
                }
            }
        }

        // Decrement pending requests
        let _ = state.registry.decrement_pending(&backend_id);
        info!(backend_id = %backend_id, "Streaming request completed");
    }
}

/// Create an error chunk in OpenAI streaming format.
fn create_error_chunk(message: &str) -> ChatCompletionChunk {
    ChatCompletionChunk {
        id: format!("chatcmpl-error-{}", uuid::Uuid::new_v4()),
        object: "chat.completion.chunk".to_string(),
        created: chrono::Utc::now().timestamp(),
        model: "error".to_string(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta {
                role: None,
                content: Some(format!("[Error: {}]", message)),
            },
            finish_reason: Some("error".to_string()),
        }],
    }
}
