# Research: API Gateway (F04)

**Date**: 2026-02-03
**Phase**: Implemented (v0.1)

This document captures the technical decisions made during implementation of the API Gateway — the HTTP server that provides OpenAI-compatible endpoints for chat completions, model listing, and health checks.

## Research Questions & Findings

### 1. Web Framework Selection: Axum

**Question**: Which Rust web framework should Nexus use for its HTTP API?

**Decision**: `axum` (v0.7) with `tower-http` middleware.

**Rationale**:
- Built on `tokio` and `hyper` — Nexus already depends on `tokio` for async runtime, so axum shares the same foundation
- Type-safe extractors (`State`, `Json`, `HeaderMap`) catch request parsing errors at compile time
- Native SSE support via `axum::response::sse::Sse` — critical for streaming chat completions
- WebSocket support via `axum::extract::ws` — used by the dashboard (F10)
- `tower` middleware ecosystem provides request body limits, CORS, timeouts, and tracing out of the box
- State sharing via `Arc<AppState>` with `.with_state()` — no global state, no macros

**Alternatives Considered**:
- `actix-web`: Rejected because it uses its own runtime (`actix-rt`), conflicting with Nexus's `tokio` usage. Mixing runtimes prevents sharing async resources (like `reqwest::Client`).
- `warp`: Rejected because its filter-based composition becomes hard to read with complex routing. Axum's router-based approach is more intuitive for REST APIs.
- `rocket`: Rejected because v0.5 uses macros heavily. Axum's derive-free approach provides better IDE support and compile error messages.

**Implementation**:
```rust
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(completions::handle))
        .route("/v1/models", get(models::handle))
        .route("/health", get(health::handle))
        .route("/metrics", get(metrics_handler))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_SIZE))
        .with_state(state)
}
```

**References**:
- https://docs.rs/axum/0.7/axum/
- https://docs.rs/tower-http/0.5/tower_http/

---

### 2. Streaming SSE Implementation

**Question**: How should we implement streaming chat completions?

**Decision**: Use `async_stream::stream!` to create an SSE stream that proxies chunks from the backend via `reqwest`'s byte stream.

**Rationale**:
- OpenAI's streaming protocol uses Server-Sent Events (SSE) with `data: {json}\n\n` frames
- `async_stream::stream!` provides generator-like syntax — much cleaner than manual `Poll` implementation
- The stream buffers incoming bytes and splits on newlines to handle partial SSE frames
- Axum's `Sse<S>` wrapper handles SSE framing, content-type headers, and keep-alive automatically

**Alternatives Considered**:
- Manual `Pin<Box<dyn Stream>>`: Rejected because hand-implementing `poll_next` is error-prone. `async_stream` achieves the same result with imperative syntax.
- Buffering the entire response: Rejected because it defeats streaming. Time-to-first-token would increase from milliseconds to full generation time.
- `hyper::Body` passthrough: Rejected because Nexus needs to parse SSE frames to detect `[DONE]`, handle errors, and inject metadata headers.

**Implementation**:
```rust
fn create_sse_stream(...) -> impl Stream<Item = Result<Event, Infallible>> {
    async_stream::stream! {
        let mut byte_stream = response.bytes_stream();
        let mut buffer = String::new();
        while let Some(chunk_result) = byte_stream.next().await {
            // Buffer bytes, split on newlines, parse SSE "data: " prefix
            // Forward each data payload as an Event
        }
    }
}
```

---

### 3. OpenAI-Compatible Error Format

**Question**: How should API errors be structured?

**Decision**: Match OpenAI's error format exactly with `error.message`, `error.type`, `error.param`, and `error.code`.

**Rationale**:
- Nexus is a drop-in replacement for OpenAI's API — clients expect the same error format
- Structured errors with `code` field enable programmatic error handling (e.g., "model_not_found" vs "service_unavailable")
- Status code is derived from the `code` field via `status_code()` method — single source of truth
- `IntoResponse` implementation allows returning `ApiError` directly from handlers
- `model_not_found` includes available models as a hint — actionable errors over opaque ones

**Alternatives Considered**:
- Custom error format: Rejected because it would break existing OpenAI client libraries. Compatibility is a core architectural principle.
- `anyhow::Error` with custom `IntoResponse`: Rejected because it loses type safety. Each error variant needs a specific HTTP status code — enum dispatch is clearer.
- `Problem Details` (RFC 7807): Rejected because OpenAI doesn't use it. Following a different standard would confuse clients.

