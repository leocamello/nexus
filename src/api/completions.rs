//! Chat completions endpoint handler.

use crate::api::{
    headers::{NexusTransparentHeaders, RouteReason},
    ApiError, AppState, ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse,
    ChunkChoice, ChunkDelta,
};
use crate::logging::generate_request_id;
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
use std::sync::Arc;
use tracing::{info, instrument, warn, Span};

/// Header name for fallback model notification (lowercase for HTTP/2 compatibility)
pub const FALLBACK_HEADER: &str = "x-nexus-fallback-model";

/// POST /v1/chat/completions - Handle chat completion requests.
#[instrument(
    skip(state, headers, request),
    fields(
        request_id = tracing::field::Empty,
        model = %request.model,
        actual_model = tracing::field::Empty,
        backend = tracing::field::Empty,
        backend_type = tracing::field::Empty,
        status = tracing::field::Empty,
        status_code = tracing::field::Empty,
        error_message = tracing::field::Empty,
        latency_ms = tracing::field::Empty,
        tokens_prompt = tracing::field::Empty,
        tokens_completion = tracing::field::Empty,
        tokens_total = tracing::field::Empty,
        stream = %request.stream,
        route_reason = tracing::field::Empty,
        retry_count = 0u32,
        fallback_chain = tracing::field::Empty,
    )
)]
pub async fn handle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Response, ApiError> {
    // Generate request ID for correlation
    let request_id = generate_request_id();
    Span::current().record("request_id", request_id.as_str());

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
    let routing_result_res = state.router.select_backend(&requirements);

    // Handle routing errors with actionable context for 503 (T059-T062)
    let routing_result = match routing_result_res {
        Ok(result) => result,
        Err(e) => {
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

            // Record error in tracing span
            let latency = start_time.elapsed().as_millis() as u64;
            Span::current().record("backend", "none");
            Span::current().record("latency_ms", latency);
            Span::current().record("status", "error");
            Span::current().record("error_message", error_message.as_str());
            let status_code = match &e {
                crate::routing::RoutingError::ModelNotFound { .. } => 404u16,
                crate::routing::RoutingError::FallbackChainExhausted { .. } => 404u16,
                crate::routing::RoutingError::NoHealthyBackend { .. } => 503u16,
                crate::routing::RoutingError::CapabilityMismatch { .. } => 400u16,
            };
            Span::current().record("status_code", status_code);

            // T059-T062: For NoHealthyBackend, return ServiceUnavailableError with context
            match e {
                crate::routing::RoutingError::ModelNotFound { model } => {
                    return Err(ApiError::model_not_found(&model, &available));
                }
                crate::routing::RoutingError::FallbackChainExhausted { chain } => {
                    return Err(ApiError::model_not_found(&chain[0], &available));
                }
                crate::routing::RoutingError::NoHealthyBackend { model } => {
                    use crate::api::error::{ActionableErrorContext, ServiceUnavailableError};

                    // T061: Get list of healthy backends
                    let available_backends = available_backend_names(&state);

                    // T060: Populate required_tier (TODO: need model tier lookup)
                    // For now, we don't have tier info, so set to None
                    let required_tier = None;

                    // T062: eta_seconds initially null
                    let eta_seconds = None;

                    // Build actionable context
                    let context = ActionableErrorContext {
                        required_tier,
                        available_backends,
                        eta_seconds,
                        privacy_zone_required: None,
                    };

                    // T065: Structured logging for routing failure
                    warn!(
                        model = %model,
                        available_backends = ?context.available_backends,
                        "Routing failure: no healthy backend"
                    );

                    let error = ServiceUnavailableError::new(
                        format!("No healthy backend available for model '{}'", model),
                        context,
                    );

                    // T063: Return 503 with actionable context
                    return Ok(error.into_response());
                }
                crate::routing::RoutingError::CapabilityMismatch { model, missing } => {
                    return Err(ApiError::bad_request(&format!(
                        "Model '{}' lacks required capabilities: {:?}",
                        model, missing
                    )));
                }
            }
        }
    };

    let backend = &routing_result.backend;
    let fallback_used = routing_result.fallback_used;
    let actual_model = routing_result.actual_model.clone();

    // Replace alias with resolved model name before forwarding to backend
    let mut request = request;
    request.model = actual_model.clone();

    // Record routing fields in span
    Span::current().record("backend", backend.id.as_str());
    Span::current().record(
        "backend_type",
        format!("{:?}", backend.backend_type).as_str(),
    );
    Span::current().record("route_reason", routing_result.route_reason.as_str());
    if fallback_used {
        Span::current().record("actual_model", actual_model.as_str());
    }

    // Try backend with retry logic
    let max_retries = state.config.routing.max_retries as usize;
    let mut last_error = None;
    let mut fallback_chain_vec: Vec<String> = vec![];

    for attempt in 0..=max_retries {
        // Update retry_count in span
        Span::current().record("retry_count", attempt as u32);

        if attempt > 0 {
            warn!(backend_id = %backend.id, attempt, "Retrying backend after failure");
        } else {
            info!(backend_id = %backend.id, attempt, "Trying backend");
        }

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
                    // Record in metrics
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

                    // Record in tracing span
                    Span::current().record("tokens_prompt", usage.prompt_tokens);
                    Span::current().record("tokens_completion", usage.completion_tokens);
                    Span::current().record("tokens_total", usage.total_tokens);
                }

                // Record completion fields in span
                let latency = start_time.elapsed().as_millis() as u64;
                Span::current().record("latency_ms", latency);
                Span::current().record("status", "success");
                Span::current().record("status_code", 200u16);

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

                // T035/T047: Inject X-Nexus-* transparent headers (F12)
                // Derive RouteReason from routing_result.route_reason string
                let route_reason = determine_route_reason(
                    &routing_result.route_reason,
                    routing_result.fallback_used,
                    attempt as u32,
                );

                // Get privacy zone from agent profile, or use backend type default
                let privacy_zone = state
                    .registry
                    .get_agent(&backend.id)
                    .map(|agent| agent.profile().privacy_zone)
                    .unwrap_or_else(|| backend.backend_type.default_privacy_zone());

                let header_inject_start = std::time::Instant::now();
                let nexus_headers = NexusTransparentHeaders::new(
                    backend.id.clone(),
                    backend.backend_type,
                    route_reason,
                    privacy_zone,
                    routing_result.cost_estimated,
                );
                nexus_headers.inject_into_response(&mut resp);

                let header_inject_time_us = header_inject_start.elapsed().as_micros();
                tracing::debug!(header_inject_time_us, "header injection completed");

                // Also inject fallback header if applicable
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

                // Track this backend in fallback chain
                if !fallback_chain_vec.contains(&backend.id) {
                    fallback_chain_vec.push(backend.id.clone());
                }

                // Record error message in span
                Span::current().record("error_message", e.error.message.as_str());

                // Update fallback chain in span
                let fallback_chain_str = fallback_chain_vec.join(",");
                Span::current().record("fallback_chain", fallback_chain_str.as_str());

                if attempt > 0 {
                    warn!(
                        backend_id = %backend.id,
                        error = %e.error.message,
                        attempt,
                        "Backend request failed (retry)"
                    );
                } else {
                    warn!(
                        backend_id = %backend.id,
                        error = %e.error.message,
                        "Backend request failed"
                    );
                }

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
                    // This was the last retry, record the error at ERROR level
                    tracing::error!(
                        backend_id = %backend.id,
                        error = %e.error.message,
                        attempts = max_retries + 1,
                        "All retry attempts exhausted"
                    );

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
    let latency = start_time.elapsed().as_millis() as u64;
    Span::current().record("latency_ms", latency);
    Span::current().record("status", "error");
    Span::current().record("status_code", 502u16);
    if let Some(ref err) = last_error {
        Span::current().record("error_message", err.error.message.as_str());
    }

    Err(last_error.unwrap_or_else(|| ApiError::bad_gateway("All backends failed")))
}

