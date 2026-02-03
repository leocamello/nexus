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

### Phase 1: Contract Tests (OpenAI API Compliance)

**Goal**: Define contract tests that verify OpenAI API format compliance. These tests define the expected behavior before any implementation.

**Tests to write first** (8 contract tests):
1. `test_contract_completions_request_format` - Valid request accepted
2. `test_contract_completions_response_format` - Response matches OpenAI schema
3. `test_contract_completions_streaming_format` - SSE chunks match OpenAI schema
4. `test_contract_models_response_format` - /v1/models matches OpenAI schema
5. `test_contract_error_400_format` - Bad request error matches OpenAI schema
6. `test_contract_error_404_format` - Not found error matches OpenAI schema
7. `test_contract_error_502_format` - Bad gateway error matches OpenAI schema
8. `test_contract_error_503_format` - Service unavailable matches OpenAI schema

**Implementation**:
1. Create `tests/api_contract.rs` with JSON schema validation tests
2. Tests should FAIL initially (no implementation yet)

**Acceptance**: All 8 contract tests defined and verified to FAIL.

---

### Phase 2: Types and Error Handling

**Goal**: Define all OpenAI-compatible request/response types and error handling to make contract tests pass.

**Unit tests to write** (12 tests):
1. `test_chat_message_deserialize_text` - Simple text message
2. `test_chat_message_deserialize_multimodal` - Vision content parts
3. `test_chat_request_deserialize_minimal` - Only required fields
4. `test_chat_request_deserialize_full` - All optional fields
5. `test_chat_request_stream_default_false` - stream defaults to false
6. `test_chat_response_serialize` - Response matches OpenAI format
7. `test_chat_chunk_serialize` - Streaming chunk format
8. `test_usage_serialize` - Token counts format (optional field)
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

**Acceptance**: All 12 unit tests pass, contract tests from Phase 1 now pass.

---

### Phase 3: AppState and Router Setup

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

### Phase 4: Health Endpoint

**Goal**: Implement enhanced /health endpoint with backend status.

**Integration tests** (5 tests):
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

### Phase 5: Models Endpoint

**Goal**: Implement GET /v1/models returning OpenAI-format model list.

**Integration tests** (6 tests):
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

### Phase 6: Chat Completions - Non-Streaming

**Goal**: Implement POST /v1/chat/completions for non-streaming requests with retry logic.

**Integration tests** (14 tests - includes edge cases):
1. `test_completions_valid_request` - Returns valid response
2. `test_completions_invalid_json` - Returns 400 with error
3. `test_completions_model_not_found` - Returns 404 with hint
4. `test_completions_no_healthy_backends` - Returns 503
5. `test_completions_backend_timeout` - Returns 504
6. `test_completions_backend_error` - Returns 502
7. `test_completions_forwards_auth_header` - Authorization passed through
8. `test_completions_passes_usage_stats` - Usage from backend in response (or omitted)
9. `test_completions_retry_on_failure` - Retries with next healthy backend
10. `test_completions_tracks_pending_requests` - Increments/decrements registry
11. `test_completions_all_retries_fail` - Returns 502 after max_retries exhausted
12. `test_completions_payload_too_large` - Returns 413 for oversized body
13. `test_completions_long_model_name` - Handles long model names correctly
14. `test_completions_backend_invalid_json` - Returns 502 for non-JSON backend response

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

**Acceptance**: All 14 tests pass, non-streaming completions work with retry.

---

### Phase 7: Chat Completions - Streaming

**Goal**: Implement SSE streaming for POST /v1/chat/completions with `stream: true`.

**Integration tests** (10 tests - includes edge cases):
1. `test_streaming_returns_sse` - Content-Type is text/event-stream
2. `test_streaming_sends_chunks` - Receives multiple data: lines
3. `test_streaming_done_message` - Final message is `data: [DONE]`
4. `test_streaming_chunk_format` - Each chunk is valid JSON
5. `test_streaming_forwards_immediately` - No buffering (timing test)
6. `test_streaming_backend_error_mid_stream` - Handles disconnect gracefully
7. `test_streaming_client_disconnect` - Cancels backend request via tokio::select!
8. `test_streaming_model_not_found` - Returns error before streaming starts
9. `test_streaming_backend_format_transform` - Transforms non-OpenAI SSE to OpenAI format
10. `test_streaming_backend_invalid_sse` - Returns 502 for unparseable SSE

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

**Acceptance**: All 10 tests pass, streaming works end-to-end.

---

### Phase 8: Integration with CLI Serve Command

**Goal**: Wire up the API router to the existing `nexus serve` command.

**Integration tests** (5 tests):
1. `test_serve_starts_api_server` - Server accepts requests
2. `test_serve_with_config_timeout` - Timeout from config applied
3. `test_serve_graceful_shutdown` - In-flight requests complete within 30s
4. `test_serve_rejects_after_shutdown` - New requests rejected with 503 during shutdown
5. `test_serve_shutdown_timeout` - Forced termination after 30s if requests don't complete

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

**Acceptance**: All 5 tests pass, `nexus serve` runs the full API.

---

### Phase 9: Concurrent Request Handling & Performance

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

### Phase 10: Documentation and Cleanup

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

## Open Questions (Resolved)

1. **Router integration**: Simple backend selection with retry for MVP. Full router scoring deferred to F05.
   - **Decision**: ✅ MVP includes retry logic (FR-008) but not load/latency scoring.

2. **Model aggregation**: One entry per unique model ID (deduplicated across backends).
   - **Decision**: ✅ Deduplicate by model.id in /v1/models response.

3. **Usage stats source**: Pass through from backend; omit usage field if backend doesn't provide it.
   - **Decision**: ✅ Aligned with updated FR-006.

4. **Retry policy**: Immediate retry with next healthy backend (no backoff). Retry on: connection error, timeout, 5xx response.
   - **Decision**: ✅ Simple immediate retry; exponential backoff in F05 Router.

---

## Complexity Tracking

| Gate | Status | Justification |
|------|--------|---------------|
| Simplicity (≤3 modules) | ✅ | 3 logical groups: handlers, types, router setup |
| Anti-Abstraction | ✅ | Direct Axum/reqwest usage, no wrappers |
| Integration-First | ✅ | OpenAI contract tests, wiremock backends |
| Performance | ✅ | Targets defined, tests planned |

## Test Summary

| Phase | Contract | Integration | Unit | Total |
|-------|----------|-------------|------|-------|
| Phase 1: Contract Tests | 8 | 0 | 0 | 8 |
| Phase 2: Types | 0 | 0 | 12 | 12 |
| Phase 3: Router Setup | 0 | 5 | 0 | 5 |
| Phase 4: Health | 0 | 5 | 0 | 5 |
| Phase 5: Models | 0 | 6 | 0 | 6 |
| Phase 6: Non-Streaming | 0 | 14 | 0 | 14 |
| Phase 7: Streaming | 0 | 10 | 0 | 10 |
| Phase 8: CLI Integration | 0 | 5 | 0 | 5 |
| Phase 9: Performance | 0 | 5 | 0 | 5 |
| **Total** | **8** | **50** | **12** | **70** |

> Test order follows constitution: Contract → Integration → Unit