**Implementation**:
```rust
#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    pub error: ApiErrorBody,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiErrorBody {
    pub message: String,
    pub r#type: String,
    pub param: Option<String>,
    pub code: Option<String>,
}

impl ApiError {
    pub fn model_not_found(model: &str, available: &[String]) -> Self {
        let hint = if available.is_empty() {
            "No models available".to_string()
        } else {
            format!("Available: {}", available.join(", "))
        };
        Self {
            error: ApiErrorBody {
                message: format!("Model '{}' not found. {}", model, hint),
                r#type: "invalid_request_error".to_string(),
                param: Some("model".to_string()),
                code: Some("model_not_found".to_string()),
            },
        }
    }

    fn status_code(&self) -> StatusCode {
        match self.error.code.as_deref() {
            Some("model_not_found") => StatusCode::NOT_FOUND,
            Some("bad_gateway") => StatusCode::BAD_GATEWAY,
            Some("gateway_timeout") => StatusCode::GATEWAY_TIMEOUT,
            Some("service_unavailable") => StatusCode::SERVICE_UNAVAILABLE,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
```

---

### 4. Request Timeout and Retry Strategy

**Question**: How should we handle backend timeouts and retries?

**Decision**: Configurable timeout via `request_timeout_seconds` (default: 300s) on `reqwest::Client`, with retry loop up to `max_retries` (default: 2).

**Rationale**:
- LLM inference is slow — a 70B model generating 2000 tokens can take 60-120s. The 300s default accommodates large generations
- Timeout is set on the `reqwest::Client` at construction — consistent across all requests
- Retries are against the same backend (fallback to other backends is router logic)
- Each retry increments/decrements `pending_requests` atomically for accurate routing metrics

**Alternatives Considered**:
- Per-request timeout via `tower::timeout::Timeout`: Rejected because it kills the connection server-side while the backend continues generating. Client-side timeout via `reqwest` is cleaner.
- Exponential backoff: Rejected because LLM backends don't benefit from backoff — they're either processing or down. Immediate retry is more responsive.
- No retries: Rejected because transient network errors are common in home lab environments. A single retry catches most transient failures.

**Implementation**:
```rust
// Client-level timeout
let http_client = reqwest::Client::builder()
    .timeout(Duration::from_secs(config.server.request_timeout_seconds))
    .pool_max_idle_per_host(10)
    .build()?;

// Retry loop in handler
for attempt in 0..=max_retries {
    state.registry.increment_pending(&backend.id)?;
    match proxy_request(&state, backend, &headers, &request).await {
        Ok(response) => { /* decrement, return success */ }
        Err(e) => { /* decrement, store last_error */ }
    }
}
```

---

### 5. Reqwest Connection Pooling

**Question**: How should we manage HTTP connections to backends?

**Decision**: Single `reqwest::Client` shared across all handlers via `AppState`, with `pool_max_idle_per_host(10)`.

**Rationale**:
- `reqwest::Client` maintains an internal connection pool — reusing connections avoids TCP/TLS overhead
- Thread-safe (`Arc`-based internally), shared across all `tokio` tasks
- `pool_max_idle_per_host(10)` keeps up to 10 idle connections per backend — sufficient without wasting file descriptors

**Alternatives Considered**:
- `hyper::Client` directly: Rejected because `reqwest` wraps `hyper` with simpler API, automatic JSON serialization, and built-in redirect/timeout handling.
- Per-backend `reqwest::Client`: Rejected because it prevents connection sharing and increases memory usage.
- No pooling: Rejected because TCP+TLS handshake adds 50-200ms per request. Connection reuse drops this to near-zero.

**Implementation**:
```rust
pub struct AppState {
    pub http_client: reqwest::Client,  // Shared across all handlers
    // ...
}
```

---

### 6. Graceful Shutdown Architecture

**Question**: How should the server handle shutdown signals?

**Decision**: `CancellationToken` propagated to all background tasks, with `axum::serve().with_graceful_shutdown()`.

**Rationale**:
- Graceful shutdown: stop accepting new connections, finish in-flight requests, stop background tasks, then exit
- `CancellationToken` (same as F02) is shared with health checker and mDNS discovery for coordinated shutdown
- Signal handling covers both `SIGINT` (Ctrl+C) and `SIGTERM` (container orchestrator stop)
- Background task `JoinHandle`s are awaited after server stops for clean exit

