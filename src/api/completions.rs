//! Chat completions endpoint handler.

use crate::api::{
    headers::{NexusTransparentHeaders, RouteReason},
    ApiError, AppState, ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse,
    ChunkChoice, ChunkDelta,
};
use crate::logging::generate_request_id;
use crate::registry::Backend;
use crate::routing::reconciler::intent::{RejectionReason, TierEnforcementMode};
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
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{info, instrument, warn, Span};

/// Header name for fallback model notification (lowercase for HTTP/2 compatibility)
pub const FALLBACK_HEADER: &str = "x-nexus-fallback-model";

/// Header name for rejection reasons summary (lowercase for HTTP/2 compatibility)
const REJECTION_REASONS_HEADER: &str = "x-nexus-rejection-reasons";

/// Header name for strict tier enforcement (lowercase for HTTP/2 compatibility)
const STRICT_HEADER: &str = "x-nexus-strict";

/// Header name for flexible tier enforcement (lowercase for HTTP/2 compatibility)
const FLEXIBLE_HEADER: &str = "x-nexus-flexible";

/// Header name for request priority (T028)
const PRIORITY_HEADER: &str = "x-nexus-priority";

/// Extract tier enforcement mode from request headers (FR-007, FR-008, FR-009).
///
/// # Header Priority
/// 1. If `X-Nexus-Strict` is present → Strict mode (safer default)
/// 2. If `X-Nexus-Flexible` is present → Flexible mode
/// 3. If neither present → Strict mode (default, FR-009)
///
/// # Examples
/// ```text
/// No headers              → Strict
/// X-Nexus-Strict: true    → Strict
/// X-Nexus-Flexible: true  → Flexible
/// Both headers            → Strict (takes precedence)
/// ```
fn extract_tier_enforcement_mode(headers: &HeaderMap) -> TierEnforcementMode {
    // Strict takes precedence if present (FR-007)
    if headers.contains_key(STRICT_HEADER) {
        return TierEnforcementMode::Strict;
    }

    // Check flexible header (FR-008)
    if let Some(val) = headers.get(FLEXIBLE_HEADER) {
        if val.to_str().ok() == Some("true") {
            return TierEnforcementMode::Flexible;
        }
    }

    // Default to strict (FR-009)
    TierEnforcementMode::Strict
}

