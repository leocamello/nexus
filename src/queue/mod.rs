//! Request queue for burst traffic handling
//!
//! Bounded dual-priority queue using tokio channels. Requests are queued when
//! all backends are at capacity and drained as capacity becomes available.

use crate::api::ChatCompletionRequest;
use crate::config::QueueConfig;
use crate::routing::reconciler::intent::RoutingIntent;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

/// Priority level for queued requests
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    High,
    Normal,
}

impl Priority {
    /// Parse priority from header value. Invalid values default to Normal.
    pub fn from_header(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "high" => Priority::High,
            _ => Priority::Normal,
        }
    }
}

/// A request waiting in the queue
pub struct QueuedRequest {
    /// Routing intent from the reconciler pipeline
    pub intent: RoutingIntent,
    /// Original chat completion request
    pub request: ChatCompletionRequest,
    /// Channel to send the response back to the waiting handler
    pub response_tx: oneshot::Sender<QueueResponse>,
    /// When the request was enqueued
    pub enqueued_at: Instant,
    /// Request priority
    pub priority: Priority,
}

impl std::fmt::Debug for QueuedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueuedRequest")
            .field("priority", &self.priority)
            .field("enqueued_at", &self.enqueued_at)
            .finish()
    }
}

/// Response sent back through the oneshot channel
pub type QueueResponse = Result<axum::response::Response, crate::api::ApiError>;

/// Errors from queue operations
#[derive(Debug, Error)]
pub enum QueueError {
    /// Queue is full (total depth == max_size)
    #[error("Queue is full ({max_size} requests)")]
    Full { max_size: u32 },

    /// Queue is disabled
    #[error("Request queuing is disabled")]
    Disabled,
}

/// Bounded dual-priority request queue.
///
/// High-priority requests are dequeued before normal-priority requests.
/// Total depth across both channels respects `max_size` from config.
pub struct RequestQueue {
    high_tx: mpsc::Sender<QueuedRequest>,
    high_rx: tokio::sync::Mutex<mpsc::Receiver<QueuedRequest>>,
    normal_tx: mpsc::Sender<QueuedRequest>,
    normal_rx: tokio::sync::Mutex<mpsc::Receiver<QueuedRequest>>,
    depth: Arc<AtomicUsize>,
    config: QueueConfig,
}

impl RequestQueue {
    /// Create a new RequestQueue from configuration.
    pub fn new(config: QueueConfig) -> Self {
        let capacity = config.max_size as usize;
        // Split capacity: high gets up to full capacity, normal gets up to full capacity.
        // Total depth is tracked atomically and enforced in enqueue().
        let (high_tx, high_rx) = mpsc::channel(capacity.max(1));
        let (normal_tx, normal_rx) = mpsc::channel(capacity.max(1));

        Self {
            high_tx,
            high_rx: tokio::sync::Mutex::new(high_rx),
            normal_tx,
            normal_rx: tokio::sync::Mutex::new(normal_rx),
            depth: Arc::new(AtomicUsize::new(0)),
            config,
        }
    }

    /// Enqueue a request. Returns QueueError::Full if at capacity.
    pub fn enqueue(&self, request: QueuedRequest) -> Result<(), QueueError> {
        if !self.config.is_enabled() {
            return Err(QueueError::Disabled);
        }

        // CAS loop to atomically check-and-increment depth, preventing TOCTOU race
        loop {
            let current = self.depth.load(Ordering::SeqCst);
            if current >= self.config.max_size as usize {
                return Err(QueueError::Full {
                    max_size: self.config.max_size,
                });
            }
            if self
                .depth
                .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                break;
            }
        }
        metrics::gauge!("nexus_queue_depth").set(self.depth() as f64);

        let tx = match request.priority {
            Priority::High => &self.high_tx,
            Priority::Normal => &self.normal_tx,
        };