**Alternatives Considered**:
- `tokio::signal::ctrl_c()` only: Rejected because containers send `SIGTERM`, not `SIGINT`.
- Global `AtomicBool` shutdown flag: Rejected because it doesn't integrate with async. `CancellationToken` provides future-based notification via `.cancelled()`.
- `tokio::sync::broadcast` for shutdown: Rejected because broadcast requires receivers before send. `CancellationToken` is simpler for "stop everything".

**Implementation**:
```rust
async fn shutdown_signal(cancel_token: CancellationToken) {
    let ctrl_c = async { tokio::signal::ctrl_c().await.expect("...") };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(SignalKind::terminate())
            .expect("...").recv().await;
    };

    tokio::select! {
        _ = ctrl_c => { tracing::info!("Received SIGINT"); }
        _ = terminate => { tracing::info!("Received SIGTERM"); }
    }
    cancel_token.cancel();
}

// In run_serve():
axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal(cancel_token.clone()))
    .await?;

// Cleanup
if let Some(handle) = health_handle { handle.await?; }
if let Some(handle) = discovery_handle { handle.await?; }
```

---

### 7. Request Body Size Limit

**Question**: How should we protect against oversized request payloads?

**Decision**: `tower_http::limit::RequestBodyLimitLayer` with a 10 MB maximum.

**Rationale**:
- Chat completion requests with vision (base64 images) can be large, but 10 MB is a reasonable upper bound
- Tower middleware applies the limit before the handler runs — no wasted parsing work
- Returns 413 Payload Too Large automatically, matching HTTP semantics
- Configurable at the middleware level without touching handler code

**Alternatives Considered**:
- No limit: Rejected because a malicious or buggy client could exhaust server memory with a single request.
- `axum::body::Bytes` with manual length check: Rejected because it requires reading the entire body first, which defeats the purpose of the limit.
- Lower limit (1 MB): Rejected because multimodal requests with images commonly exceed 1 MB. 10 MB accommodates several high-resolution images.

**Implementation**:
```rust
const MAX_BODY_SIZE: usize = 10 * 1024 * 1024; // 10 MB

Router::new()
    .route("/v1/chat/completions", post(completions::handle))
    .layer(RequestBodyLimitLayer::new(MAX_BODY_SIZE))
    .with_state(state)
```

---

### 8. Multimodal Request Type Design

**Question**: How should we handle both text-only and vision (multimodal) chat requests?

**Decision**: `MessageContent` enum with `#[serde(untagged)]` to deserialize either `String` or `Vec<ContentPart>`.

**Rationale**:
- OpenAI's API accepts `content` as either a plain string or an array of content parts (text + images)
- `#[serde(untagged)]` tries each variant in order — text first (most common), then parts
- The router inspects `ContentPart` for `image_url` types to determine vision capability requirements
- `#[serde(flatten)]` on `extra` fields preserves unknown fields for passthrough to backends

**Alternatives Considered**:
- Always deserialize as `Vec<ContentPart>`: Rejected because it forces text-only messages into a single-element array, making the common case verbose.
- `serde_json::Value` for content field: Rejected because it loses type safety. The router needs to inspect content parts for capability detection — raw JSON would require repeated parsing.

**Implementation**:
```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text { content: String },
    Parts { content: Vec<ContentPart> },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContentPart {
    #[serde(rename = "type")]
    pub part_type: String,
    pub text: Option<String>,
    pub image_url: Option<ImageUrl>,
}
```

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Streaming proxy drops bytes on network interruption | Medium | `buffer` collects partial frames across TCP segments. Stream errors break the loop and decrement pending. |
| 300s default timeout blocks resources on stalled backends | Medium | `pending_requests` atomic ensures the router deprioritizes loaded backends. Health checker will mark truly stalled backends unhealthy. |
| `#[serde(untagged)]` has poor error messages on parse failure | Low | Request parsing errors return 400 with the serde message. Acceptable since clients should send valid OpenAI payloads. |
| Connection pool exhaustion under high concurrency | Medium | `pool_max_idle_per_host(10)` is per backend. New connections are created beyond this limit; they're just not pooled when idle. |

---

## References

- [axum documentation](https://docs.rs/axum/0.7/axum/)
- [tower-http documentation](https://docs.rs/tower-http/0.5/tower_http/)
- [reqwest documentation](https://docs.rs/reqwest/0.12/reqwest/)
- [OpenAI Chat Completions API](https://platform.openai.com/docs/api-reference/chat)
- [async-stream documentation](https://docs.rs/async-stream/0.3/async_stream/)
- LEARNINGS.md: "Graceful Shutdown Pattern"