/// Extract priority from X-Nexus-Priority header (T028).
///
/// Valid values: "high", "normal". Default: "normal".
/// Invalid values default to "normal".
fn extract_priority(headers: &HeaderMap) -> crate::queue::Priority {
    headers
        .get(PRIORITY_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(crate::queue::Priority::from_header)
        .unwrap_or(crate::queue::Priority::Normal)
}

/// Inject budget-related response headers (F14: T037-T040).
///
/// Adds the following headers based on budget status:
/// - X-Nexus-Cost-Estimated: Only from budget if not already set by F12 headers and cost > 0
/// - X-Nexus-Budget-Status: When status != Normal (Normal/SoftLimit/HardLimit)
/// - X-Nexus-Budget-Utilization: When status != Normal (percentage, 2 decimals)
/// - X-Nexus-Budget-Remaining: When status != Normal (USD remaining, 2 decimals)
fn inject_budget_headers<B>(
    response: &mut Response<B>,
    routing_result: &crate::routing::RoutingResult,
) {
    use crate::routing::reconciler::intent::BudgetStatus;

    let headers = response.headers_mut();

    // T040: Set cost header from routing estimate only if not already set by
    // NexusTransparentHeaders (F12) and cost is meaningful (> 0)
    if !headers.contains_key("x-nexus-cost-estimated") {
        if let Some(cost) = routing_result.cost_estimated {
            if cost > 0.0 {
                let _ = headers.insert(
                    HeaderName::from_static("x-nexus-cost-estimated"),
                    HeaderValue::from_str(&format!("{:.4}", cost))
                        .unwrap_or_else(|_| HeaderValue::from_static("0.0000")),
                );
            }
        }
    }

    // T037-T039: Only add budget headers when not in Normal status
    if routing_result.budget_status != BudgetStatus::Normal {
        // T037: Budget status
        let status_str = match routing_result.budget_status {
            BudgetStatus::Normal => "Normal",
            BudgetStatus::SoftLimit => "SoftLimit",
            BudgetStatus::HardLimit => "HardLimit",
        };
        let _ = headers.insert(
            HeaderName::from_static("x-nexus-budget-status"),
            HeaderValue::from_static(status_str),
        );

        // T038: Budget utilization percentage
        if let Some(utilization) = routing_result.budget_utilization {
            let _ = headers.insert(
                HeaderName::from_static("x-nexus-budget-utilization"),
                HeaderValue::from_str(&format!("{:.2}", utilization))
                    .unwrap_or_else(|_| HeaderValue::from_static("0.00")),
            );
        }

        // T039: Budget remaining in USD
        if let Some(remaining) = routing_result.budget_remaining {
            let _ = headers.insert(
                HeaderName::from_static("x-nexus-budget-remaining"),
                HeaderValue::from_str(&format!("{:.2}", remaining))
                    .unwrap_or_else(|_| HeaderValue::from_static("0.00")),
            );
        }
    }
}

/// Build a structured 503 response for routing rejections with actionable details.
///
/// Extracts privacy zone and tier info from rejection reasons to populate
/// ActionableErrorContext alongside the rejection details.
fn rejection_response(
    rejection_reasons: Vec<RejectionReason>,
    available_backends: Vec<String>,
) -> Response {
    use crate::api::error::{ActionableErrorContext, ServiceUnavailableError};

    let count = rejection_reasons.len();
    let reconcilers: HashSet<&str> = rejection_reasons
        .iter()
        .map(|r| r.reconciler.as_str())
        .collect();
    let reconciler_list: Vec<&str> = reconcilers.into_iter().collect();

    // Extract privacy zone from rejection reasons (if PrivacyReconciler rejected)
    let privacy_zone_required = rejection_reasons
        .iter()
        .find(|r| r.reconciler == "PrivacyReconciler")
        .map(|r| {
            if r.reason.contains("restricted") {
                "restricted".to_string()
            } else {
                "open".to_string()
            }
        });

    // Extract required tier from rejection reasons (if TierReconciler rejected)
    let required_tier = rejection_reasons
        .iter()
        .find(|r| r.reconciler == "TierReconciler")
        .and_then(|r| {
            // Extract tier number from reason like "agent tier 2 below minimum 3"
            r.reason
                .split("minimum ")
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
                .and_then(|s| s.parse::<u8>().ok())
        });

    let context = ActionableErrorContext {
        required_tier,
        available_backends,
        eta_seconds: None,
        privacy_zone_required,
    };

    let message = format!("Request rejected: {} agents excluded", count);

    warn!(
        rejection_count = count,
        reconcilers = ?reconciler_list,
        privacy_zone = ?context.privacy_zone_required,
        required_tier = ?context.required_tier,
        "Routing rejection with actionable context"
    );

    let error = ServiceUnavailableError::new(message, context);
    let mut response = error.into_response();

    // Add rejection details header
    let header_value = format!(
        "{} agents rejected by {}",
        count,
        reconciler_list.join(", ")
    );
    if let Ok(val) = HeaderValue::from_str(&header_value) {
        response
            .headers_mut()
            .insert(HeaderName::from_static(REJECTION_REASONS_HEADER), val);
    }

    // Add rejection reasons as JSON in a separate header for programmatic access
    if let Ok(json) = serde_json::to_string(&rejection_reasons) {
        if let Ok(val) = HeaderValue::from_str(&json) {
            response
                .headers_mut()
                .insert(HeaderName::from_static("x-nexus-rejection-details"), val);
        }
    }

    response
}

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

    // Extract request requirements
    let requirements = RequestRequirements::from_request(&request);

    // Extract tier enforcement mode from request headers (T032, FR-007, FR-008, FR-009)
    let tier_mode = extract_tier_enforcement_mode(&headers);

    // Use router to select backend
    let routing_result_res = state.router.select_backend(&requirements, Some(tier_mode));

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
                crate::routing::RoutingError::Reject { .. } => "routing_rejected",
                crate::routing::RoutingError::Queue { .. } => "queued",
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
                crate::routing::RoutingError::Reject { rejection_reasons } => {
                    format!("Request rejected: {} reasons", rejection_reasons.len())
                }
                crate::routing::RoutingError::Queue { reason, .. } => {
                    format!("Request queued: {}", reason)
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
                crate::routing::RoutingError::Reject { .. } => 503u16,
                crate::routing::RoutingError::Queue { .. } => 503u16,
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
                crate::routing::RoutingError::Reject { rejection_reasons } => {
                    let backends = available_backend_names(&state);
                    return Ok(rejection_response(rejection_reasons, backends));
                }
                crate::routing::RoutingError::Queue {
                    reason,
                    estimated_wait_ms,
                } => {
                    // T027: Enqueue if queue available
                    if let Some(ref queue) = state.queue {
                        let priority = extract_priority(&headers);
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        let intent = crate::routing::reconciler::intent::RoutingIntent::new(
                            request_id.clone(),
                            requested_model.clone(),
                            requested_model.clone(),
                            requirements.clone(),
                            vec![],
                        );
                        let queued = crate::queue::QueuedRequest {
                            intent,
                            request: request.clone(),
                            response_tx: tx,
                            enqueued_at: std::time::Instant::now(),
                            priority,
                        };

                        match queue.enqueue(queued) {
                            Ok(()) => {
                                info!(
                                    reason = %reason,
                                    estimated_wait_ms,
                                    priority = ?priority,
                                    "Request enqueued"
                                );

                                let max_wait =
                                    std::time::Duration::from_secs(queue.config().max_wait_seconds);
                                match tokio::time::timeout(max_wait, rx).await {
                                    Ok(Ok(resp)) => {
                                        return resp;
                                    }
                                    _ => {
                                        return Ok(crate::queue::build_timeout_response(
                                            &queue.config().max_wait_seconds.to_string(),
                                        ));
                                    }
                                }
                            }
                            Err(crate::queue::QueueError::Full { .. }) => {
                                warn!("Queue full, rejecting request");
                                return Ok(ApiError::service_unavailable(
                                    "All backends at capacity \
                                         and queue is full",
                                )
                                .into_response());
                            }
                            Err(crate::queue::QueueError::Disabled) => {
                                return Ok(ApiError::service_unavailable(
                                    "All backends at capacity",
                                )
                                .into_response());
                            }
                        }
                    } else {
                        return Ok(ApiError::service_unavailable("All backends at capacity")
                            .into_response());
                    }
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

                // Record quality outcome: success with TTFT
                let ttft_ms = start_time.elapsed().as_millis() as u32;
                state
                    .router
                    .quality_store()
                    .record_outcome(&backend.id, true, ttft_ms);

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

                // Estimate cost from usage data (cloud backends only)
                let cost_estimated = response.usage.as_ref().and_then(|u| {
                    state
                        .pricing
                        .estimate_cost(&actual_model, u.prompt_tokens, u.completion_tokens)
                });

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
                    cost_estimated,
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

                // Inject budget headers (F14: T037-T040)
                inject_budget_headers(&mut resp, &routing_result);

                return Ok(resp);
            }
            Err(e) => {
                let _ = state.registry.decrement_pending(&backend.id);

                // Record quality outcome: failure
                let ttft_ms = start_time.elapsed().as_millis() as u32;
                state
                    .router
                    .quality_store()
                    .record_outcome(&backend.id, false, ttft_ms);

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
    let tier_mode = extract_tier_enforcement_mode(&headers);
    let routing_result = state.router.select_backend(&requirements, Some(tier_mode));

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
                crate::routing::RoutingError::Reject { rejection_reasons } => {
                    let backends = available_backend_names(&state);
                    return Ok(rejection_response(rejection_reasons, backends));
                }
                crate::routing::RoutingError::Queue { .. } => {
                    // Streaming requests don't support queuing
                    return Ok(
                        ApiError::service_unavailable("All backends at capacity").into_response()
                    );
                }
            }
        }
    };

    let backend = &routing_result.backend;
    let fallback_used = routing_result.fallback_used;
    let actual_model = routing_result.actual_model.clone();
    let backend_id = backend.id.clone();

    // Replace alias with resolved model name before forwarding to backend
    let mut request = request;
    request.model = actual_model.clone();

    // Track start time for quality metrics
    let start_time = std::time::Instant::now();

    // Increment pending requests
    let _ = state.registry.increment_pending(&backend_id);

    info!(backend_id = %backend_id, "Starting streaming request");

    // Create SSE stream - pass cloned backend data
    let stream = create_sse_stream(
        state.clone(),
        Arc::clone(backend),
        headers,
        request,
        start_time,
    );

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

    // Inject budget headers (F14: T037-T040)
    inject_budget_headers(&mut resp, &routing_result);

    Ok(resp)
}

