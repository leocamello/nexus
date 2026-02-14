//! Chat completions endpoint handler.

use crate::api::{
    ApiError, AppState, ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse,
    ChunkChoice, ChunkDelta,
};
use crate::registry::Backend;
use crate::routing::RequestRequirements;
use axum::{
    extract::State,
    http::{HeaderMap, HeaderName, HeaderValue},
    response::{
        sse::{Event, Sse},
        IntoResponse, Response,
    },
    Json,
};
use futures::StreamExt;
use std::sync::Arc;
use tracing::{info, warn};

/// Header name for fallback model notification (lowercase for HTTP/2 compatibility)
pub const FALLBACK_HEADER: &str = "x-nexus-fallback-model";

/// POST /v1/chat/completions - Handle chat completion requests.
pub async fn handle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Response, ApiError> {
    // Start timer for request duration tracking
    let start_time = std::time::Instant::now();
    let requested_model = request.model.clone();

    info!(model = %request.model, stream = request.stream, "Chat completion request");

    // For streaming requests, use streaming handler
    if request.stream {
        return handle_streaming(state, headers, request).await;
    }

    // Use router to select backend
    let requirements = RequestRequirements::from_request(&request);
    let routing_result = state.router.select_backend(&requirements).map_err(|e| {
        let available = available_models(&state);

        // Record error metrics
        let error_type = match &e {
            crate::routing::RoutingError::ModelNotFound { .. } => "model_not_found",
            crate::routing::RoutingError::FallbackChainExhausted { .. } => "fallback_exhausted",
            crate::routing::RoutingError::NoHealthyBackend { .. } => "no_healthy_backend",
            crate::routing::RoutingError::CapabilityMismatch { .. } => "capability_mismatch",
        };

        let sanitized_model = state.metrics_collector.sanitize_label(&requested_model);
        metrics::counter!("nexus_errors_total",
            "error_type" => error_type,
            "model" => sanitized_model.clone()
        )
        .increment(1);

        // Record routing error in request history
        let error_message = match &e {
            crate::routing::RoutingError::ModelNotFound { model } => {
                format!("Model '{}' not found", model)
            }
            crate::routing::RoutingError::FallbackChainExhausted { chain } => {
                format!("Fallback chain exhausted: {}", chain[0])
            }
            crate::routing::RoutingError::NoHealthyBackend { model } => {
                format!("No healthy backend available for model '{}'", model)
            }
            crate::routing::RoutingError::CapabilityMismatch { model, missing } => {
                format!(
                    "Model '{}' lacks required capabilities: {:?}",
                    model, missing
                )
            }
        };

        record_request_completion(
            &state,
            &requested_model,
            "none",
            start_time.elapsed().as_millis() as u64,
            crate::dashboard::types::RequestStatus::Error,
            Some(error_message.clone()),
        );

        match e {
            crate::routing::RoutingError::ModelNotFound { model } => {
                ApiError::model_not_found(&model, &available)
            }
            crate::routing::RoutingError::FallbackChainExhausted { chain } => {
                ApiError::model_not_found(&chain[0], &available)
            }
            crate::routing::RoutingError::NoHealthyBackend { model } => {
                ApiError::service_unavailable(&format!(
                    "No healthy backend available for model '{}'",
                    model
                ))
            }
            crate::routing::RoutingError::CapabilityMismatch { model, missing } => {
                ApiError::bad_request(&format!(
                    "Model '{}' lacks required capabilities: {:?}",
                    model, missing
                ))
            }
        }
    })?;

    let backend = &routing_result.backend;
    let fallback_used = routing_result.fallback_used;
    let actual_model = routing_result.actual_model.clone();

    // Try backend with retry logic
    let max_retries = state.config.routing.max_retries as usize;
    let mut last_error = None;

    for attempt in 0..=max_retries {
        info!(backend_id = %backend.id, attempt, "Trying backend");

        // Increment pending requests
        let _ = state.registry.increment_pending(&backend.id);

        match proxy_request(&state, backend, &headers, &request).await {
            Ok(response) => {
                let _ = state.registry.decrement_pending(&backend.id);
                info!(backend_id = %backend.id, "Request succeeded");

                // Record success metrics
                let duration = start_time.elapsed().as_secs_f64();
                let sanitized_model = state.metrics_collector.sanitize_label(&actual_model);
                let sanitized_backend = state.metrics_collector.sanitize_label(&backend.id);

                // Increment request counter
                metrics::counter!("nexus_requests_total",
                    "model" => sanitized_model.clone(),
                    "backend" => sanitized_backend.clone(),
                    "status" => "200"
                )
                .increment(1);

                // Record request duration
                metrics::histogram!("nexus_request_duration_seconds",
                    "model" => sanitized_model.clone(),
                    "backend" => sanitized_backend.clone()
                )
                .record(duration);

                // Record fallback usage if applicable
                if fallback_used {
                    let sanitized_requested =
                        state.metrics_collector.sanitize_label(&requested_model);
                    metrics::counter!("nexus_fallbacks_total",
                        "from_model" => sanitized_requested,
                        "to_model" => sanitized_model.clone()
                    )
                    .increment(1);
                }

                // Record token usage if available in response
                if let Some(ref usage) = response.usage {
                    metrics::histogram!("nexus_tokens_total",
                        "model" => sanitized_model.clone(),
                        "backend" => sanitized_backend.clone(),
                        "type" => "prompt"
                    )
                    .record(usage.prompt_tokens as f64);

                    metrics::histogram!("nexus_tokens_total",
                        "model" => sanitized_model.clone(),
                        "backend" => sanitized_backend.clone(),
                        "type" => "completion"
                    )
                    .record(usage.completion_tokens as f64);
                }

                // Record request in history and broadcast update
                record_request_completion(
                    &state,
                    &actual_model,
                    &backend.id,
                    start_time.elapsed().as_millis() as u64,
                    crate::dashboard::types::RequestStatus::Success,
                    None,
                );

                // Create response with fallback header if applicable
                let mut resp = Json(response).into_response();
                if fallback_used {
                    if let Ok(header_value) = HeaderValue::from_str(&actual_model) {
                        resp.headers_mut()
                            .insert(HeaderName::from_static(FALLBACK_HEADER), header_value);
                    }
                }
                return Ok(resp);
            }
            Err(e) => {
                let _ = state.registry.decrement_pending(&backend.id);
                warn!(backend_id = %backend.id, error = %e.error.message, "Backend request failed");

                // Record backend error
                let sanitized_model = state.metrics_collector.sanitize_label(&requested_model);
                let error_type = if e.error.code.as_deref() == Some("gateway_timeout") {
                    "timeout"
                } else {
                    "backend_error"
                };

                metrics::counter!("nexus_errors_total",
                    "error_type" => error_type,
                    "model" => sanitized_model
                )
                .increment(1);

                // Record error in request history
                if attempt == max_retries {
                    // This was the last retry, record the error
                    record_request_completion(
                        &state,
                        &actual_model,
                        &backend.id,
                        start_time.elapsed().as_millis() as u64,
                        crate::dashboard::types::RequestStatus::Error,
                        Some(e.error.message.clone()),
                    );
                }

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
    // Use router to select backend
    let requirements = RequestRequirements::from_request(&request);
    let routing_result = state.router.select_backend(&requirements).map_err(|e| {
        let available = available_models(&state);
        match e {
            crate::routing::RoutingError::ModelNotFound { model } => {
                ApiError::model_not_found(&model, &available)
            }
            crate::routing::RoutingError::FallbackChainExhausted { chain } => {
                ApiError::model_not_found(&chain[0], &available)
            }
            crate::routing::RoutingError::NoHealthyBackend { model } => {
                ApiError::service_unavailable(&format!(
                    "No healthy backend available for model '{}'",
                    model
                ))
            }
            crate::routing::RoutingError::CapabilityMismatch { model, missing } => {
                ApiError::bad_request(&format!(
                    "Model '{}' lacks required capabilities: {:?}",
                    model, missing
                ))
            }
        }
    })?;

    let backend = routing_result.backend;
    let fallback_used = routing_result.fallback_used;
    let actual_model = routing_result.actual_model.clone();
    let backend_id = backend.id.clone();

    // Increment pending requests
    let _ = state.registry.increment_pending(&backend_id);

    info!(backend_id = %backend_id, "Starting streaming request");

    // Create SSE stream - pass cloned backend data
    let stream = create_sse_stream(state, Arc::clone(&backend), headers, request);

    // Create SSE response and add fallback header if needed
    let mut resp = Sse::new(stream).into_response();
    if fallback_used {
        if let Ok(header_value) = HeaderValue::from_str(&actual_model) {
            resp.headers_mut()
                .insert(HeaderName::from_static(FALLBACK_HEADER), header_value);
        }
    }

    Ok(resp)
}

/// Create an SSE stream that proxies chunks from the backend.
fn create_sse_stream(
    state: Arc<AppState>,
    backend: Arc<Backend>,
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

/// Record a completed request in history and broadcast to dashboard
fn record_request_completion(
    state: &Arc<AppState>,
    model: &str,
    backend_id: &str,
    duration_ms: u64,
    status: crate::dashboard::types::RequestStatus,
    error_message: Option<String>,
) {
    use crate::dashboard::types::HistoryEntry;
    use crate::dashboard::websocket::create_request_complete_update;

    let entry = HistoryEntry {
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        model: model.to_string(),
        backend_id: backend_id.to_string(),
        duration_ms,
        status: status.clone(),
        error_message,
    };

    // Push to request history ring buffer
    state.request_history.push(entry.clone());

    // Broadcast update to WebSocket clients
    let update = create_request_complete_update(entry);
    let _ = state.ws_broadcast.send(update);
}