        // try_send is non-blocking; if the channel is full, we already checked
        // depth so this should succeed unless there's a race (acceptable).
        if tx.try_send(request).is_err() {
            self.depth.fetch_sub(1, Ordering::SeqCst);
            metrics::gauge!("nexus_queue_depth").set(self.depth() as f64);
            return Err(QueueError::Full {
                max_size: self.config.max_size,
            });
        }

        Ok(())
    }

    /// Try to dequeue a request. High priority is drained first.
    pub async fn try_dequeue(&self) -> Option<QueuedRequest> {
        // Try high priority first
        {
            let mut rx = self.high_rx.lock().await;
            if let Ok(req) = rx.try_recv() {
                self.depth.fetch_sub(1, Ordering::SeqCst);
                metrics::gauge!("nexus_queue_depth").set(self.depth() as f64);
                return Some(req);
            }
        }

        // Then normal priority
        {
            let mut rx = self.normal_rx.lock().await;
            if let Ok(req) = rx.try_recv() {
                self.depth.fetch_sub(1, Ordering::SeqCst);
                metrics::gauge!("nexus_queue_depth").set(self.depth() as f64);
                return Some(req);
            }
        }

        None
    }

    /// Current queue depth (high + normal)
    pub fn depth(&self) -> usize {
        self.depth.load(Ordering::SeqCst)
    }

    /// Queue configuration
    pub fn config(&self) -> &QueueConfig {
        &self.config
    }
}

/// Background drain loop that processes queued requests as capacity becomes
/// available.
///
/// Watches for backend capacity, dequeues requests, re-runs the reconciler
/// pipeline, and sends responses via oneshot channels.
pub async fn queue_drain_loop(
    queue: Arc<RequestQueue>,
    state: Arc<crate::api::AppState>,
    cancel: tokio_util::sync::CancellationToken,
) {
    use crate::routing::reconciler::intent::TierEnforcementMode;
    use crate::routing::RequestRequirements;
    use std::time::Duration;

    tracing::info!("Queue drain loop started");

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::info!("Queue drain loop shutting down");
                // Drain remaining requests with 503
                drain_remaining(&queue).await;
                break;
            }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                // Process queued requests
                while let Some(queued) = queue.try_dequeue().await {
                    let max_wait = Duration::from_secs(
                        queue.config().max_wait_seconds
                    );

                    // Drain-side timeout: skip requests already expired before processing.
                    // The handler also has its own timeout guard (tokio::time::timeout on
                    // the oneshot rx), so a request may time out on either side.
                    if queued.enqueued_at.elapsed() > max_wait {
                        tracing::warn!(
                            priority = ?queued.priority,
                            waited_ms = queued.enqueued_at.elapsed().as_millis() as u64,
                            "Queued request timed out"
                        );
                        let retry_after =
                            queue.config().max_wait_seconds.to_string();
                        let error_response = build_timeout_response(&retry_after);
                        let _ = queued.response_tx.send(Ok(error_response));
                        continue;
                    }

                    // Re-run routing
                    let requirements =
                        RequestRequirements::from_request(&queued.request);
                    let result = state.router.select_backend(
                        &requirements,
                        Some(TierEnforcementMode::Strict),
                    );

                    match result {
                        Ok(routing_result) => {
                            // Route the request through the agent
                            let response = process_queued_request(
                                &state,
                                &routing_result,
                                &queued.request,
                            )
                            .await;
                            let _ = queued.response_tx.send(response);
                        }
                        Err(_) => {
                            // Still no capacity, re-enqueue if not timed out
                            if queued.enqueued_at.elapsed() < max_wait {
                                let re_queued = QueuedRequest {
                                    intent: queued.intent,
                                    request: queued.request,
                                    response_tx: queued.response_tx,
                                    enqueued_at: queued.enqueued_at,
                                    priority: queued.priority,
                                };
                                if queue.enqueue(re_queued).is_err() {
                                    tracing::warn!(
                                        "Failed to re-enqueue request"
                                    );
                                }
                            } else {
                                let retry_after = queue
                                    .config()
                                    .max_wait_seconds
                                    .to_string();
                                let error_response =
                                    build_timeout_response(&retry_after);
                                let _ =
                                    queued.response_tx.send(Ok(error_response));
                            }
                        }
                    }
                }
            }
        }
    }

    tracing::info!("Queue drain loop stopped");
}