/// Proxy request to backend.
async fn proxy_request(
    state: &Arc<AppState>,
    backend: &crate::registry::Backend,
    headers: &HeaderMap,
    request: &ChatCompletionRequest,
) -> Result<ChatCompletionResponse, ApiError> {
    // Try to get agent from registry (T036)
    if let Some(agent) = state.registry.get_agent(&backend.id) {
        // Use agent-based chat completion (T036)
        let response = agent
            .chat_completion(request.clone(), Some(headers))
            .await
            .map_err(ApiError::from_agent_error)?;

        Ok(response)
    } else {
        // Legacy fallback: direct HTTP (backwards compatibility)
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

        // T050: For non-success responses, preserve the backend error response unchanged
        // This maintains OpenAI compatibility and follows Constitution Principle III
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();

            // Try to preserve the backend's error response as-is
            match ApiError::from_backend_json(status.as_u16(), body.clone()) {
                Ok(preserved_error) => return Err(preserved_error),
                Err(wrapped_error) => return Err(wrapped_error),
            }
        }

        response
            .json::<ChatCompletionResponse>()
            .await
            .map_err(|e| ApiError::bad_gateway(&format!("Invalid backend response: {}", e)))
    }
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

/// Get list of available backend names for error context (T061).
fn available_backend_names(state: &Arc<AppState>) -> Vec<String> {
    state
        .registry
        .get_all_backends()
        .into_iter()
        .filter(|b| matches!(b.status, crate::registry::BackendStatus::Healthy))
        .map(|b| b.id.clone())
        .collect()
}

