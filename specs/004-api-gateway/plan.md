# Implementation Plan: Core API Gateway

**Spec**: [spec.md](./spec.md)  
**Status**: Ready for Implementation  
**Estimated Complexity**: High

## Approach

Implement the OpenAI-compatible API Gateway using Axum, with streaming SSE support via async-stream. Follow strict TDD: write failing tests first (Red), implement to make them pass (Green), then refactor.

### Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| HTTP framework | Axum 0.7 | Already in stack, excellent async support |
| Request routing | Axum Router with extractors | Type-safe, composable |
| Streaming | `Sse<impl Stream>` + async-stream | Axum-native SSE, backpressure handling |
| Backend client | reqwest with connection pooling | Already configured, supports streaming |
| Error types | Single `ApiError` with `IntoResponse` | Clean handler signatures |
| Request timeout | Tower layer + reqwest timeout | Defense in depth |
| Shared state | `Arc<AppState>` via Axum State | Thread-safe, cloneable |

### File Structure

```
src/
├── main.rs                 # Entry point (unchanged)
├── lib.rs                  # Add api module export
├── api/
│   ├── mod.rs              # Router setup, AppState, create_router()
│   ├── completions.rs      # POST /v1/chat/completions (streaming + non-streaming)
│   ├── models.rs           # GET /v1/models handler
│   ├── health.rs           # GET /health handler (enhanced)
│   └── types.rs            # Request/Response types + ApiError
├── cli/
│   └── serve.rs            # Update to use api::create_router()
├── config/                 # (existing)
├── health/                 # (existing)
└── registry/               # (existing)

tests/
└── api_integration.rs      # End-to-end API tests with mock backends
```

### Dependencies

**Already in Cargo.toml:**
- `axum = { version = "0.7", features = ["macros"] }` ✓
- `reqwest = { version = "0.12", features = ["json", "stream"] }` ✓
- `async-stream = "0.3"` ✓
- `futures = "0.3"` ✓
- `serde = { features = ["derive"] }` ✓
- `serde_json = "1"` ✓
- `tracing = "0.1"` ✓
- `tokio = { features = ["full"] }` ✓
- `tower-http = { features = ["trace", "cors"] }` ✓

**New dependencies needed:**
```toml
# SSE streaming with Axum (check if already included in axum features)
# None - axum's Sse is in the base crate

# Request timeout layer
# tower-http already has timeout - may need to add feature
tower-http = { version = "0.5", features = ["trace", "cors", "timeout"] }

# Streaming response body parsing
futures-util = "0.3"  # For StreamExt on response body
```

**Dev dependencies (already present):**
- `wiremock = "0.6"` ✓ - Mock HTTP backends
- `tokio-test = "0.4"` ✓

---

## Constitution Check

### Simplicity Gate
- [x] Using ≤3 main modules? → Yes: 1 handler group (completions/models/health) + 1 types + 1 mod.rs
- [x] No speculative features? → Yes: Only implementing spec requirements
- [x] Start with simplest approach? → Yes: Direct proxy, no caching

### Anti-Abstraction Gate
- [x] Using Axum/reqwest directly? → Yes: No wrapper layers
- [x] Single representation per data type? → Yes: Request types match OpenAI spec
- [x] No framework-on-framework? → Yes: Standard Axum patterns

### Integration-First Gate
- [x] API contracts defined? → Yes: OpenAI Chat Completions API
- [x] Integration tests planned? → Yes: wiremock-based tests
- [x] E2E flow testable? → Yes: Full request→backend→response flow

### Performance Gate
- [x] Routing decision < 1ms? → Delegated to Router (F05)
- [x] Total overhead < 5ms? → Yes: Target in NFR-002
- [x] Memory baseline < 50MB? → Yes: NFR-006 targets < 10MB overhead

---

## Implementation Phases

### Phase 1: Types and Error Handling (Tests First)

**Goal**: Define all OpenAI-compatible request/response types and error handling.