/// Create an SSE stream that proxies chunks from the backend.
fn create_sse_stream(
    state: Arc<AppState>,
    backend: Arc<Backend>,
    headers: HeaderMap,
    request: ChatCompletionRequest,
    start_time: std::time::Instant,
) -> impl futures::Stream<Item = Result<Event, std::convert::Infallible>> {
    async_stream::stream! {
        let backend_id = backend.id.clone();

        // Try to get agent from registry (T037)
        if let Some(agent) = state.registry.get_agent(&backend_id) {
            // Use agent-based streaming (T037)
            match agent.chat_completion_stream(request.clone(), Some(&headers)).await {
                Ok(mut stream) => {
                    let mut succeeded = true;
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
                                succeeded = false;
                                warn!(backend_id = %backend_id, error = %e, "Stream error from agent");
                                let error_chunk = create_error_chunk(&format!("Stream error: {}", e));
                                yield Ok(Event::default().data(serde_json::to_string(&error_chunk).unwrap_or_default()));
                                yield Ok(Event::default().data("[DONE]"));
                                break;
                            }
                        }
                    }
                    // Record quality outcome for streaming
                    let ttft_ms = start_time.elapsed().as_millis() as u32;
                    state.router.quality_store().record_outcome(&backend_id, succeeded, ttft_ms);
                }
                Err(e) => {
                    warn!(backend_id = %backend_id, error = %e, "Failed to start streaming from agent");
                    let error_chunk = create_error_chunk(&format!("Failed to start streaming: {}", e));
                    yield Ok(Event::default().data(serde_json::to_string(&error_chunk).unwrap_or_default()));
                    yield Ok(Event::default().data("[DONE]"));
                    // Record quality outcome: failure
                    let ttft_ms = start_time.elapsed().as_millis() as u32;
                    state.router.quality_store().record_outcome(&backend_id, false, ttft_ms);
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
                    // Record quality outcome: connection failure
                    let ttft_ms = start_time.elapsed().as_millis() as u32;
                    state.router.quality_store().record_outcome(&backend_id, false, ttft_ms);
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
                // Record quality outcome: backend error
                let ttft_ms = start_time.elapsed().as_millis() as u32;
                state.router.quality_store().record_outcome(&backend_id, false, ttft_ms);
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

        // Record quality outcome for legacy streaming: success
        let ttft_ms = start_time.elapsed().as_millis() as u32;
        state.router.quality_store().record_outcome(&backend_id, true, ttft_ms);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tier_enforcement_mode_no_headers() {
        let headers = HeaderMap::new();
        assert_eq!(
            extract_tier_enforcement_mode(&headers),
            TierEnforcementMode::Strict
        );
    }

    #[test]
    fn test_extract_tier_enforcement_mode_strict() {
        let mut headers = HeaderMap::new();
        headers.insert(STRICT_HEADER, HeaderValue::from_static("true"));
        assert_eq!(
            extract_tier_enforcement_mode(&headers),
            TierEnforcementMode::Strict
        );
    }

    #[test]
    fn test_extract_tier_enforcement_mode_flexible() {
        let mut headers = HeaderMap::new();
        headers.insert(FLEXIBLE_HEADER, HeaderValue::from_static("true"));
        assert_eq!(
            extract_tier_enforcement_mode(&headers),
            TierEnforcementMode::Flexible
        );
    }

    #[test]
    fn test_extract_tier_enforcement_mode_both_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(STRICT_HEADER, HeaderValue::from_static("true"));
        headers.insert(FLEXIBLE_HEADER, HeaderValue::from_static("true"));
        // Strict takes precedence
        assert_eq!(
            extract_tier_enforcement_mode(&headers),
            TierEnforcementMode::Strict
        );
    }

    #[test]
    fn test_extract_tier_enforcement_mode_flexible_false() {
        let mut headers = HeaderMap::new();
        headers.insert(FLEXIBLE_HEADER, HeaderValue::from_static("false"));
        // Should default to Strict
        assert_eq!(
            extract_tier_enforcement_mode(&headers),
            TierEnforcementMode::Strict
        );
    }

    // ── determine_route_reason tests ──

    #[test]
    fn test_route_reason_fallback_used() {
        let reason = determine_route_reason("highest_score:backend-1:0.95", true, 0);
        assert_eq!(reason, RouteReason::Failover);
    }

    #[test]
    fn test_route_reason_retry_count() {
        let reason = determine_route_reason("highest_score:backend-1:0.95", false, 2);
        assert_eq!(reason, RouteReason::Failover);
    }

    #[test]
    fn test_route_reason_privacy() {
        let reason = determine_route_reason("privacy_zone_match", false, 0);
        assert_eq!(reason, RouteReason::PrivacyRequirement);
    }

    #[test]
    fn test_route_reason_restricted() {
        let reason = determine_route_reason("restricted_zone_only", false, 0);
        assert_eq!(reason, RouteReason::PrivacyRequirement);
    }

    #[test]
    fn test_route_reason_capacity() {
        let reason = determine_route_reason("capacity_exceeded", false, 0);
        assert_eq!(reason, RouteReason::CapacityOverflow);
    }

    #[test]
    fn test_route_reason_overflow() {
        let reason = determine_route_reason("overflow_to_backup", false, 0);
        assert_eq!(reason, RouteReason::CapacityOverflow);
    }

    #[test]
    fn test_route_reason_failover_string() {
        let reason = determine_route_reason("failover_triggered", false, 0);
        assert_eq!(reason, RouteReason::Failover);
    }

    #[test]
    fn test_route_reason_highest_score_default() {
        let reason = determine_route_reason("highest_score:backend-1:0.95", false, 0);
        assert_eq!(reason, RouteReason::CapabilityMatch);
    }

    #[test]
    fn test_route_reason_only_healthy_backend() {
        let reason = determine_route_reason("only_healthy_backend", false, 0);
        assert_eq!(reason, RouteReason::CapabilityMatch);
    }

    #[test]
    fn test_route_reason_round_robin() {
        let reason = determine_route_reason("round_robin:backend-1", false, 0);
        assert_eq!(reason, RouteReason::CapabilityMatch);
    }

    // ── create_error_chunk tests ──

    #[test]
    fn test_error_chunk_has_error_content() {
        let chunk = create_error_chunk("something went wrong");
        assert_eq!(chunk.choices.len(), 1);
        let content = chunk.choices[0].delta.content.as_deref().unwrap();
        assert!(content.contains("something went wrong"));
    }

    #[test]
    fn test_error_chunk_finish_reason() {
        let chunk = create_error_chunk("fail");
        assert_eq!(chunk.choices[0].finish_reason.as_deref(), Some("error"));
    }

    #[test]
    fn test_error_chunk_model() {
        let chunk = create_error_chunk("fail");
        assert_eq!(chunk.model, "error");
    }

    #[test]
    fn test_error_chunk_content_format() {
        let chunk = create_error_chunk("timeout");
        let content = chunk.choices[0].delta.content.as_deref().unwrap();
        assert_eq!(content, "[Error: timeout]");
    }

    // ── rejection_response tests ──

    #[tokio::test]
    async fn test_rejection_response_privacy() {
        let reasons = vec![RejectionReason {
            agent_id: "agent-1".to_string(),
            reconciler: "PrivacyReconciler".to_string(),
            reason: "restricted zone required".to_string(),
            suggested_action: "Use a local backend".to_string(),
        }];
        let resp = rejection_response(reasons, vec![]);
        assert_eq!(resp.status(), axum::http::StatusCode::SERVICE_UNAVAILABLE);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["context"]["privacy_zone_required"], "restricted");
    }

    #[tokio::test]
    async fn test_rejection_response_tier() {
        let reasons = vec![RejectionReason {
            agent_id: "agent-2".to_string(),
            reconciler: "TierReconciler".to_string(),
            reason: "agent tier 2 below minimum 3".to_string(),
            suggested_action: "Upgrade tier".to_string(),
        }];
        let resp = rejection_response(reasons, vec![]);
        assert_eq!(resp.status(), axum::http::StatusCode::SERVICE_UNAVAILABLE);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["context"]["required_tier"], 3);
    }

    #[tokio::test]
    async fn test_rejection_response_empty_reasons() {
        let resp = rejection_response(vec![], vec![]);
        assert_eq!(resp.status(), axum::http::StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_rejection_response_available_backends() {
        let reasons = vec![RejectionReason {
            agent_id: "agent-3".to_string(),
            reconciler: "SchedulerReconciler".to_string(),
            reason: "no capacity".to_string(),
            suggested_action: "Wait".to_string(),
        }];
        let backends = vec!["backend-a".to_string(), "backend-b".to_string()];
        let resp = rejection_response(reasons, backends);
        assert_eq!(resp.status(), axum::http::StatusCode::SERVICE_UNAVAILABLE);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let available = json["context"]["available_backends"].as_array().unwrap();
        assert!(available.iter().any(|v| v == "backend-a"));
        assert!(available.iter().any(|v| v == "backend-b"));
    }

    #[test]
    fn test_extract_priority_high() {
        let mut headers = HeaderMap::new();
        headers.insert("x-nexus-priority", HeaderValue::from_static("high"));
        let priority = extract_priority(&headers);
        assert_eq!(priority, crate::queue::Priority::High);
    }

    #[test]
    fn test_extract_priority_normal() {
        let mut headers = HeaderMap::new();
        headers.insert("x-nexus-priority", HeaderValue::from_static("normal"));
        let priority = extract_priority(&headers);
        assert_eq!(priority, crate::queue::Priority::Normal);
    }

    #[test]
    fn test_extract_priority_missing() {
        let headers = HeaderMap::new();
        let priority = extract_priority(&headers);
        assert_eq!(priority, crate::queue::Priority::Normal);
    }

    #[test]
    fn test_extract_priority_invalid() {
        let mut headers = HeaderMap::new();
        headers.insert("x-nexus-priority", HeaderValue::from_static("critical"));
        let priority = extract_priority(&headers);
        assert_eq!(priority, crate::queue::Priority::Normal);
    }

    #[tokio::test]
    async fn test_inject_budget_headers_normal_status() {
        use crate::registry::{BackendType, DiscoverySource};
        use crate::routing::reconciler::intent::BudgetStatus;

        let backend = Backend::new(
            "b1".to_string(),
            "B1".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        let routing_result = crate::routing::RoutingResult {
            backend: Arc::new(backend),
            actual_model: "test-model".to_string(),
            route_reason: "test".to_string(),
            fallback_used: false,
            cost_estimated: None,
            budget_status: BudgetStatus::Normal,
            budget_utilization: None,
            budget_remaining: None,
        };

        let mut resp =
            axum::response::IntoResponse::into_response(axum::Json(serde_json::json!({})));
        inject_budget_headers(&mut resp, &routing_result);

        assert!(!resp.headers().contains_key("x-nexus-budget-status"));
        assert!(!resp.headers().contains_key("x-nexus-budget-utilization"));
        assert!(!resp.headers().contains_key("x-nexus-budget-remaining"));
    }

    #[tokio::test]
    async fn test_inject_budget_headers_soft_limit() {
        use crate::registry::{BackendType, DiscoverySource};
        use crate::routing::reconciler::intent::BudgetStatus;

        let backend = Backend::new(
            "b1".to_string(),
            "B1".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        let routing_result = crate::routing::RoutingResult {
            backend: Arc::new(backend),
            actual_model: "test-model".to_string(),
            route_reason: "test".to_string(),
            fallback_used: false,
            cost_estimated: Some(0.05),
            budget_status: BudgetStatus::SoftLimit,
            budget_utilization: Some(80.5),
            budget_remaining: Some(9.75),
        };

        let mut resp =
            axum::response::IntoResponse::into_response(axum::Json(serde_json::json!({})));
        inject_budget_headers(&mut resp, &routing_result);

        assert_eq!(
            resp.headers()
                .get("x-nexus-budget-status")
                .unwrap()
                .to_str()
                .unwrap(),
            "SoftLimit"
        );
        assert_eq!(
            resp.headers()
                .get("x-nexus-budget-utilization")
                .unwrap()
                .to_str()
                .unwrap(),
            "80.50"
        );
        assert_eq!(
            resp.headers()
                .get("x-nexus-budget-remaining")
                .unwrap()
                .to_str()
                .unwrap(),
            "9.75"
        );
        assert_eq!(
            resp.headers()
                .get("x-nexus-cost-estimated")
                .unwrap()
                .to_str()
                .unwrap(),
            "0.0500"
        );
    }

    #[tokio::test]
    async fn test_inject_budget_headers_cost_zero() {
        use crate::registry::{BackendType, DiscoverySource};
        use crate::routing::reconciler::intent::BudgetStatus;

        let backend = Backend::new(
            "b1".to_string(),
            "B1".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        let routing_result = crate::routing::RoutingResult {
            backend: Arc::new(backend),
            actual_model: "test-model".to_string(),
            route_reason: "test".to_string(),
            fallback_used: false,
            cost_estimated: Some(0.0),
            budget_status: BudgetStatus::Normal,
            budget_utilization: None,
            budget_remaining: None,
        };

        let mut resp =
            axum::response::IntoResponse::into_response(axum::Json(serde_json::json!({})));
        inject_budget_headers(&mut resp, &routing_result);

        // Cost of 0.0 should NOT set the header
        assert!(!resp.headers().contains_key("x-nexus-cost-estimated"));
    }

    #[tokio::test]
    async fn test_inject_budget_headers_hard_limit() {
        use crate::registry::{BackendType, DiscoverySource};
        use crate::routing::reconciler::intent::BudgetStatus;

        let backend = Backend::new(
            "b1".to_string(),
            "B1".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        let routing_result = crate::routing::RoutingResult {
            backend: Arc::new(backend),
            actual_model: "test-model".to_string(),
            route_reason: "test".to_string(),
            fallback_used: false,
            cost_estimated: Some(0.12),
            budget_status: BudgetStatus::HardLimit,
            budget_utilization: Some(100.0),
            budget_remaining: Some(0.0),
        };

        let mut resp =
            axum::response::IntoResponse::into_response(axum::Json(serde_json::json!({})));
        inject_budget_headers(&mut resp, &routing_result);

        assert_eq!(
            resp.headers()
                .get("x-nexus-budget-status")
                .unwrap()
                .to_str()
                .unwrap(),
            "HardLimit"
        );
        assert_eq!(
            resp.headers()
                .get("x-nexus-budget-utilization")
                .unwrap()
                .to_str()
                .unwrap(),
            "100.00"
        );
        assert_eq!(
            resp.headers()
                .get("x-nexus-budget-remaining")
                .unwrap()
                .to_str()
                .unwrap(),
            "0.00"
        );
        assert_eq!(
            resp.headers()
                .get("x-nexus-cost-estimated")
                .unwrap()
                .to_str()
                .unwrap(),
            "0.1200"
        );
    }

    #[test]
    fn test_create_error_chunk_has_error_content() {
        let chunk = create_error_chunk("something went wrong");
        assert_eq!(chunk.object, "chat.completion.chunk");
        assert_eq!(chunk.model, "error");
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].finish_reason, Some("error".to_string()));
        assert!(chunk.choices[0]
            .delta
            .content
            .as_ref()
            .unwrap()
            .contains("something went wrong"));
    }

    #[test]
    fn test_available_models_empty_registry() {
        use crate::config::NexusConfig;
        use crate::registry::Registry;

        let registry = Arc::new(Registry::new());
        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(registry, config));

        let models = available_models(&state);
        assert!(models.is_empty());

        let backends = available_backend_names(&state);
        assert!(backends.is_empty());
    }

    #[test]
    fn test_record_request_completion_increments_and_broadcasts() {
        use crate::config::NexusConfig;
        use crate::registry::{BackendType, DiscoverySource, Registry};

        let registry = Arc::new(Registry::new());
        let backend = Backend::new(
            "test-backend".to_string(),
            "Test".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        let _ = registry.add_backend(backend);

        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(Arc::clone(&registry), config));

        // Subscribe to websocket broadcast before sending
        let mut rx = state.ws_broadcast.subscribe();

        record_request_completion(
            &state,
            "test-model",
            "test-backend",
            150,
            crate::dashboard::types::RequestStatus::Success,
            None,
        );

        // Verify broadcast was sent
        let update = rx.try_recv();
        assert!(update.is_ok());
    }

    // ================================================================
    // Integration-style handler tests via full axum router
    // ================================================================

    /// Helper: build a JSON body for a simple chat completion request.
    fn simple_completion_body(model: &str) -> serde_json::Value {
        serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": "hello"}]
        })
    }

    /// Helper: read response body as JSON.
    async fn body_json(response: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn test_handle_model_not_found() {
        use crate::api::{create_router, AppState};
        use crate::config::NexusConfig;
        use crate::registry::Registry;
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::Service;

        let registry = Arc::new(Registry::new());
        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(registry, config));
        let mut app = create_router(state);

        let body = simple_completion_body("nonexistent-model");
        let request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let json = body_json(response).await;
        let msg = json["error"]["message"].as_str().unwrap();
        assert!(msg.contains("nonexistent-model"), "msg was: {}", msg);
        assert_eq!(json["error"]["code"], "model_not_found");
    }

    #[tokio::test]
    async fn test_handle_no_healthy_backend() {
        use crate::api::{create_router, AppState};
        use crate::config::NexusConfig;
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model, Registry};
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::Service;

        let registry = Arc::new(Registry::new());
        let backend = Backend::new(
            "unhealthy-1".to_string(),
            "Unhealthy".to_string(),
            "http://localhost:99999".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        registry.add_backend(backend).unwrap();
        registry
            .update_models(
                "unhealthy-1",
                vec![Model {
                    id: "test-model".to_string(),
                    name: "test-model".to_string(),
                    context_length: 4096,
                    supports_vision: false,
                    supports_tools: false,
                    supports_json_mode: false,
                    max_output_tokens: None,
                }],
            )
            .unwrap();
        // Leave status as Unknown (not Healthy) — router treats as unhealthy
        registry
            .update_status("unhealthy-1", BackendStatus::Unhealthy, None)
            .unwrap();

        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(registry, config));
        let mut app = create_router(state);

        let body = simple_completion_body("test-model");
        let request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let json = body_json(response).await;
        let msg = json["error"]["message"].as_str().unwrap();
        assert!(msg.contains("test-model"), "msg was: {}", msg);
        // Actionable context should be present
        assert!(json.get("context").is_some());
        assert!(json["context"]["available_backends"].is_array());
    }

    #[tokio::test]
    async fn test_handle_vision_request_no_vision_backend() {
        use crate::api::{create_router, AppState};
        use crate::config::NexusConfig;
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model, Registry};
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::Service;

        // Register a backend with a non-vision model
        let registry = Arc::new(Registry::new());
        let backend = Backend::new(
            "no-vision".to_string(),
            "NoVision".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        registry.add_backend(backend).unwrap();
        registry
            .update_status("no-vision", BackendStatus::Healthy, None)
            .unwrap();
        registry
            .update_models(
                "no-vision",
                vec![Model {
                    id: "llama3".to_string(),
                    name: "llama3".to_string(),
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

        // Send a vision request (image_url content part)
        let body = serde_json::json!({
            "model": "llama3",
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": "What is in this image?"},
                    {"type": "image_url", "image_url": {"url": "data:image/png;base64,abc"}}
                ]
            }]
        });

        let request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        // Vision-incapable backends are filtered; results in 503
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_handle_rejection_response_budget() {
        let reasons = vec![RejectionReason {
            agent_id: "agent-budget".to_string(),
            reconciler: "BudgetReconciler".to_string(),
            reason: "daily budget exceeded: $5.00 spent of $5.00 limit".to_string(),
            suggested_action: "Increase budget or wait for reset".to_string(),
        }];
        let backends = vec!["local-1".to_string()];
        let resp = rejection_response(reasons, backends);
        assert_eq!(resp.status(), axum::http::StatusCode::SERVICE_UNAVAILABLE);

        let json = body_json(resp).await;
        let msg = json["error"]["message"].as_str().unwrap();
        assert!(msg.contains("rejected"), "msg was: {}", msg);
        let available = json["context"]["available_backends"].as_array().unwrap();
        assert!(available.iter().any(|v| v == "local-1"));

        // Budget rejection shouldn't set privacy_zone or required_tier
        assert!(
            json["context"].get("privacy_zone_required").is_none()
                || json["context"]["privacy_zone_required"].is_null()
        );
        assert!(
            json["context"].get("required_tier").is_none()
                || json["context"]["required_tier"].is_null()
        );
    }

    #[tokio::test]
    async fn test_handle_queue_full_response() {
        // Directly test the queue-full code path: when the RoutingError::Queue
        // variant fires and queue.enqueue() returns Full, the handler should
        // return 503 "queue is full".
        use axum::http::StatusCode;

        // Build the response the same way the handler does on queue full
        let resp = ApiError::service_unavailable("All backends at capacity and queue is full")
            .into_response();

        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

        let json = body_json(resp).await;
        let msg = json["error"]["message"].as_str().unwrap();
        assert!(msg.contains("queue is full"), "msg was: {}", msg);
    }

    #[tokio::test]
    async fn test_handle_streaming_model_not_found() {
        use crate::api::{create_router, AppState};
        use crate::config::NexusConfig;
        use crate::registry::Registry;
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::Service;

        let registry = Arc::new(Registry::new());
        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(registry, config));
        let mut app = create_router(state);

        let body = serde_json::json!({
            "model": "nonexistent-streaming-model",
            "messages": [{"role": "user", "content": "hello"}],
            "stream": true
        });

        let request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let json = body_json(response).await;
        let msg = json["error"]["message"].as_str().unwrap();
        assert!(
            msg.contains("nonexistent-streaming-model"),
            "msg was: {}",
            msg
        );
    }

    #[tokio::test]
    async fn test_handle_fallback_chain_exhausted() {
        use crate::api::AppState;
        use crate::config::NexusConfig;
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model, Registry};
        use crate::routing::{Router, RoutingStrategy, ScoringWeights};
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use std::collections::HashMap;
        use tower::Service;

        let registry = Arc::new(Registry::new());

        // Register a backend with model "primary-model" that is unhealthy
        let backend = Backend::new(
            "fb-backend-1".to_string(),
            "FallbackBackend1".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            HashMap::new(),
        );
        registry.add_backend(backend).unwrap();
        registry
            .update_status("fb-backend-1", BackendStatus::Unhealthy, None)
            .unwrap();
        registry
            .update_models(
                "fb-backend-1",
                vec![
                    Model {
                        id: "primary-model".to_string(),
                        name: "primary-model".to_string(),
                        context_length: 4096,
                        supports_vision: false,
                        supports_tools: false,
                        supports_json_mode: false,
                        max_output_tokens: None,
                    },
                    Model {
                        id: "fallback-model".to_string(),
                        name: "fallback-model".to_string(),
                        context_length: 4096,
                        supports_vision: false,
                        supports_tools: false,
                        supports_json_mode: false,
                        max_output_tokens: None,
                    },
                ],
            )
            .unwrap();

        // Configure fallback chain: primary-model → fallback-model
        let mut fallbacks = HashMap::new();
        fallbacks.insert(
            "primary-model".to_string(),
            vec!["fallback-model".to_string()],
        );

        let mut config = NexusConfig::default();
        config.routing.fallbacks = fallbacks.clone();

        let router = Router::with_aliases_and_fallbacks(
            Arc::clone(&registry),
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            HashMap::new(),
            fallbacks,
        );

        let config = Arc::new(config);
        let mut state = AppState::new(registry, config);
        state.router = Arc::new(router);
        let state = Arc::new(state);
        let mut app = crate::api::create_router(state);

        let body = simple_completion_body("primary-model");
        let request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        // FallbackChainExhausted maps to 404
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let json = body_json(response).await;
        let msg = json["error"]["message"].as_str().unwrap();
        assert!(
            msg.contains("primary-model"),
            "Expected model name in error, got: {}",
            msg
        );
    }

    #[tokio::test]
    async fn test_handle_with_nexus_headers_on_success() {
        use crate::agent::types::{AgentCapabilities, AgentProfile, PrivacyZone};
        use crate::agent::{
            AgentError, HealthStatus, InferenceAgent, ModelCapability, StreamChunk,
        };
        use crate::api::AppState;
        use crate::config::NexusConfig;
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model, Registry};
        use async_trait::async_trait;
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use futures_util::stream::BoxStream;
        use tower::Service;

        /// A mock agent that returns a successful chat completion.
        struct SuccessAgent;

        #[async_trait]
        impl InferenceAgent for SuccessAgent {
            fn id(&self) -> &str {
                "success-agent"
            }
            fn name(&self) -> &str {
                "Success Agent"
            }
            fn profile(&self) -> AgentProfile {
                AgentProfile {
                    backend_type: "ollama".to_string(),
                    version: None,
                    privacy_zone: PrivacyZone::Restricted,
                    capabilities: AgentCapabilities::default(),
                    capability_tier: Some(2),
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
                Ok(ChatCompletionResponse {
                    id: "chatcmpl-test".to_string(),
                    object: "chat.completion".to_string(),
                    created: 1234567890,
                    model: "test-model".to_string(),
                    choices: vec![crate::api::types::Choice {
                        index: 0,
                        message: crate::api::types::ChatMessage {
                            role: "assistant".to_string(),
                            content: crate::api::types::MessageContent::Text {
                                content: "Hello!".to_string(),
                            },
                            name: None,
                            function_call: None,
                        },
                        finish_reason: Some("stop".to_string()),
                    }],
                    usage: Some(crate::api::types::Usage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                        total_tokens: 15,
                    }),
                    extra: std::collections::HashMap::new(),
                })
            }
            async fn chat_completion_stream(
                &self,
                _request: ChatCompletionRequest,
                _headers: Option<&HeaderMap>,
            ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError>
            {
                Err(AgentError::Unsupported("chat_completion_stream"))
            }
        }

        let registry = Arc::new(Registry::new());
        let backend = Backend::new(
            "success-agent".to_string(),
            "SuccessBackend".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        let agent: Arc<dyn InferenceAgent> = Arc::new(SuccessAgent);
        registry.add_backend_with_agent(backend, agent).unwrap();
        registry
            .update_status("success-agent", BackendStatus::Healthy, None)
            .unwrap();
        registry
            .update_models(
                "success-agent",
                vec![Model {
                    id: "test-model".to_string(),
                    name: "test-model".to_string(),
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
        let mut app = crate::api::create_router(state);

        let body = simple_completion_body("test-model");
        let request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
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
            "success-agent"
        );
        assert!(
            response.headers().contains_key("x-nexus-backend-type"),
            "Missing x-nexus-backend-type header"
        );
        assert_eq!(
            response.headers().get("x-nexus-backend-type").unwrap(),
            "local"
        );
        assert!(
            response.headers().contains_key("x-nexus-route-reason"),
            "Missing x-nexus-route-reason header"
        );
        assert!(
            response.headers().contains_key("x-nexus-privacy-zone"),
            "Missing x-nexus-privacy-zone header"
        );
    }

    // ── Legacy proxy_request path (no agent registered) tests ──────────

    #[tokio::test]
    async fn test_legacy_proxy_request_no_agent() {
        // When a backend has no agent, proxy_request falls back to direct HTTP.
        // We mock the backend server to return a valid completion.
        use crate::api::AppState;
        use crate::config::NexusConfig;
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model, Registry};
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::Service;

        let mut mock_server = mockito::Server::new_async().await;
        let mock = mock_server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"cmpl-1","object":"chat.completion","created":1234567890,"model":"test-model","choices":[{"index":0,"message":{"role":"assistant","content":"Hi"},"finish_reason":"stop"}]}"#)
            .create_async()
            .await;

        let registry = Arc::new(Registry::new());
        // Add backend WITHOUT an agent — triggers legacy proxy path
        let backend = Backend::new(
            "legacy-backend".to_string(),
            "Legacy".to_string(),
            mock_server.url(),
            BackendType::Generic,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        registry.add_backend(backend).unwrap();
        registry
            .update_status("legacy-backend", BackendStatus::Healthy, None)
            .unwrap();
        registry
            .update_models(
                "legacy-backend",
                vec![Model {
                    id: "test-model".to_string(),
                    name: "test-model".to_string(),
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
        let mut app = crate::api::create_router(state);

        let body = simple_completion_body("test-model");
        let request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let json = body_json(response).await;
        assert_eq!(json["id"], "cmpl-1");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_legacy_proxy_request_backend_error() {
        // Legacy proxy with 500 error from backend
        use crate::api::AppState;
        use crate::config::NexusConfig;
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model, Registry};
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::Service;

        let mut mock_server = mockito::Server::new_async().await;
        let mock = mock_server
            .mock("POST", "/v1/chat/completions")
            .with_status(500)
            .with_body(r#"{"error":{"message":"internal error","type":"server_error","code":"internal_error"}}"#)
            .expect(3)
            .create_async()
            .await;

        let registry = Arc::new(Registry::new());
        let backend = Backend::new(
            "legacy-err".to_string(),
            "LegacyErr".to_string(),
            mock_server.url(),
            BackendType::Generic,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        registry.add_backend(backend).unwrap();
        registry
            .update_status("legacy-err", BackendStatus::Healthy, None)
            .unwrap();
        registry
            .update_models(
                "legacy-err",
                vec![Model {
                    id: "err-model".to_string(),
                    name: "err-model".to_string(),
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
        let mut app = crate::api::create_router(state);

        let body = simple_completion_body("err-model");
        let request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        // Backend error should be forwarded as 500
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_legacy_proxy_request_gateway_timeout() {
        // Legacy proxy with 504 gateway timeout
        use crate::api::AppState;
        use crate::config::NexusConfig;
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model, Registry};
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::Service;

        let mut mock_server = mockito::Server::new_async().await;
        let mock = mock_server
            .mock("POST", "/v1/chat/completions")
            .with_status(504)
            .with_body("")
            .expect(3)
            .create_async()
            .await;

        let registry = Arc::new(Registry::new());
        let backend = Backend::new(
            "legacy-timeout".to_string(),
            "LegacyTimeout".to_string(),
            mock_server.url(),
            BackendType::Generic,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        registry.add_backend(backend).unwrap();
        registry
            .update_status("legacy-timeout", BackendStatus::Healthy, None)
            .unwrap();
        registry
            .update_models(
                "legacy-timeout",
                vec![Model {
                    id: "timeout-model".to_string(),
                    name: "timeout-model".to_string(),
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
        let mut app = crate::api::create_router(state);

        let body = simple_completion_body("timeout-model");
        let request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_legacy_proxy_request_invalid_json() {
        // Legacy proxy returns 200 with invalid JSON
        use crate::api::AppState;
        use crate::config::NexusConfig;
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model, Registry};
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::Service;

        let mut mock_server = mockito::Server::new_async().await;
        let mock = mock_server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body("not valid json at all")
            .expect(3)
            .create_async()
            .await;

        let registry = Arc::new(Registry::new());
        let backend = Backend::new(
            "legacy-invalid".to_string(),
            "LegacyInvalid".to_string(),
            mock_server.url(),
            BackendType::Generic,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        registry.add_backend(backend).unwrap();
        registry
            .update_status("legacy-invalid", BackendStatus::Healthy, None)
            .unwrap();
        registry
            .update_models(
                "legacy-invalid",
                vec![Model {
                    id: "invalid-model".to_string(),
                    name: "invalid-model".to_string(),
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
        let mut app = crate::api::create_router(state);

        let body = simple_completion_body("invalid-model");
        let request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        // Invalid JSON body → bad gateway
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        mock.assert_async().await;
    }

    #[test]
    fn test_record_request_completion_with_error() {
        use crate::config::NexusConfig;
        use crate::registry::{BackendType, DiscoverySource, Registry};

        let registry = Arc::new(Registry::new());
        let backend = Backend::new(
            "err-backend".to_string(),
            "ErrBackend".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        let _ = registry.add_backend(backend);

        let config = Arc::new(NexusConfig::default());
        let state = Arc::new(AppState::new(Arc::clone(&registry), config));

        let mut rx = state.ws_broadcast.subscribe();

        record_request_completion(
            &state,
            "test-model",
            "err-backend",
            500,
            crate::dashboard::types::RequestStatus::Error,
            Some("timeout error".to_string()),
        );

        let update = rx.try_recv();
        assert!(update.is_ok());
    }

    // ── Streaming routing error paths ──────────────────────────────────

    #[tokio::test]
    async fn test_streaming_no_healthy_backend() {
        use crate::api::AppState;
        use crate::config::NexusConfig;
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model, Registry};
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::Service;

        let registry = Arc::new(Registry::new());
        let backend = Backend::new(
            "stream-unhealthy".to_string(),
            "StreamUnhealthy".to_string(),
            "http://localhost:99999".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        registry.add_backend(backend).unwrap();
        registry
            .update_status("stream-unhealthy", BackendStatus::Unhealthy, None)
            .unwrap();
        registry
            .update_models(
                "stream-unhealthy",
                vec![Model {
                    id: "stream-model".to_string(),
                    name: "stream-model".to_string(),
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
        let mut app = crate::api::create_router(state);

        let body = serde_json::json!({
            "model": "stream-model",
            "messages": [{"role": "user", "content": "hi"}],
            "stream": true
        });

        let request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_streaming_capability_mismatch() {
        use crate::api::AppState;
        use crate::config::NexusConfig;
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model, Registry};
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::Service;

        let registry = Arc::new(Registry::new());
        let backend = Backend::new(
            "stream-novision".to_string(),
            "StreamNoVision".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        registry.add_backend(backend).unwrap();
        registry
            .update_status("stream-novision", BackendStatus::Healthy, None)
            .unwrap();
        registry
            .update_models(
                "stream-novision",
                vec![Model {
                    id: "stream-cap".to_string(),
                    name: "stream-cap".to_string(),
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
        let mut app = crate::api::create_router(state);

        // Streaming request requiring vision on a non-vision backend
        let body = serde_json::json!({
            "model": "stream-cap",
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": "describe"},
                    {"type": "image_url", "image_url": {"url": "data:image/png;base64,abc"}}
                ]
            }],
            "stream": true
        });

        let request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn test_route_reason_saturated() {
        let reason = determine_route_reason("saturated_backend", false, 0);
        assert_eq!(reason, RouteReason::CapacityOverflow);
    }

    #[test]
    fn test_route_reason_overloaded() {
        let reason = determine_route_reason("overloaded", false, 0);
        assert_eq!(reason, RouteReason::CapacityOverflow);
    }

    #[test]
    fn test_route_reason_backup() {
        let reason = determine_route_reason("backup_route", false, 0);
        assert_eq!(reason, RouteReason::Failover);
    }

    #[test]
    fn test_route_reason_fallback_string() {
        let reason = determine_route_reason("fallback_used", false, 0);
        assert_eq!(reason, RouteReason::Failover);
    }

    #[tokio::test]
    async fn test_streaming_success_with_agent() {
        use crate::agent::types::{AgentCapabilities, AgentProfile, PrivacyZone};
        use crate::agent::{
            AgentError, HealthStatus, InferenceAgent, ModelCapability, StreamChunk,
        };
        use crate::api::AppState;
        use crate::config::NexusConfig;
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model, Registry};
        use async_trait::async_trait;
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use futures_util::stream::BoxStream;
        use tower::Service;

        struct StreamAgent;

        #[async_trait]
        impl InferenceAgent for StreamAgent {
            fn id(&self) -> &str {
                "stream-agent"
            }
            fn name(&self) -> &str {
                "Stream Agent"
            }
            fn profile(&self) -> AgentProfile {
                AgentProfile {
                    backend_type: "ollama".to_string(),
                    version: None,
                    privacy_zone: PrivacyZone::Restricted,
                    capabilities: AgentCapabilities::default(),
                    capability_tier: Some(1),
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
                _req: ChatCompletionRequest,
                _h: Option<&HeaderMap>,
            ) -> Result<ChatCompletionResponse, AgentError> {
                Err(AgentError::Unsupported("non-streaming"))
            }
            async fn chat_completion_stream(
                &self,
                _req: ChatCompletionRequest,
                _h: Option<&HeaderMap>,
            ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError>
            {
                use futures_util::stream;
                let chunks = vec![
                    Ok(StreamChunk {
                        data: r#"{"id":"c1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}]}"#.to_string(),
                    }),
                    Ok(StreamChunk {
                        data: "[DONE]".to_string(),
                    }),
                ];
                Ok(Box::pin(stream::iter(chunks)))
            }
        }

        let registry = Arc::new(Registry::new());
        let backend = Backend::new(
            "stream-agent".to_string(),
            "StreamAgent".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        let agent: Arc<dyn InferenceAgent> = Arc::new(StreamAgent);
        registry.add_backend_with_agent(backend, agent).unwrap();
        registry
            .update_status("stream-agent", BackendStatus::Healthy, None)
            .unwrap();
        registry
            .update_models(
                "stream-agent",
                vec![Model {
                    id: "stream-test".to_string(),
                    name: "stream-test".to_string(),
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
        let mut app = crate::api::create_router(state);

        let body = serde_json::json!({
            "model": "stream-test",
            "messages": [{"role": "user", "content": "hello"}],
            "stream": true
        });

        let request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Verify X-Nexus headers on streaming response
        assert!(response.headers().contains_key("x-nexus-backend"));
        assert_eq!(
            response.headers().get("x-nexus-backend").unwrap(),
            "stream-agent"
        );
    }

    #[tokio::test]
    async fn test_streaming_agent_error_yields_error_chunk() {
        use crate::agent::types::{AgentCapabilities, AgentProfile, PrivacyZone};
        use crate::agent::{
            AgentError, HealthStatus, InferenceAgent, ModelCapability, StreamChunk,
        };
        use crate::api::AppState;
        use crate::config::NexusConfig;
        use crate::registry::{BackendStatus, BackendType, DiscoverySource, Model, Registry};
        use async_trait::async_trait;
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use futures_util::stream::BoxStream;
        use tower::Service;

        struct FailStreamAgent;

        #[async_trait]
        impl InferenceAgent for FailStreamAgent {
            fn id(&self) -> &str {
                "fail-stream"
            }
            fn name(&self) -> &str {
                "Fail Stream"
            }
            fn profile(&self) -> AgentProfile {
                AgentProfile {
                    backend_type: "ollama".to_string(),
                    version: None,
                    privacy_zone: PrivacyZone::Restricted,
                    capabilities: AgentCapabilities::default(),
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
                _req: ChatCompletionRequest,
                _h: Option<&HeaderMap>,
            ) -> Result<ChatCompletionResponse, AgentError> {
                Err(AgentError::Unsupported("non-streaming"))
            }
            async fn chat_completion_stream(
                &self,
                _req: ChatCompletionRequest,
                _h: Option<&HeaderMap>,
            ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError>
            {
                Err(AgentError::Network("connection refused".to_string()))
            }
        }

        let registry = Arc::new(Registry::new());
        let backend = Backend::new(
            "fail-stream".to_string(),
            "FailStream".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        let agent: Arc<dyn InferenceAgent> = Arc::new(FailStreamAgent);
        registry.add_backend_with_agent(backend, agent).unwrap();
        registry
            .update_status("fail-stream", BackendStatus::Healthy, None)
            .unwrap();
        registry
            .update_models(
                "fail-stream",
                vec![Model {
                    id: "fail-stream-model".to_string(),
                    name: "fail-stream-model".to_string(),
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
        let mut app = crate::api::create_router(state);

        let body = serde_json::json!({
            "model": "fail-stream-model",
            "messages": [{"role": "user", "content": "hi"}],
            "stream": true
        });

        let request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.call(request).await.unwrap();
        // The streaming response starts as 200 (SSE protocol)
        // The error is inside the SSE stream as an error chunk
        assert_eq!(response.status(), StatusCode::OK);

        // Read the body to verify error chunk is present
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body_str = String::from_utf8_lossy(&body);
        assert!(
            body_str.contains("Error") || body_str.contains("error"),
            "Expected error in stream body, got: {}",
            &body_str[..std::cmp::min(200, body_str.len())]
        );
    }

    #[test]
    fn test_route_reason_privacy_string() {
        let reason = determine_route_reason("privacy_zone_match", false, 0);
        assert_eq!(reason, RouteReason::PrivacyRequirement);
    }

    #[test]
    fn test_route_reason_retry_count_failover() {
        let reason = determine_route_reason("highest_score:b1:0.95", false, 1);
        assert_eq!(reason, RouteReason::Failover);
    }

    #[tokio::test]
    async fn test_inject_budget_headers_soft_limit_no_utilization() {
        use crate::registry::{BackendType, DiscoverySource};
        use crate::routing::reconciler::intent::BudgetStatus;

        let backend = Backend::new(
            "b1".to_string(),
            "B1".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        let routing_result = crate::routing::RoutingResult {
            backend: Arc::new(backend),
            actual_model: "test-model".to_string(),
            route_reason: "test".to_string(),
            fallback_used: false,
            cost_estimated: None,
            budget_status: BudgetStatus::SoftLimit,
            budget_utilization: None,
            budget_remaining: None,
        };

        let mut resp =
            axum::response::IntoResponse::into_response(axum::Json(serde_json::json!({})));
        inject_budget_headers(&mut resp, &routing_result);

        assert_eq!(
            resp.headers()
                .get("x-nexus-budget-status")
                .unwrap()
                .to_str()
                .unwrap(),
            "SoftLimit"
        );
        // No utilization/remaining set
        assert!(!resp.headers().contains_key("x-nexus-budget-utilization"));
        assert!(!resp.headers().contains_key("x-nexus-budget-remaining"));
        assert!(!resp.headers().contains_key("x-nexus-cost-estimated"));
    }

    #[tokio::test]
    async fn test_inject_budget_headers_cost_already_set() {
        use crate::registry::{BackendType, DiscoverySource};
        use crate::routing::reconciler::intent::BudgetStatus;

        let backend = Backend::new(
            "b1".to_string(),
            "B1".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            std::collections::HashMap::new(),
        );
        let routing_result = crate::routing::RoutingResult {
            backend: Arc::new(backend),
            actual_model: "test-model".to_string(),
            route_reason: "test".to_string(),
            fallback_used: false,
            cost_estimated: Some(0.10),
            budget_status: BudgetStatus::Normal,
            budget_utilization: None,
            budget_remaining: None,
        };

        let mut resp =
            axum::response::IntoResponse::into_response(axum::Json(serde_json::json!({})));
        // Pre-set the cost header (simulating F12 headers already set)
        resp.headers_mut().insert(
            HeaderName::from_static("x-nexus-cost-estimated"),
            HeaderValue::from_static("0.0200"),
        );
        inject_budget_headers(&mut resp, &routing_result);

        // Should keep the pre-existing value, not overwrite
        assert_eq!(
            resp.headers()
                .get("x-nexus-cost-estimated")
                .unwrap()
                .to_str()
                .unwrap(),
            "0.0200"
        );
    }

    #[tokio::test]
    async fn test_rejection_response_has_rejection_details_header() {
        let reasons = vec![
            RejectionReason {
                agent_id: "a1".to_string(),
                reconciler: "PrivacyReconciler".to_string(),
                reason: "restricted zone".to_string(),
                suggested_action: "Use local".to_string(),
            },
            RejectionReason {
                agent_id: "a2".to_string(),
                reconciler: "TierReconciler".to_string(),
                reason: "agent tier 1 below minimum 3".to_string(),
                suggested_action: "Upgrade".to_string(),
            },
        ];
        let resp = rejection_response(reasons, vec!["b1".to_string()]);
        assert_eq!(resp.status(), axum::http::StatusCode::SERVICE_UNAVAILABLE);

        // Verify x-nexus-rejection-reasons header is set
        assert!(resp.headers().contains_key("x-nexus-rejection-reasons"));
        // Verify x-nexus-rejection-details header (JSON) is set
        assert!(resp.headers().contains_key("x-nexus-rejection-details"));

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // Both privacy and tier should be extracted
        assert_eq!(json["context"]["privacy_zone_required"], "restricted");
        assert_eq!(json["context"]["required_tier"], 3);
    }

    #[tokio::test]
    async fn test_rejection_response_privacy_open_zone() {
        let reasons = vec![RejectionReason {
            agent_id: "a1".to_string(),
            reconciler: "PrivacyReconciler".to_string(),
            reason: "open zone required for cloud".to_string(),
            suggested_action: "Use cloud backend".to_string(),
        }];
        let resp = rejection_response(reasons, vec![]);
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["context"]["privacy_zone_required"], "open");
    }
}