/// Handle streaming chat completion requests.
async fn handle_streaming(
    state: Arc<AppState>,
    headers: HeaderMap,
    request: ChatCompletionRequest,
) -> Result<Response, ApiError> {
    // Use router to select backend
    let requirements = RequestRequirements::from_request(&request);
    let routing_result = state.router.select_backend(&requirements);

    // Handle routing errors (same as non-streaming for consistency)
    let routing_result = match routing_result {
        Ok(result) => result,
        Err(e) => {
            let available = available_models(&state);
            match e {
                crate::routing::RoutingError::ModelNotFound { model } => {
                    return Err(ApiError::model_not_found(&model, &available));
                }
                crate::routing::RoutingError::FallbackChainExhausted { chain } => {
                    return Err(ApiError::model_not_found(&chain[0], &available));
                }
                crate::routing::RoutingError::NoHealthyBackend { model } => {
                    use crate::api::error::{ActionableErrorContext, ServiceUnavailableError};

                    let available_backends = available_backend_names(&state);
                    let context = ActionableErrorContext {
                        required_tier: None,
                        available_backends,
                        eta_seconds: None,
                        privacy_zone_required: None,
                    };

                    warn!(
                        model = %model,
                        available_backends = ?context.available_backends,
                        "Streaming routing failure: no healthy backend"
                    );

                    let error = ServiceUnavailableError::new(
                        format!("No healthy backend available for model '{}'", model),
                        context,
                    );

                    return Ok(error.into_response());
                }
                crate::routing::RoutingError::CapabilityMismatch { model, missing } => {
                    return Err(ApiError::bad_request(&format!(
                        "Model '{}' lacks required capabilities: {:?}",
                        model, missing
                    )));
                }
            }
        }
    };

    let backend = routing_result.backend;
    let fallback_used = routing_result.fallback_used;
    let actual_model = routing_result.actual_model.clone();
    let backend_id = backend.id.clone();

    // Replace alias with resolved model name before forwarding to backend
    let mut request = request;
    request.model = actual_model.clone();

    // Increment pending requests
    let _ = state.registry.increment_pending(&backend_id);

    info!(backend_id = %backend_id, "Starting streaming request");

    // Create SSE stream - pass cloned backend data
    let stream = create_sse_stream(state.clone(), Arc::clone(&backend), headers, request);

    // Create SSE response and add headers
    let mut resp = Sse::new(stream).into_response();

    // T036/T047: Inject X-Nexus-* transparent headers for streaming (F12)
    // Headers must be injected BEFORE first SSE chunk
    let route_reason = determine_route_reason(
        &routing_result.route_reason,
        fallback_used,
        0, // No retries for streaming
    );

    // Get privacy zone from agent profile, or use backend type default
    let privacy_zone = state
        .registry
        .get_agent(&backend.id)
        .map(|agent| agent.profile().privacy_zone)
        .unwrap_or_else(|| backend.backend_type.default_privacy_zone());

    let header_inject_start = std::time::Instant::now();
    let nexus_headers = NexusTransparentHeaders::new(
        backend.id.clone(),
        backend.backend_type,
        route_reason,
        privacy_zone,
        routing_result.cost_estimated,
    );
    nexus_headers.inject_into_response(&mut resp);

    let header_inject_time_us = header_inject_start.elapsed().as_micros();
    tracing::debug!(header_inject_time_us, "header injection completed");

    // Also add fallback header if needed
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
        let backend_id = backend.id.clone();

        // Try to get agent from registry (T037)
        if let Some(agent) = state.registry.get_agent(&backend_id) {
            // Use agent-based streaming (T037)
            match agent.chat_completion_stream(request.clone(), Some(&headers)).await {
                Ok(mut stream) => {
                    // Stream chunks from agent
                    use futures::StreamExt;
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(chunk) => {
                                // Check if this is [DONE]
                                if chunk.data == "[DONE]" {
                                    yield Ok(Event::default().data("[DONE]"));
                                    break;
                                } else {
                                    // Forward the chunk data (already JSON)
                                    yield Ok(Event::default().data(chunk.data));
                                }
                            }
                            Err(e) => {
                                warn!(backend_id = %backend_id, error = %e, "Stream error from agent");
                                let error_chunk = create_error_chunk(&format!("Stream error: {}", e));
                                yield Ok(Event::default().data(serde_json::to_string(&error_chunk).unwrap_or_default()));
                                yield Ok(Event::default().data("[DONE]"));
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(backend_id = %backend_id, error = %e, "Failed to start streaming from agent");
                    let error_chunk = create_error_chunk(&format!("Failed to start streaming: {}", e));
                    yield Ok(Event::default().data(serde_json::to_string(&error_chunk).unwrap_or_default()));
                    yield Ok(Event::default().data("[DONE]"));
                }
            }
        } else {
            // Legacy fallback: direct HTTP streaming
            let url = format!("{}/v1/chat/completions", backend.url);

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
            use futures::StreamExt;
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

    // Increment total completed requests for this backend
    let _ = state.registry.increment_total_requests(backend_id);

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

/// T047: Determine RouteReason enum from internal routing decision string.
///
/// Maps internal routing logic strings to Nexus Transparent Protocol RouteReason enum.
/// This function implements the routing decision transparency required by F12.
fn determine_route_reason(
    internal_reason: &str,
    fallback_used: bool,
    retry_count: u32,
) -> RouteReason {
    // If fallback was used or retries occurred, it's a failover situation
    if fallback_used || retry_count > 0 {
        return RouteReason::Failover;
    }

    // Parse internal routing decision string
    // Examples: "highest_score:backend-1:0.95", "only_healthy_backend", "priority:backend-1:50"

    // Check for privacy-related routing
    if internal_reason.contains("privacy") || internal_reason.contains("restricted") {
        return RouteReason::PrivacyRequirement;
    }

    // Check for capacity-related routing
    if internal_reason.contains("capacity")
        || internal_reason.contains("overflow")
        || internal_reason.contains("saturated")
        || internal_reason.contains("overloaded")
    {
        return RouteReason::CapacityOverflow;
    }

    // Check for failover indicators
    if internal_reason.contains("failover")
        || internal_reason.contains("backup")
        || internal_reason.contains("fallback")
    {
        return RouteReason::Failover;
    }

    // Default: capability match (standard routing)
    // This covers cases like:
    // - "highest_score:..." (model matched, backend selected by score)
    // - "only_healthy_backend" (model matched, single option)
    // - "priority:..." (model matched, backend selected by priority)
    // - "round_robin:..." (model matched, round-robin selection)
    // - "random:..." (model matched, random selection)
    RouteReason::CapabilityMatch
}