**Tests to write first** (12 tests):
1. `test_chat_message_deserialize_text` - Simple text message
2. `test_chat_message_deserialize_multimodal` - Vision content parts
3. `test_chat_request_deserialize_minimal` - Only required fields
4. `test_chat_request_deserialize_full` - All optional fields
5. `test_chat_request_stream_default_false` - stream defaults to false
6. `test_chat_response_serialize` - Response matches OpenAI format
7. `test_chat_chunk_serialize` - Streaming chunk format
8. `test_usage_serialize` - Token counts format
9. `test_api_error_serialize_400` - Bad request error format
10. `test_api_error_serialize_404` - Model not found format
11. `test_api_error_serialize_502` - Bad gateway format
12. `test_api_error_into_response` - ApiError implements IntoResponse

**Implementation**:

1. Create `src/api/types.rs`:
   ```rust
   // Request types
   pub struct ChatCompletionRequest {
       pub model: String,
       pub messages: Vec<ChatMessage>,
       #[serde(default)]
       pub stream: bool,
       // ... optional fields with #[serde(default)]
       #[serde(flatten)]
       pub extra: HashMap<String, Value>,
   }
   
   pub struct ChatMessage {
       pub role: String,
       #[serde(flatten)]
       pub content: MessageContent,
       // ... optional fields
   }
   
   #[serde(untagged)]
   pub enum MessageContent {
       Text { content: String },
       Parts { content: Vec<ContentPart> },
   }
   
   // Response types
   pub struct ChatCompletionResponse { ... }
   pub struct ChatCompletionChunk { ... }
   pub struct Usage { ... }
   
   // Error types
   pub struct ApiError { ... }
   impl IntoResponse for ApiError { ... }
   ```

2. Create `src/api/mod.rs`:
   ```rust
   pub mod types;
   // (handlers added in later phases)
   ```

3. Update `src/lib.rs` to export `api` module

**Acceptance**: All 12 tests pass, types serialize/deserialize correctly.

---

### Phase 2: AppState and Router Setup (Tests First)

**Goal**: Create shared application state and Axum router skeleton.

**Tests to write first** (5 tests):
1. `test_app_state_creation` - AppState can be created with registry
2. `test_router_has_completions_route` - POST /v1/chat/completions exists
3. `test_router_has_models_route` - GET /v1/models exists
4. `test_router_has_health_route` - GET /health exists
5. `test_router_returns_404_unknown` - Unknown routes return 404

**Implementation**:

1. Add to `src/api/mod.rs`:
   ```rust
   pub mod completions;
   pub mod models;
   pub mod health;
   pub mod types;
   
   use std::sync::Arc;
   use axum::{Router, routing::{get, post}};
   use crate::registry::Registry;
   use crate::config::NexusConfig;
   
   pub struct AppState {
       pub registry: Arc<Registry>,
       pub config: Arc<NexusConfig>,
       pub http_client: reqwest::Client,
   }
   
   impl AppState {
       pub fn new(registry: Arc<Registry>, config: Arc<NexusConfig>) -> Self {
           let http_client = reqwest::Client::builder()
               .timeout(std::time::Duration::from_secs(
                   config.server.request_timeout_seconds
               ))
               .pool_max_idle_per_host(10)
               .build()
               .expect("Failed to create HTTP client");
           
           Self { registry, config, http_client }
       }
   }
   
   pub fn create_router(state: Arc<AppState>) -> Router {
       Router::new()
           .route("/v1/chat/completions", post(completions::handle))
           .route("/v1/models", get(models::handle))
           .route("/health", get(health::handle))
           .with_state(state)
   }
   ```

2. Create stub handlers that return 501 Not Implemented

**Acceptance**: All 5 tests pass, router responds to correct paths.

---

### Phase 3: Health Endpoint (Tests First)

**Goal**: Implement enhanced /health endpoint with backend status.

**Tests to write first** (5 tests):
1. `test_health_all_healthy` - Returns "healthy" status
2. `test_health_some_unhealthy` - Returns "degraded" status
3. `test_health_none_healthy` - Returns "unhealthy" status
4. `test_health_includes_backend_counts` - JSON has total/healthy/unhealthy
5. `test_health_includes_model_count` - JSON has models count