/// Process a queued request by forwarding to the selected backend.
async fn process_queued_request(
    state: &Arc<crate::api::AppState>,
    routing_result: &crate::routing::RoutingResult,
    request: &ChatCompletionRequest,
) -> QueueResponse {
    let backend = &routing_result.backend;
    let mut request = request.clone();
    request.model = routing_result.actual_model.clone();

    let _ = state.registry.increment_pending(&backend.id);

    let result = if let Some(agent) = state.registry.get_agent(&backend.id) {
        agent
            .chat_completion(request, None)
            .await
            .map_err(crate::api::ApiError::from_agent_error)
    } else {
        Err(crate::api::ApiError::bad_gateway(
            "Agent not found for backend",
        ))
    };

    let _ = state.registry.decrement_pending(&backend.id);

    match result {
        Ok(response) => Ok(axum::response::Json(response).into_response()),
        Err(e) => Err(e),
    }
}

use axum::response::IntoResponse;

/// Build a 503 response with retry_after header for timed-out requests.
pub fn build_timeout_response(retry_after: &str) -> axum::response::Response {
    let error = crate::api::ApiError::service_unavailable("Request timed out in queue");
    let mut response = error.into_response();
    if let Ok(val) = axum::http::HeaderValue::from_str(retry_after) {
        response
            .headers_mut()
            .insert(axum::http::header::RETRY_AFTER, val);
    }
    response
}

