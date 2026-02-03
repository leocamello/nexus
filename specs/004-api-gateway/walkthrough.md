# Core API Gateway - Code Walkthrough

**Feature**: F01 - Core API Gateway  
**Audience**: Junior developers joining the project  
**Last Updated**: 2026-02-03

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: mod.rs - Router & AppState](#file-1-modrs---router--appstate)
4. [File 2: types.rs - OpenAI-Compatible Types](#file-2-typesrs---openai-compatible-types)
5. [File 3: completions.rs - The Main Handler](#file-3-completionsrs---the-main-handler)
6. [File 4: models.rs - Model Listing](#file-4-modelsrs---model-listing)
7. [File 5: health.rs - Health Endpoint](#file-5-healthrs---health-endpoint)
8. [Understanding the Tests](#understanding-the-tests)
9. [Request Flow Diagram](#request-flow-diagram)
10. [Key Patterns](#key-patterns)

---

## The Big Picture

The API Gateway is the **front door** to Nexus. It receives HTTP requests from clients (like Claude Code, Continue.dev, or curl) and routes them to the appropriate backend LLM server.

### What It Does

1. **Accepts OpenAI-compatible requests** - Same API format used by OpenAI's GPT models
2. **Routes to healthy backends** - Uses the Registry to find backends with the requested model
3. **Handles failures gracefully** - Retries with other backends if one fails
4. **Streams responses** - Supports Server-Sent Events (SSE) for real-time streaming

### How It Fits in Nexus

```
┌─────────────────────────────────────────────────────────────────────┐
│                             Nexus                                   │
│                                                                     │
│  ┌────────────────────────────────────────────────────────────┐    │
│  │                   API Gateway (you are here!)              │    │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │    │
│  │  │ /v1/chat/   │  │ /v1/models  │  │ /health     │        │    │
│  │  │ completions │  │             │  │             │        │    │
│  │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘        │    │
│  └─────────┼────────────────┼────────────────┼───────────────┘    │
│            │                │                │                     │
│            ▼                ▼                ▼                     │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │                    Backend Registry                          │  │
│  │            (Source of truth for all backends)                │  │
│  └─────────────────────────────────────────────────────────────┘  │
│                              │                                     │
│            ┌─────────────────┼─────────────────┐                  │
│            ▼                 ▼                 ▼                  │
│       ┌────────┐       ┌────────┐        ┌────────┐              │
│       │ Ollama │       │  vLLM  │        │ OpenAI │              │
│       │ Server │       │ Server │        │  API   │              │
│       └────────┘       └────────┘        └────────┘              │
└─────────────────────────────────────────────────────────────────────┘
```

---

## File Structure

```
src/api/
├── mod.rs          # Router setup, AppState, body limit layer
├── types.rs        # Request/response types (OpenAI format)
├── completions.rs  # POST /v1/chat/completions handler
├── models.rs       # GET /v1/models handler
└── health.rs       # GET /health handler

tests/
├── api_contract.rs     # OpenAI format compliance tests
├── api_integration.rs  # Router integration tests
├── api_streaming.rs    # SSE streaming tests
└── api_edge_cases.rs   # Edge cases, concurrency, performance
```

---

## File 1: mod.rs - Router & AppState

This file is the **entry point** for the API module. It sets up the HTTP router and shared state.

### AppState - Shared Application State

```rust
pub struct AppState {
    pub registry: Arc<Registry>,      // Shared backend registry
    pub config: Arc<NexusConfig>,     // Configuration
    pub http_client: reqwest::Client, // HTTP client for backend calls
}

impl AppState {
    pub fn new(registry: Arc<Registry>, config: Arc<NexusConfig>) -> Self {
        let timeout_secs = config.server.request_timeout_seconds;

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .pool_max_idle_per_host(10)  // Connection pooling
            .build()
            .expect("Failed to create HTTP client");

        Self { registry, config, http_client }
    }
}
```

**Key Points:**
- `Arc<T>` means shared ownership across threads (Atomic Reference Counted)
- `reqwest::Client` is reused for connection pooling - don't create a new one per request!
- `.pool_max_idle_per_host(10)` keeps 10 connections warm per backend

### Router Setup

```rust
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(completions::handle))
        .route("/v1/models", get(models::handle))
        .route("/health", get(health::handle))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_SIZE))  // 10MB limit
        .with_state(state)
}
```

**What's happening:**
1. Create a new Axum router
2. Define routes with HTTP method + path + handler function
3. Add middleware layer for request body size limit (prevents memory exhaustion)
4. Attach shared state that all handlers can access

---

## File 2: types.rs - OpenAI-Compatible Types

This file defines the **data structures** that match OpenAI's API format. This is critical for compatibility with existing tools.

### Request Type

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,               // "llama3:70b"
    pub messages: Vec<ChatMessage>,  // Conversation history
    #[serde(default)]                // Default is false
    pub stream: bool,                // Enable SSE streaming?
    
    // Optional parameters
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    // ...
    
    // Pass through unknown fields to backend
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}
```

**Why `#[serde(flatten)]`?**

Different backends support different parameters. Instead of defining every possible field, we capture unknown fields in `extra` and pass them through to the backend. This makes Nexus transparent to backend-specific features.

### Response Type

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,              // "chatcmpl-abc123"
    pub object: String,          // "chat.completion"
    pub created: i64,            // Unix timestamp
    pub model: String,           // Model that was used
    pub choices: Vec<Choice>,    // The actual responses
    pub usage: Option<Usage>,    // Token counts (if provided)
}
```

### Error Type with Status Codes

```rust
impl ApiError {
    pub fn bad_request(message: &str) -> Self { /* 400 */ }
    pub fn model_not_found(model: &str, available: &[String]) -> Self { /* 404 */ }
    pub fn bad_gateway(message: &str) -> Self { /* 502 */ }
    pub fn gateway_timeout() -> Self { /* 504 */ }
    pub fn service_unavailable(message: &str) -> Self { /* 503 */ }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status_code(), Json(self)).into_response()
    }
}
```

**Key Concept:** `IntoResponse` trait lets Axum automatically convert our error type into an HTTP response with the correct status code and JSON body.

---

## File 3: completions.rs - The Main Handler

This is the **heart of the API Gateway**. It handles chat completion requests, including both streaming and non-streaming.

### The Main Handler

```rust
pub async fn handle(
    State(state): State<Arc<AppState>>,  // Injected shared state
    headers: HeaderMap,                   // HTTP headers
    Json(request): Json<ChatCompletionRequest>,  // Parsed request body
) -> Result<Response, ApiError> {
    // Route based on stream flag
    if request.stream {
        return handle_streaming(state, headers, request).await;
    }
    
    // Non-streaming logic...
}
```

**Axum Magic:** 
- `State(state)` - Axum extracts our AppState from the router
- `Json(request)` - Axum parses the JSON body into our type
- `Result<Response, ApiError>` - Can return success or error

### Finding Backends for a Model

```rust
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
```

**Why filter by health?**

A backend might have the model, but if it's unhealthy (e.g., down or overloaded), we shouldn't send requests there.

### Retry Logic

```rust
let max_retries = state.config.routing.max_retries as usize;
let mut last_error = None;

for (attempt, backend) in healthy.iter().take(max_retries + 1).enumerate() {
    info!(backend_id = %backend.id, attempt, "Trying backend");

    // Track pending requests for load balancing
    let _ = state.registry.increment_pending(&backend.id);

    match proxy_request(&state, backend, &headers, &request).await {
        Ok(response) => {
            let _ = state.registry.decrement_pending(&backend.id);
            return Ok(Json(response).into_response());
        }
        Err(e) => {
            let _ = state.registry.decrement_pending(&backend.id);
            warn!(backend_id = %backend.id, error = %e.error.message, "Failed");
            last_error = Some(e);
        }
    }
}

// All retries failed
Err(last_error.unwrap_or_else(|| ApiError::bad_gateway("All backends failed")))
```

**Key Points:**
- `take(max_retries + 1)` - Try first backend + N retries
- `increment/decrement_pending` - Track in-flight requests for each backend
- `last_error` - Remember the last error to return if all fail

### Streaming with SSE

```rust
fn create_sse_stream(
    state: Arc<AppState>,
    backend: Backend,
    headers: HeaderMap,
    request: ChatCompletionRequest,
) -> impl Stream<Item = Result<Event, Infallible>> {
    async_stream::stream! {
        // Make request to backend
        let response = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                yield Ok(Event::default().data(/* error chunk */));
                yield Ok(Event::default().data("[DONE]"));
                return;
            }
        };

        // Stream response chunks
        let mut byte_stream = response.bytes_stream();
        
        while let Some(chunk_result) = byte_stream.next().await {
            // Parse SSE lines and forward them
            if let Some(data) = line.strip_prefix("data: ") {
                yield Ok(Event::default().data(data));
            }
        }
    }
}
```

**What's SSE (Server-Sent Events)?**

A simple streaming protocol. The server sends lines like:
```
data: {"content": "Hello"}

data: {"content": " world"}

data: [DONE]
```

Each `data:` line is a chunk. The client receives them in real-time as they're generated.

---

## File 4: models.rs - Model Listing

Returns a list of all available models from all healthy backends.

```rust
pub async fn handle(
    State(state): State<Arc<AppState>>,
) -> Json<ModelsResponse> {
    let backends = state.registry.get_healthy_backends();
    let mut models_map = std::collections::HashMap::new();

    // Collect and deduplicate models
    for backend in backends {
        for model in &backend.models {
            models_map.entry(model.id.clone()).or_insert_with(|| {
                ModelObject {
                    id: model.id.clone(),
                    object: "model".to_string(),
                    created: chrono::Utc::now().timestamp(),
                    owned_by: "nexus".to_string(),
                    // Include capabilities
                    context_length: Some(model.context_length),
                    capabilities: Some(ModelCapabilities {
                        vision: model.supports_vision,
                        tools: model.supports_tools,
                        json_mode: model.supports_json_mode,
                    }),
                }
            });
        }
    }

    Json(ModelsResponse {
        object: "list".to_string(),
        data: models_map.into_values().collect(),
    })
}
```

**Why deduplicate?**

If two backends both have "llama3:70b", we only show it once in the list. The `HashMap` handles this automatically - same key = same model.

---

## File 5: health.rs - Health Endpoint

Returns system status for monitoring and load balancers.

```rust
pub async fn handle(
    State(state): State<Arc<AppState>>,
) -> Json<HealthResponse> {
    let backends = state.registry.get_all_backends();
    
    let healthy_count = backends.iter().filter(|b| b.status == BackendStatus::Healthy).count();
    let unhealthy_count = backends.iter().filter(|b| b.status == BackendStatus::Unhealthy).count();
    
    let status = match (healthy_count, backends.len()) {
        (h, t) if h == t && t > 0 => "healthy",   // All healthy
        (h, _) if h > 0 => "degraded",            // Some healthy
        _ => "unhealthy",                          // None healthy
    };

    Json(HealthResponse {
        status: status.to_string(),
        uptime_seconds: 0,  // TODO: Track actual uptime
        backends: BackendCounts {
            total: backends.len(),
            healthy: healthy_count,
            unhealthy: unhealthy_count,
        },
        models: count_unique_models(&backends),
    })
}
```

**Status Logic:**
- `healthy` - All backends are healthy AND at least one exists
- `degraded` - Some backends are healthy, some are not
- `unhealthy` - No healthy backends

---

## Understanding the Tests

### Test Categories

| File | Tests | Purpose |
|------|-------|---------|
| `api_contract.rs` | 8 | Verify OpenAI format compliance |
| `api_integration.rs` | 5 | Router setup and basic routing |
| `api_streaming.rs` | 8 | SSE streaming functionality |
| `api_edge_cases.rs` | 14 | Edge cases, concurrency, performance |

### Example: Contract Test

```rust
#[test]
fn test_contract_completions_response_format() {
    // OpenAI's exact response format
    let json = json!({
        "id": "chatcmpl-abc123",
        "object": "chat.completion",
        "created": 1699999999,
        "model": "llama3:70b",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": "Hello!"},
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    });

    // Our type must deserialize this correctly
    let response: ChatCompletionResponse = serde_json::from_value(json).unwrap();
    
    assert_eq!(response.object, "chat.completion");
    assert_eq!(response.choices.len(), 1);
}
```

**Why Contract Tests?**

They ensure our types match the OpenAI specification exactly. If we change a field name or type, these tests fail immediately.

### Example: Streaming Test with Mock Backend

```rust
#[tokio::test]
async fn test_streaming_sends_chunks() {
    let mock_server = MockServer::start().await;

    // Mock backend returns SSE response
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("data: {...chunk1...}\n\ndata: {...chunk2...}\n\ndata: [DONE]\n\n")
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let (mut app, _) = create_test_app_with_mock(&mock_server).await;

    // Make streaming request
    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .body(Body::from(r#"{"model": "test", "messages": [], "stream": true}"#))
        .unwrap();

    let response = app.call(request).await.unwrap();
    let body = body_to_string(response.into_body()).await;

    // Verify we received multiple chunks
    let data_lines: Vec<_> = body.lines().filter(|l| l.starts_with("data: ")).collect();
    assert!(data_lines.len() >= 2);
}
```

**Key Pattern: Wiremock**

We use `wiremock` to create a fake HTTP server that returns canned responses. This lets us test our code without needing real LLM backends.

---

## Request Flow Diagram

```
Client Request                              Nexus API Gateway
     │                                             │
     │  POST /v1/chat/completions                  │
     │  {model: "llama3:70b", messages: [...]}     │
     ▼                                             ▼
┌─────────────────────────────────────────────────────────────────┐
│  1. Parse JSON body into ChatCompletionRequest                  │
│     - Validates required fields (model, messages)               │
│     - Checks body size limit (10MB)                             │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  2. Query Registry for backends with this model                 │
│     - registry.get_backends_for_model("llama3:70b")             │
│     - Filter to healthy backends only                           │
└─────────────────────────────────────────────────────────────────┘
                              │
                    ┌─────────┴─────────┐
                    │                   │
              Found backends?      No backends
                    │                   │
                    ▼                   ▼
┌──────────────────────────┐  ┌──────────────────────────┐
│  3. Try backends with    │  │  Return 404 Not Found    │
│     retry logic          │  │  "Model not found.       │
│                          │  │   Available: ..."        │
│  - Increment pending     │  └──────────────────────────┘
│  - Forward request       │
│  - On success: return    │
│  - On failure: try next  │
└──────────────────────────┘
           │
           ▼
┌──────────────────────────────────────────────────────────────┐
│  4. Return response to client                                 │
│     - Non-streaming: JSON response body                       │
│     - Streaming: SSE with data: lines                         │
└──────────────────────────────────────────────────────────────┘
```

---

## Key Patterns

### Pattern 1: Axum Extractors

```rust
pub async fn handle(
    State(state): State<Arc<AppState>>,  // Extract shared state
    headers: HeaderMap,                   // Extract headers
    Json(request): Json<ChatCompletionRequest>,  // Extract + parse body
) -> Result<Response, ApiError> { ... }
```

Axum automatically extracts these from the HTTP request. The order matters - extractors that consume the body must come last.

### Pattern 2: Error Conversion with `IntoResponse`

```rust
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status_code(), Json(self)).into_response()
    }
}
```

This lets us return `Result<Response, ApiError>` and Axum handles the conversion to proper HTTP responses.

### Pattern 3: Async Streams for SSE

```rust
fn create_sse_stream(...) -> impl Stream<Item = Result<Event, Infallible>> {
    async_stream::stream! {
        // Each `yield` sends a chunk to the client
        yield Ok(Event::default().data("chunk1"));
        yield Ok(Event::default().data("chunk2"));
        yield Ok(Event::default().data("[DONE]"));
    }
}
```

`async_stream::stream!` creates an async iterator that yields SSE events. Axum's `Sse` wrapper converts this to proper HTTP streaming.

### Pattern 4: Request Tracking for Load Balancing

```rust
// Before request
state.registry.increment_pending(&backend.id);

// Make request...

// After request (success or failure)
state.registry.decrement_pending(&backend.id);
```

This tracks how many in-flight requests each backend has. The Router can use this for load balancing (prefer backends with fewer pending requests).

---

## Common Debugging Tips

### 1. Enable Debug Logging

```bash
RUST_LOG=debug cargo run -- serve
```

This shows all request/response details, including which backends are tried.

### 2. Check Backend Health

```bash
curl http://localhost:8000/health | jq
```

If `healthy_count` is 0, no requests will succeed.

### 3. Check Available Models

```bash
curl http://localhost:8000/v1/models | jq
```

If your model isn't listed, the backend either doesn't have it or isn't healthy.

### 4. Test Streaming

```bash
curl -N http://localhost:8000/v1/chat/completions \
  -d '{"model": "llama3:70b", "messages": [{"role": "user", "content": "Hi"}], "stream": true}'
```

The `-N` flag disables buffering so you see chunks as they arrive.

---

## Key Rust Concepts

| Concept | What It Means | Example in This Module |
|---------|---------------|------------------------|
| `State<T>` | Axum extractor for shared state | `State(state): State<Arc<AppState>>` |
| `Json<T>` | Axum extractor/response for JSON | `Json(request): Json<ChatCompletionRequest>` |
| `IntoResponse` | Trait to convert types to HTTP responses | `impl IntoResponse for ApiError` |
| `impl Stream` | Returns any type implementing Stream trait | `fn create_sse_stream(...) -> impl Stream<...>` |
| `Infallible` | Type that can never be constructed (no errors) | `Result<Event, Infallible>` |
| `async_stream::stream!` | Macro to create async iterators | `stream! { yield Ok(Event::default().data(...)); }` |
| `HeaderMap` | Collection of HTTP headers | `headers.get("Authorization")` |
| `tower::ServiceExt` | Extension trait for calling routers in tests | `app.call(request).await` |
| `bytes_stream()` | Convert response to streaming bytes | `response.bytes_stream()` |
| `serde(flatten)` | Capture unknown fields in HashMap | Extra fields passed to backends |

---

## Error Flow Diagram

```
                    Request arrives
                          │
                          ▼
              ┌───────────────────────┐
              │ Body > 10MB?          │──Yes──▶ 413 Payload Too Large
              └───────────┬───────────┘
                          │ No
                          ▼
              ┌───────────────────────┐
              │ JSON parsing failed?  │──Yes──▶ 400 Bad Request
              └───────────┬───────────┘         "Invalid request body"
                          │ No
                          ▼
              ┌───────────────────────┐
              │ Model exists?         │──No───▶ 404 Not Found
              └───────────┬───────────┘         "Model X not found.
                          │ Yes                  Available: [list]"
                          ▼
              ┌───────────────────────┐
              │ Healthy backends?     │──No───▶ 503 Service Unavailable
              └───────────┬───────────┘         "No healthy backends"
                          │ Yes
                          ▼
              ┌───────────────────────┐
              │ Backend request       │──Timeout──▶ 504 Gateway Timeout
              │                       │──Error────▶ Try next backend
              └───────────┬───────────┘             (up to max_retries)
                          │ Success
                          ▼
              ┌───────────────────────┐
              │ All retries failed?   │──Yes──▶ 502 Bad Gateway
              └───────────┬───────────┘         "All backends failed"
                          │ No
                          ▼
                    Return response
                    (200 OK + body)
```

---

## Summary

The API Gateway is the **front door** to Nexus, exposing OpenAI-compatible endpoints:

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/v1/chat/completions` | POST | Chat with LLM (streaming/non-streaming) |
| `/v1/models` | GET | List available models from all backends |
| `/health` | GET | System health for load balancers |

### Key Files

| File | Lines | One-Sentence Summary |
|------|-------|---------------------|
| `src/api/mod.rs` | ~115 | Router setup, AppState, body limit middleware |
| `src/api/types.rs` | ~430 | OpenAI-compatible request/response types |
| `src/api/completions.rs` | ~274 | Chat completions with retry and streaming |
| `src/api/models.rs` | ~60 | List models with deduplication |
| `src/api/health.rs` | ~50 | Health status with backend counts |

### Test Coverage

| Test File | Count | What It Tests |
|-----------|-------|---------------|
| `tests/api_contract.rs` | 8 | OpenAI format compliance |
| `tests/api_integration.rs` | 5 | Router setup and routing |
| `tests/api_streaming.rs` | 8 | SSE streaming end-to-end |
| `tests/api_edge_cases.rs` | 14 | Concurrency, timeouts, edge cases |

**Total**: 35 integration tests + type unit tests in `types.rs`

### Architecture Decisions

1. **No retry after streaming starts** - Once we send the first SSE chunk, we can't retry with another backend (would corrupt the stream)
2. **View models for JSON** - `BackendView` converts atomic fields to regular types for serialization
3. **Body limit via middleware** - Uses Tower's `RequestBodyLimitLayer` instead of checking in handlers
4. **Connection pooling** - Single `reqwest::Client` with `pool_max_idle_per_host(10)` for all backends

---

## Next Steps

Now that you understand the API Gateway, explore:

1. **Intelligent Router** (`src/routing/`) - Will add smart backend selection based on load, latency, and capabilities
2. **mDNS Discovery** (`src/discovery/`) - Automatically finds backends on the local network
3. **Configuration** (`src/config/`) - Customize timeouts, retry counts, and more