**Implementation**:

1. Create `src/api/health.rs`:
   ```rust
   use axum::{extract::State, Json};
   use serde::Serialize;
   use std::sync::Arc;
   use crate::api::AppState;
   
   #[derive(Serialize)]
   pub struct HealthResponse {
       pub status: String,
       pub uptime_seconds: u64,
       pub backends: BackendCounts,
       pub models: usize,
   }
   
   #[derive(Serialize)]
   pub struct BackendCounts {
       pub total: usize,
       pub healthy: usize,
       pub unhealthy: usize,
   }
   
   pub async fn handle(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
       let all_backends = state.registry.get_all_backends();
       let healthy_backends = state.registry.get_healthy_backends();
       let model_count = state.registry.model_count();
       
       let status = match (healthy_backends.len(), all_backends.len()) {
           (h, t) if h == t && t > 0 => "healthy",
           (h, _) if h > 0 => "degraded",
           _ => "unhealthy",
       };
       
       Json(HealthResponse {
           status: status.to_string(),
           uptime_seconds: 0, // TODO: Track startup time
           backends: BackendCounts {
               total: all_backends.len(),
               healthy: healthy_backends.len(),
               unhealthy: all_backends.len() - healthy_backends.len(),
           },
           models: model_count,
       })
   }
   ```

**Acceptance**: All 5 tests pass, health endpoint returns correct status.

---

### Phase 4: Models Endpoint (Tests First)

**Goal**: Implement GET /v1/models returning OpenAI-format model list.

**Tests to write first** (6 tests):
1. `test_models_empty_registry` - Returns empty list
2. `test_models_single_backend` - Returns models from one backend
3. `test_models_multiple_backends` - Aggregates unique models
4. `test_models_excludes_unhealthy` - Only healthy backend models
5. `test_models_includes_capabilities` - context_length, vision, tools
6. `test_models_format_matches_openai` - object="list", data array

**Implementation**:

1. Create `src/api/models.rs`:
   ```rust
   use axum::{extract::State, Json};
   use serde::Serialize;
   use std::sync::Arc;
   use std::collections::HashMap;
   use crate::api::AppState;
   
   #[derive(Serialize)]
   pub struct ModelsResponse {
       pub object: String,
       pub data: Vec<ModelObject>,
   }
   
   #[derive(Serialize)]
   pub struct ModelObject {
       pub id: String,
       pub object: String,
       pub created: i64,
       pub owned_by: String,
       // Nexus extensions
       pub context_length: Option<u32>,
       pub capabilities: Option<ModelCapabilities>,
   }
   
   #[derive(Serialize)]
   pub struct ModelCapabilities {
       pub vision: bool,
       pub tools: bool,
       pub json_mode: bool,
   }
   
   pub async fn handle(State(state): State<Arc<AppState>>) -> Json<ModelsResponse> {
       let backends = state.registry.get_healthy_backends();
       let mut models_map: HashMap<String, ModelObject> = HashMap::new();
       
       for backend in backends {
           for model in &backend.models {
               models_map.entry(model.id.clone()).or_insert_with(|| {
                   ModelObject {
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

**Acceptance**: All 6 tests pass, models endpoint returns correct format.

---

### Phase 5: Chat Completions - Non-Streaming (Tests First)

**Goal**: Implement POST /v1/chat/completions for non-streaming requests.

**Tests to write first** (10 tests):
1. `test_completions_valid_request` - Returns valid response
2. `test_completions_invalid_json` - Returns 400 with error
3. `test_completions_model_not_found` - Returns 404 with hint
4. `test_completions_no_healthy_backends` - Returns 503
5. `test_completions_backend_timeout` - Returns 504
6. `test_completions_backend_error` - Returns 502
7. `test_completions_forwards_auth_header` - Authorization passed through
8. `test_completions_passes_usage_stats` - Usage from backend in response
9. `test_completions_retry_on_failure` - Retries with next backend
10. `test_completions_tracks_pending_requests` - Increments/decrements registry

**Implementation**:

1. Create `src/api/completions.rs`:
   ```rust
   use axum::{
       extract::{State, Json as AxumJson},
       http::{HeaderMap, StatusCode},
       response::{IntoResponse, Response},
   };
   use std::sync::Arc;
   use tracing::{info, warn, error};
   use crate::api::{AppState, types::*};
   
   pub async fn handle(
       State(state): State<Arc<AppState>>,
       headers: HeaderMap,
       AxumJson(request): AxumJson<ChatCompletionRequest>,
   ) -> Result<Response, ApiError> {
       // Log request
       info!(model = %request.model, stream = request.stream, "Chat completion request");
       
       // Check if streaming requested
       if request.stream {
           return handle_streaming(state, headers, request).await;
       }
       
       // Find backends for model
       let backends = state.registry.get_backends_for_model(&request.model);
       if backends.is_empty() {
           return Err(ApiError::model_not_found(&request.model, &available_models(&state)));
       }
       
       // Filter to healthy backends
       let healthy: Vec<_> = backends.into_iter()
           .filter(|b| b.status == BackendStatus::Healthy)
           .collect();
       
       if healthy.is_empty() {
           return Err(ApiError::service_unavailable("No healthy backends available"));
       }
       
       // Try backends with retry
       let max_retries = state.config.routing.max_retries;
       let mut last_error = None;
       
       for (attempt, backend) in healthy.iter().take(max_retries + 1).enumerate() {
           // Increment pending
           let _ = state.registry.increment_pending(&backend.id);
           
           match proxy_request(&state.http_client, backend, &headers, &request).await {
               Ok(response) => {
                   let _ = state.registry.decrement_pending(&backend.id);
                   return Ok(AxumJson(response).into_response());
               }
               Err(e) => {
                   let _ = state.registry.decrement_pending(&backend.id);
                   warn!(backend = %backend.id, attempt, error = %e, "Backend request failed");
                   last_error = Some(e);
               }
           }
       }
       
       // All retries failed
       Err(last_error.unwrap_or_else(|| ApiError::bad_gateway("All backends failed")))
   }
   
   async fn proxy_request(
       client: &reqwest::Client,
       backend: &Backend,
       headers: &HeaderMap,
       request: &ChatCompletionRequest,
   ) -> Result<ChatCompletionResponse, ApiError> {
       let url = format!("{}/v1/chat/completions", backend.url);
       
       let mut req = client.post(&url).json(request);
       
       // Forward Authorization header
       if let Some(auth) = headers.get("authorization") {
           req = req.header("Authorization", auth);
       }
       
       let response = req.send().await
           .map_err(|e| ApiError::bad_gateway(&e.to_string()))?;
       
       if response.status() == StatusCode::GATEWAY_TIMEOUT {
           return Err(ApiError::gateway_timeout());
       }
       
       if !response.status().is_success() {
           let status = response.status();
           let body = response.text().await.unwrap_or_default();
           return Err(ApiError::from_backend(status, &body));
       }
       
       response.json::<ChatCompletionResponse>().await
           .map_err(|e| ApiError::bad_gateway(&format!("Invalid backend response: {}", e)))
   }
   ```

**Acceptance**: All 10 tests pass, non-streaming completions work.

---

### Phase 6: Chat Completions - Streaming (Tests First)

**Goal**: Implement SSE streaming for POST /v1/chat/completions with `stream: true`.

**Tests to write first** (8 tests):
1. `test_streaming_returns_sse` - Content-Type is text/event-stream
2. `test_streaming_sends_chunks` - Receives multiple data: lines
3. `test_streaming_done_message` - Final message is `data: [DONE]`
4. `test_streaming_chunk_format` - Each chunk is valid JSON
5. `test_streaming_forwards_immediately` - No buffering (timing test)
6. `test_streaming_backend_error_mid_stream` - Handles disconnect
7. `test_streaming_client_disconnect` - Cancels backend request
8. `test_streaming_model_not_found` - Returns error before streaming

**Implementation**:

1. Add to `src/api/completions.rs`:
   ```rust
   use axum::response::sse::{Event, Sse};
   use futures::stream::Stream;
   use async_stream::stream;
   
   async fn handle_streaming(
       state: Arc<AppState>,
       headers: HeaderMap,
       request: ChatCompletionRequest,
   ) -> Result<Response, ApiError> {
       // Find backend (same as non-streaming)
       let backends = state.registry.get_backends_for_model(&request.model);
       if backends.is_empty() {
           return Err(ApiError::model_not_found(&request.model, &available_models(&state)));
       }
       
       let healthy: Vec<_> = backends.into_iter()
           .filter(|b| b.status == BackendStatus::Healthy)
           .collect();
       
       if healthy.is_empty() {
           return Err(ApiError::service_unavailable("No healthy backends available"));
       }
       
       // For streaming, use first healthy backend (no retry mid-stream)
       let backend = healthy.into_iter().next().unwrap();
       let _ = state.registry.increment_pending(&backend.id);
       
       // Create SSE stream
       let stream = create_sse_stream(
           state.clone(),
           state.http_client.clone(),
           backend,
           headers,
           request,
       );
       
       Ok(Sse::new(stream).into_response())
   }
   
   fn create_sse_stream(
       state: Arc<AppState>,
       client: reqwest::Client,
       backend: Backend,
       headers: HeaderMap,
       request: ChatCompletionRequest,
   ) -> impl Stream<Item = Result<Event, std::convert::Infallible>> {
       stream! {
           let url = format!("{}/v1/chat/completions", backend.url);
           
           let mut req = client.post(&url).json(&request);
           if let Some(auth) = headers.get("authorization") {
               req = req.header("Authorization", auth);
           }
           
           let response = match req.send().await {
               Ok(r) => r,
               Err(e) => {
                   let _ = state.registry.decrement_pending(&backend.id);
                   yield Ok(Event::default().data(format!(
                       r#"{{"error":{{"message":"{}","type":"server_error"}}}}"#,
                       e
                   )));
                   return;
               }
           };
           
           // Stream response body line by line
           let mut stream = response.bytes_stream();
           let mut buffer = String::new();
           
           while let Some(chunk) = stream.next().await {
               match chunk {
                   Ok(bytes) => {
                       buffer.push_str(&String::from_utf8_lossy(&bytes));
                       
                       // Process complete lines
                       while let Some(pos) = buffer.find('\n') {
                           let line = buffer[..pos].trim();
                           if line.starts_with("data: ") {
                               let data = &line[6..];
                               if data == "[DONE]" {
                                   yield Ok(Event::default().data("[DONE]"));
                               } else {
                                   yield Ok(Event::default().data(data.to_string()));
                               }
                           }
                           buffer = buffer[pos + 1..].to_string();
                       }
                   }
                   Err(e) => {
                       warn!(error = %e, "Stream error");
                       break;
                   }
               }
           }
           
           let _ = state.registry.decrement_pending(&backend.id);
       }
   }
   ```

**Acceptance**: All 8 tests pass, streaming works end-to-end.

---

### Phase 7: Integration with CLI Serve Command

**Goal**: Wire up the API router to the existing `nexus serve` command.

**Tests to write first** (4 tests):
1. `test_serve_starts_api_server` - Server accepts requests
2. `test_serve_with_config_timeout` - Timeout from config applied
3. `test_serve_graceful_shutdown` - In-flight requests complete
4. `test_serve_rejects_after_shutdown` - New requests rejected during shutdown

**Implementation**:

1. Update `src/cli/serve.rs`:
   ```rust
   use crate::api::{self, AppState};
   
   pub async fn run_serve(args: &ServeArgs, config: NexusConfig) -> Result<()> {
       // ... existing setup code ...
       
       // Create shared state
       let registry = Arc::new(Registry::new());
       let config = Arc::new(config);
       let app_state = Arc::new(AppState::new(registry.clone(), config.clone()));
       
       // Create API router
       let app = api::create_router(app_state);
       
       // ... health checker setup ...
       
       // Start server
       let listener = tokio::net::TcpListener::bind(&addr).await?;
       info!("Listening on http://{}", addr);
       
       axum::serve(listener, app)
           .with_graceful_shutdown(shutdown_signal())
           .await?;
       
       Ok(())
   }
   ```

**Acceptance**: All 4 tests pass, `nexus serve` runs the full API.

---

### Phase 8: Concurrent Request Handling & Performance

**Goal**: Verify concurrent request handling and measure performance.

**Tests to write first** (5 tests):
1. `test_concurrent_100_requests` - 100 parallel requests succeed
2. `test_concurrent_mixed_streaming` - Mix of streaming/non-streaming
3. `test_response_time_under_5ms` - Overhead measurement
4. `test_memory_overhead_under_10mb` - Memory baseline check
5. `test_connection_pooling` - Reuses connections to backends

**Implementation**:

1. Add load test in `tests/api_integration.rs`:
   ```rust
   #[tokio::test]
   async fn test_concurrent_100_requests() {
       let (server, _mock) = setup_test_server().await;
       
       let client = reqwest::Client::new();
       let futures: Vec<_> = (0..100)
           .map(|_| {
               let client = client.clone();
               async move {
                   client
                       .post(&format!("{}/v1/chat/completions", server.url()))
                       .json(&minimal_request())
                       .send()
                       .await
               }
           })
           .collect();
       
       let results = futures::future::join_all(futures).await;
       
       for result in results {
           assert!(result.is_ok());
           assert_eq!(result.unwrap().status(), 200);
       }
   }
   ```

**Acceptance**: All 5 tests pass, performance targets met.

---

### Phase 9: Documentation and Cleanup

**Goal**: Complete documentation and final cleanup.

**Tasks**:
1. Add module-level doc comments to all `src/api/*.rs` files
2. Add doc comments with examples for public types
3. Update README.md with API usage examples
4. Run `cargo clippy --all-targets -- -D warnings`
5. Run `cargo fmt --all`
6. Create walkthrough.md

**Acceptance**: No clippy warnings, all documentation complete.

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Streaming complexity | Start with non-streaming, add streaming after tests pass |
| Backend format differences | Transform responses to OpenAI format in proxy layer |
| Connection exhaustion | Use reqwest connection pooling with limits |
| Memory leaks in streams | Use RAII patterns, test with long-running streams |
| Timeout edge cases | Test timeout at various stages (connect, read, idle) |

## Open Questions

1. **Router integration**: This plan assumes simple first-healthy-backend selection. Full router scoring (F05) will be added later. Is this acceptable for MVP?
   - **Decision**: ✅ Yes, simple selection for MVP. Router feature will add scoring.

2. **Model aggregation**: When same model exists on multiple backends, should we show one entry or multiple?
   - **Decision**: ✅ One entry per unique model ID (deduplicated).

3. **Usage stats source**: Confirmed pass-through from backend. What if backend doesn't include usage?
   - **Decision**: Return null/omit usage field if backend doesn't provide it.

---

## Complexity Tracking

| Gate | Status | Justification |
|------|--------|---------------|
| Simplicity (≤3 modules) | ✅ | 3 logical groups: handlers, types, router setup |
| Anti-Abstraction | ✅ | Direct Axum/reqwest usage, no wrappers |
| Integration-First | ✅ | OpenAI contract tests, wiremock backends |
| Performance | ✅ | Targets defined, tests planned |

## Test Summary

| Phase | Unit Tests | Integration Tests | Total |
|-------|------------|-------------------|-------|
| Phase 1: Types | 12 | 0 | 12 |
| Phase 2: Router | 5 | 0 | 5 |
| Phase 3: Health | 0 | 5 | 5 |
| Phase 4: Models | 0 | 6 | 6 |
| Phase 5: Non-Streaming | 0 | 10 | 10 |
| Phase 6: Streaming | 0 | 8 | 8 |
| Phase 7: CLI Integration | 0 | 4 | 4 |
| Phase 8: Performance | 0 | 5 | 5 |
| **Total** | **17** | **38** | **55** |