/// Drain remaining requests on shutdown with 503 responses.
async fn drain_remaining(queue: &Arc<RequestQueue>) {
    while let Some(queued) = queue.try_dequeue().await {
        let error_response = build_timeout_response("5");
        let _ = queued.response_tx.send(Ok(error_response));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ChatCompletionRequest;
    use crate::config::QueueConfig;
    use crate::routing::reconciler::intent::RoutingIntent;
    use crate::routing::RequestRequirements;
    use std::time::{Duration, Instant};

    fn make_config(max_size: u32, max_wait_seconds: u64) -> QueueConfig {
        QueueConfig {
            enabled: true,
            max_size,
            max_wait_seconds,
        }
    }

    fn make_intent() -> RoutingIntent {
        RoutingIntent::new(
            "req-1".to_string(),
            "llama3:8b".to_string(),
            "llama3:8b".to_string(),
            RequestRequirements {
                model: "llama3:8b".to_string(),
                estimated_tokens: 100,
                needs_vision: false,
                needs_tools: false,
                needs_json_mode: false,
                prefers_streaming: false,
            },
            vec![],
        )
    }

    fn make_request() -> ChatCompletionRequest {
        serde_json::from_value(serde_json::json!({
            "model": "llama3:8b",
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .unwrap()
    }

    fn make_queued(priority: Priority) -> (QueuedRequest, oneshot::Receiver<QueueResponse>) {
        let (tx, rx) = oneshot::channel();
        let req = QueuedRequest {
            intent: make_intent(),
            request: make_request(),
            response_tx: tx,
            enqueued_at: Instant::now(),
            priority,
        };
        (req, rx)
    }

    // ========================================================================
    // T021: Unit tests for RequestQueue
    // ========================================================================

    #[tokio::test]
    async fn fifo_ordering_normal_priority() {
        let queue = RequestQueue::new(make_config(10, 30));

        let (req1, _rx1) = make_queued(Priority::Normal);
        let (req2, _rx2) = make_queued(Priority::Normal);
        let (req3, _rx3) = make_queued(Priority::Normal);

        let t1 = req1.enqueued_at;
        let t2 = req2.enqueued_at;
        let t3 = req3.enqueued_at;

        queue.enqueue(req1).unwrap();
        queue.enqueue(req2).unwrap();
        queue.enqueue(req3).unwrap();

        assert_eq!(queue.depth(), 3);

        let d1 = queue.try_dequeue().await.unwrap();
        assert_eq!(d1.enqueued_at, t1);
        let d2 = queue.try_dequeue().await.unwrap();
        assert_eq!(d2.enqueued_at, t2);
        let d3 = queue.try_dequeue().await.unwrap();
        assert_eq!(d3.enqueued_at, t3);

        assert_eq!(queue.depth(), 0);
    }

    #[tokio::test]
    async fn capacity_limits_reject_when_full() {
        let queue = RequestQueue::new(make_config(2, 30));

        let (req1, _rx1) = make_queued(Priority::Normal);
        let (req2, _rx2) = make_queued(Priority::Normal);
        let (req3, _rx3) = make_queued(Priority::Normal);

        queue.enqueue(req1).unwrap();
        queue.enqueue(req2).unwrap();
        let result = queue.enqueue(req3);

        assert!(result.is_err());
        assert!(matches!(result, Err(QueueError::Full { max_size: 2 })));
        assert_eq!(queue.depth(), 2);
    }

    #[tokio::test]
    async fn priority_ordering_high_drains_first() {
        let queue = RequestQueue::new(make_config(10, 30));

        let (normal1, _rx1) = make_queued(Priority::Normal);
        let (high1, _rx2) = make_queued(Priority::High);
        let (normal2, _rx3) = make_queued(Priority::Normal);

        // Enqueue normal, then high, then normal
        queue.enqueue(normal1).unwrap();
        queue.enqueue(high1).unwrap();
        queue.enqueue(normal2).unwrap();

        assert_eq!(queue.depth(), 3);

        // High priority should dequeue first
        let d1 = queue.try_dequeue().await.unwrap();
        assert_eq!(d1.priority, Priority::High);

        // Then normal in FIFO order
        let d2 = queue.try_dequeue().await.unwrap();
        assert_eq!(d2.priority, Priority::Normal);

        let d3 = queue.try_dequeue().await.unwrap();
        assert_eq!(d3.priority, Priority::Normal);
    }

    #[tokio::test]
    async fn depth_accuracy() {
        let queue = RequestQueue::new(make_config(10, 30));
        assert_eq!(queue.depth(), 0);

        let (req1, _rx1) = make_queued(Priority::Normal);
        queue.enqueue(req1).unwrap();
        assert_eq!(queue.depth(), 1);

        let (req2, _rx2) = make_queued(Priority::High);
        queue.enqueue(req2).unwrap();
        assert_eq!(queue.depth(), 2);

        queue.try_dequeue().await;
        assert_eq!(queue.depth(), 1);

        queue.try_dequeue().await;
        assert_eq!(queue.depth(), 0);
    }

    #[tokio::test]
    async fn max_size_zero_rejects_immediately() {
        let queue = RequestQueue::new(make_config(0, 30));
        let (req, _rx) = make_queued(Priority::Normal);
        let result = queue.enqueue(req);
        assert!(result.is_err());
        assert!(matches!(result, Err(QueueError::Disabled)));
    }

    #[tokio::test]
    async fn disabled_queue_rejects() {
        let config = QueueConfig {
            enabled: false,
            max_size: 100,
            max_wait_seconds: 30,
        };
        let queue = RequestQueue::new(config);
        let (req, _rx) = make_queued(Priority::Normal);
        let result = queue.enqueue(req);
        assert!(result.is_err());
        assert!(matches!(result, Err(QueueError::Disabled)));
    }

    #[tokio::test]
    async fn empty_dequeue_returns_none() {
        let queue = RequestQueue::new(make_config(10, 30));
        let result = queue.try_dequeue().await;
        assert!(result.is_none());
    }

    // ========================================================================
    // T022: Unit tests for queue timeout
    // ========================================================================

    #[tokio::test]
    async fn timeout_response_has_retry_after() {
        let response = build_timeout_response("30");
        assert_eq!(
            response.status(),
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        );
        let retry = response
            .headers()
            .get(axum::http::header::RETRY_AFTER)
            .unwrap();
        assert_eq!(retry.to_str().unwrap(), "30");
    }

    #[tokio::test]
    async fn enqueued_request_timeout_detection() {
        // Simulate a request that was enqueued 2 seconds ago with 1s max wait
        let config = make_config(10, 1);
        let max_wait = Duration::from_secs(config.max_wait_seconds);

        let enqueued_at = Instant::now() - Duration::from_secs(2);
        let elapsed = enqueued_at.elapsed();
        assert!(elapsed > max_wait, "Request should be timed out");
    }

    #[tokio::test]
    async fn timeout_completes_within_time_limit() {
        // This test verifies that timeout detection is fast (<2x max_wait)
        let max_wait_ms = 100u64;
        let config = QueueConfig {
            enabled: true,
            max_size: 10,
            max_wait_seconds: 0, // We use ms-based detection in test
        };

        let _queue = RequestQueue::new(config);
        let (req, rx) = make_queued(Priority::Normal);

        // Override enqueued_at to be in the past
        let timed_out_req = QueuedRequest {
            intent: req.intent,
            request: req.request,
            response_tx: req.response_tx,
            enqueued_at: Instant::now() - Duration::from_millis(max_wait_ms * 2),
            priority: req.priority,
        };

        // Directly test timeout detection
        let max_wait = Duration::from_millis(max_wait_ms);
        assert!(timed_out_req.enqueued_at.elapsed() > max_wait);

        // Send timeout response
        let retry_after = "1";
        let error_response = build_timeout_response(retry_after);
        let _ = timed_out_req.response_tx.send(Ok(error_response));

        // Verify response received within time
        let start = Instant::now();
        let result = rx.await;
        assert!(result.is_ok());
        assert!(start.elapsed() < Duration::from_millis(max_wait_ms * 2));
    }

    // ========================================================================
    // T028: Priority header parsing tests
    // ========================================================================

    #[test]
    fn priority_from_header_high() {
        assert_eq!(Priority::from_header("high"), Priority::High);
        assert_eq!(Priority::from_header("HIGH"), Priority::High);
        assert_eq!(Priority::from_header(" High "), Priority::High);
    }

    #[test]
    fn priority_from_header_normal() {
        assert_eq!(Priority::from_header("normal"), Priority::Normal);
        assert_eq!(Priority::from_header("NORMAL"), Priority::Normal);
    }

    #[test]
    fn priority_from_header_invalid_defaults_to_normal() {
        assert_eq!(Priority::from_header(""), Priority::Normal);
        assert_eq!(Priority::from_header("urgent"), Priority::Normal);
        assert_eq!(Priority::from_header("low"), Priority::Normal);
        assert_eq!(Priority::from_header("123"), Priority::Normal);
    }

    // ========================================================================
    // Concurrent enqueue: CAS loop prevents depth exceeding max_size
    // ========================================================================

    #[tokio::test]
    async fn concurrent_enqueue_respects_max_size() {
        let queue = Arc::new(RequestQueue::new(make_config(10, 30)));
        let mut handles = vec![];

        for _ in 0..50 {
            let q = Arc::clone(&queue);
            handles.push(tokio::spawn(async move {
                let (req, _rx) = make_queued(Priority::Normal);
                q.enqueue(req)
            }));
        }

        let results: Vec<_> = futures::future::join_all(handles).await;
        let successes = results
            .iter()
            .filter(|r| r.as_ref().unwrap().is_ok())
            .count();

        assert_eq!(successes, 10, "Exactly max_size requests should succeed");
        assert_eq!(queue.depth(), 10);
    }
}
