# Research: Embeddings API Implementation

**Feature**: F17 - Embeddings API  
**Date**: 2025-02-17  
**Status**: Retrospective Documentation  

> **Note**: This document captures the design decisions and research that informed the implementation.

---

## Decision 1: API Format Standard

**Question**: Which API format should Nexus use for embeddings?

**Research**:
- **OpenAI `/v1/embeddings`**: Industry standard, widely adopted by tools (Continue.dev, Claude Code, LangChain)
  - Request: `{"model": "...", "input": "..." | [...]}`
  - Response: `{"object": "list", "data": [...], "model": "...", "usage": {...}}`
  - Supports both single string and batch array inputs natively
  
- **Ollama `/api/embed`**: Ollama-specific format
  - Request: `{"model": "...", "input": "..."}`
  - Response: `{"embeddings": [[...]]}`
  - Less common, limited ecosystem support
  
- **Custom Format**: Design Nexus-specific format
  - Would require client libraries to implement Nexus-specific code
  - Breaks OpenAI compatibility principle from constitution

**Decision**: **OpenAI `/v1/embeddings` format**

**Rationale**:
1. Constitution Principle III: "OpenAI-Compatible" — strict adherence to OpenAI API
2. Ecosystem compatibility: Works with existing tools without modification
3. Well-documented specification with clear request/response contracts
4. Supports both single and batch inputs (common use cases)

**Alternatives Rejected**:
- Ollama format: Limited adoption, breaks OpenAI compatibility
- Custom format: Violates constitution, forces client modifications

---

## Decision 2: Routing Strategy

**Question**: How should Nexus route embedding requests to backends?

**Research**:
- **Dedicated Embeddings Router**: Create separate routing logic for embeddings
  - Pros: Could optimize for embedding-specific metrics
  - Cons: Code duplication, inconsistent with RFC-001 NII architecture
  
- **Reuse Unified Router**: Extend existing Router with embeddings capability
  - Pros: Consistent with chat completions, single routing codebase
  - Cons: Router must understand embedding-specific requirements
  
- **Model Registry Only**: Route based on model name without capability checking
  - Pros: Simplest implementation
  - Cons: No way to disable embeddings for specific backends

**Decision**: **Reuse Unified Router with capability flag**

**Rationale**:
1. RFC-001 NII architecture: Unified Router handles all inference requests
2. Constitution Principle V: Capability-based routing (not just load-based)
3. `AgentCapabilities.embeddings: bool` flag enables/disables backends
4. Consistent error handling and backend selection logic

**Implementation**:
- Add `embeddings: bool` to `AgentCapabilities` struct
- Router filters backends where `capabilities.embeddings == true`
- Use same `RequestRequirements` pattern as chat completions
- Token estimation uses chars/4 heuristic (consistency with chat)

**Alternatives Rejected**:
- Dedicated router: Code duplication, violates constitution simplicity
- Model registry only: No capability-based filtering, forces all agents to support embeddings

---

## Decision 3: Agent Interface Design

**Question**: How should agents implement embedding generation?

**Research**:
- **Separate Trait**: Create `EmbeddingProvider` trait separate from `InferenceAgent`
  - Pros: Clear separation of concerns
  - Cons: Agents would implement multiple traits, routing more complex
  
- **Extend InferenceAgent Trait**: Add `embeddings()` method to existing trait
  - Pros: Single trait per agent, unified interface
  - Cons: All agents must implement (or default to unsupported)
  
- **Free Functions**: Implement embeddings as standalone functions per agent
  - Pros: No trait modifications
  - Cons: Loses polymorphism, routing can't use trait objects

**Decision**: **Extend `InferenceAgent` trait with default implementation**

**Rationale**:
1. Opt-in design: Default implementation returns `Unsupported`
2. Agents explicitly choose to implement embeddings
3. Consistent with other optional methods (e.g., model lifecycle)
4. Type signature enforces contract: `Vec<String>` → `Vec<Vec<f32>>`

**Type Signature**:
```rust
async fn embeddings(&self, _input: Vec<String>) -> Result<Vec<Vec<f32>>, AgentError> {
    Err(AgentError::Unsupported("embeddings"))
}
```

**Alternatives Rejected**:
- Separate trait: Complicates agent registry and routing
- Free functions: Loses polymorphism, can't use trait objects in router

---

## Decision 4: Batch Processing

**Question**: How should Nexus handle batch embedding requests?

**Research**:
- **OpenAI**: Native batch support — sends entire array in single request
  - `/v1/embeddings` accepts `"input": ["text1", "text2", ...]`
  - Returns `data` array with embeddings indexed by position
  
- **Ollama**: No native batch support — must iterate per-input
  - `/api/embed` accepts single input only
  - Multiple requests required for batches
  
- **API Design Options**:
  1. Single string only — reject arrays at API level
  2. Always iterate — even for OpenAI (consistent but inefficient)
  3. Delegate to agents — let agents handle batch processing

**Decision**: **Delegate batch handling to agents**

**Rationale**:
1. API accepts both formats via `EmbeddingInput` enum (OpenAI compatibility)
2. Handler converts to `Vec<String>` and passes to agent
3. OpenAI agent sends entire Vec in single request (efficient)
4. Ollama agent loops through Vec with individual requests (necessary limitation)

**Implementation**:
- `EmbeddingInput` enum: `Single(String)` | `Batch(Vec<String>)`
- `#[serde(untagged)]` for transparent deserialization
- `into_vec()` method converts both variants to `Vec<String>`
- OpenAI agent: Single POST with entire array
- Ollama agent: Loop with `for text in &input { ... }`

**Trade-offs**:
- Ollama: N sequential requests for N inputs (slower, but limitation of Ollama API)
- Future optimization: Parallelize Ollama requests with `tokio::spawn` (not implemented)

**Alternatives Rejected**:
- Single string only: Breaks OpenAI compatibility, forces clients to iterate
- Always iterate: Inefficient for OpenAI, throws away native batch support

---

## Decision 5: Token Estimation

**Question**: How should Nexus estimate token counts for routing decisions?

**Research**:
- **Exact Tokenization**: Use per-model tokenizers (e.g., tiktoken for OpenAI)
  - Pros: Accurate token counts
  - Cons: Adds latency, requires tokenizer per model, routing overhead
  
- **Chars/4 Heuristic**: Estimate tokens as `chars / 4`
  - Pros: Fast (< 1ms), consistent with chat completions
  - Cons: Approximate, may overestimate/underestimate
  
- **Backend Token Counts**: Wait for backend response to get actual tokens
  - Pros: Most accurate
  - Cons: Can't use for routing decisions (chicken-and-egg problem)

**Decision**: **Chars/4 heuristic for routing, backend tokens for response**

**Rationale**:
1. Constitution Performance Standard: Routing must complete in < 1ms
2. Consistency with chat completions routing (same formula)
3. Actual token counts reported by backend in `usage` field
4. Routing decisions need speed over precision

**Formula**: `estimated_tokens = sum(input.len() / 4 for input in inputs)`

**Implementation**:
```rust
let estimated_tokens: u32 = input_texts.iter().map(|s| s.len() as u32 / 4).sum();
```

**Usage**:
- Routing: Uses estimated tokens for backend selection
- Response: Uses backend-reported tokens in `usage` field
- Monitoring: Can compare estimated vs. actual for accuracy tracking

**Alternatives Rejected**:
- Exact tokenization: Too slow for routing, violates performance gate
- Backend tokens only: Can't make routing decisions without estimates

---

## Decision 6: Error Handling

**Question**: How should Nexus handle embedding-specific errors?

**Research**:
- **OpenAI Error Format**:
  - `400 Bad Request`: Invalid input, missing required fields
  - `401 Unauthorized`: Invalid API key
  - `404 Not Found`: Model not found
  - `429 Too Many Requests`: Rate limit exceeded
  - `503 Service Unavailable`: Backend overloaded
  
- **Nexus Error Patterns** (from chat completions):
  - `ApiError::bad_request()`: Client errors (400)
  - `ApiError::model_not_found()`: Model lookup failures (404)
  - `ApiError::service_unavailable()`: Backend unavailable (503)
  - `ApiError::bad_gateway()`: Backend errors (502)

**Decision**: **Reuse existing `ApiError` patterns with OpenAI-compatible format**

**Error Mapping**:
| Condition | HTTP Code | Error Method |
|-----------|-----------|--------------|
| Empty input | 400 | `bad_request()` |
| Invalid JSON | 422 | Axum default |
| Model not found | 404 | `model_not_found()` |
| No capable backends | 503 | `service_unavailable()` |
| Agent not registered | 502 | `bad_gateway()` |
| Backend error | 502 | `from_agent_error()` |

**Implementation**:
```rust
// Empty input validation
if input_texts.is_empty() {
    return Err(ApiError::bad_request("Input must not be empty"));
}

// Routing error mapping
.map_err(|e| match e {
    RoutingError::ModelNotFound { model } => ApiError::model_not_found(&model, &[]),
    RoutingError::NoHealthyBackend { model } => ApiError::service_unavailable(...),
    _ => ApiError::bad_gateway(...),
})

// Capability check
if !agent.profile().capabilities.embeddings {
    return Err(ApiError::service_unavailable("Backend does not support embeddings"));
}
```

**Alternatives Rejected**:
- Custom error format: Breaks OpenAI compatibility
- Pass through backend errors: Inconsistent format across backends

---

## Decision 7: LMStudio and Generic Agent Behavior

**Question**: Should LMStudio and Generic agents support embeddings?

**Research**:
- **LMStudio**: Supports OpenAI-compatible `/v1/embeddings` endpoint
  - Could implement by forwarding to LMStudio
  - Requires LMStudio instance to have embedding model loaded
  
- **Generic Agent**: Represents any OpenAI-compatible server
  - Unknown if backend supports embeddings
  - Could probe endpoint, but adds latency and complexity

**Decision**: **Return `Unsupported` error for both (use default trait implementation)**

**Rationale**:
1. LMStudio: Not commonly used for embeddings (primarily chat models)
2. Generic: Unknown capabilities, better to explicitly configure
3. Users can manually set `embeddings: true` in agent profile if needed
4. Simplifies implementation: only OpenAI and Ollama implemented

**Implementation**:
- LMStudio agent: Does not override `embeddings()` method
- Generic agent: Does not override `embeddings()` method
- Both use default trait implementation: `Err(AgentError::Unsupported("embeddings"))`
- `AgentCapabilities.embeddings = false` in profiles

**Future Enhancement**:
- Add `embeddings: true` configuration option in agent profiles
- Implement forwarding to `/v1/embeddings` when explicitly enabled

**Alternatives Rejected**:
- Always try `/v1/embeddings`: May fail for backends without support
- Probe endpoint: Adds latency and complexity to agent initialization

---

## Best Practices Research

### OpenAI Embeddings Best Practices

**Source**: OpenAI API Documentation

**Key Points**:
1. **Batch Processing**: Send multiple inputs in single request (up to 2048 inputs)
2. **Model Selection**: Choose model based on use case
   - `text-embedding-ada-002`: General purpose (1536 dims)
   - `text-embedding-3-small`: Faster, cheaper (512-1536 dims)
   - `text-embedding-3-large`: Highest quality (256-3072 dims)
3. **Input Length**: Max 8191 tokens per input for ada-002
4. **Encoding Format**: `float` (default) or `base64` (more compact)
5. **Normalization**: Embeddings are NOT normalized by default

**Nexus Implementation**:
- ✅ Batch support via `input` array
- ✅ Model name passed through from client
- ⚠️ `encoding_format` parsed but not enforced (always returns float)
- ℹ️ No normalization (passes through backend behavior)

### Ollama Embeddings Best Practices

**Source**: Ollama API Documentation

**Key Points**:
1. **Single Input**: Only supports one input per request
2. **Model Loading**: Must pull embedding model first (`ollama pull all-minilm`)
3. **Response Format**: Returns `{"embeddings": [[...]]}` (nested array)
4. **Common Models**: `all-minilm`, `nomic-embed-text`, `mxbai-embed-large`

**Nexus Implementation**:
- ✅ Iterates per-input for batches
- ✅ Hardcoded to `all-minilm` model (TODO: make configurable)
- ✅ Transforms response to OpenAI format
- ℹ️ Sequential processing (could be parallelized)

---

## Technology Stack Validation

### Dependencies Used

**Existing** (no new dependencies added):
- `axum`: HTTP framework for API endpoint
- `tokio`: Async runtime for concurrent requests
- `reqwest`: HTTP client for backend forwarding
- `serde`: JSON serialization/deserialization
- `serde_json`: JSON value manipulation
- `tracing`: Structured logging

**Justification**: Reuses existing Nexus infrastructure, no new dependencies required.

### Performance Characteristics

**Latency Budget**:
- JSON parsing: ~0.1ms (Axum)
- Input validation: ~0.01ms (length check)
- Token estimation: ~0.05ms (iterate + divide)
- Routing: ~0.5ms (existing Router)
- Backend request: Backend-dependent (500ms-5s)
- Response building: ~0.1ms (enumerate + construct)

**Total Overhead**: ~0.8ms (well under 2ms target from constitution)

**Memory Footprint**:
- Request: ~1KB per input (text + metadata)
- Response: ~6KB per embedding (1536 floats × 4 bytes)
- Batch of 10: ~60KB total
- No persistent storage (stateless)

---

## Integration Points

### Upstream Dependencies

**Router** (`src/routing/mod.rs`):
- Provides `select_backend()` for capability-based routing
- Filters backends by `capabilities.embeddings == true`
- Returns `RoutingResult` with selected backend

**Agent Registry** (`src/agent/registry.rs`):
- Provides `get_agent()` for agent lookup by backend ID
- Returns `Arc<dyn InferenceAgent>` trait objects

**API Error** (`src/api/error.rs`):
- Provides `ApiError` types for HTTP error responses
- Methods: `bad_request()`, `model_not_found()`, `service_unavailable()`, `bad_gateway()`

### Downstream Consumers

**API Clients**:
- OpenAI SDK (Python, JavaScript, etc.)
- Continue.dev (VS Code extension)
- Claude Code (AI coding assistant)
- Custom applications using OpenAI-compatible clients

**Backend Agents**:
- OpenAI agent: Forwards to OpenAI API
- Ollama agent: Forwards to Ollama API
- Future: LMStudio, Generic (when configured)

---

## Risk Assessment

### Implementation Risks

**Risk 1: Ollama Sequential Iteration Performance**
- **Impact**: Slow for large batches (N requests for N inputs)
- **Likelihood**: High (Ollama limitation)
- **Mitigation**: Document limitation, consider parallelization in future
- **Status**: Documented in spec and plan as known limitation

**Risk 2: Token Estimation Accuracy**
- **Impact**: Routing decisions may be suboptimal
- **Likelihood**: Medium (chars/4 is approximate)
- **Mitigation**: Backend reports actual tokens, can be monitored
- **Status**: Accepted trade-off for performance (< 1ms routing)

**Risk 3: Model Name Hardcoding**
- **Impact**: OpenAI always uses `ada-002`, Ollama always uses `all-minilm`
- **Likelihood**: High (not configurable in initial implementation)
- **Mitigation**: Model name in request is cosmetic (backend overrides)
- **Status**: Documented as future enhancement

### Security Considerations

**Input Validation**:
- ✅ Empty input rejection (prevents wasted backend calls)
- ✅ JSON parsing errors handled gracefully
- ⚠️ No max length validation (could cause large requests)

**Authentication**:
- ✅ OpenAI agent uses bearer token (from agent config)
- ✅ Ollama agent local only (no authentication needed)
- ℹ️ Nexus itself is local-first (no API auth required)

**Error Disclosure**:
- ✅ Backend errors sanitized through `ApiError`
- ✅ No raw error messages from backends exposed
- ℹ️ Debug logs may contain sensitive info (local deployment only)

---

## Alternatives Considered (Summary)

| Decision | Chosen | Alternatives Rejected |
|----------|--------|----------------------|
| API Format | OpenAI `/v1/embeddings` | Ollama format, Custom format |
| Routing | Unified Router | Dedicated router, Model registry only |
| Agent Interface | Extend trait with default | Separate trait, Free functions |
| Batch Processing | Delegate to agents | API-level iteration, Single string only |
| Token Estimation | Chars/4 heuristic | Exact tokenization, Backend tokens only |
| Error Handling | Reuse ApiError | Custom format, Pass through backend |
| LMStudio/Generic | Unsupported | Always try endpoint, Probe at init |

---

## References

**External Documentation**:
- OpenAI Embeddings API: https://platform.openai.com/docs/api-reference/embeddings
- Ollama Embeddings API: https://github.com/ollama/ollama/blob/main/docs/api.md#generate-embeddings
- OpenAI Embedding Models: https://platform.openai.com/docs/guides/embeddings

**Internal Documentation**:
- RFC-001: Nexus Inference Integration (NII) architecture
- Constitution: `.specify/memory/constitution.md`
- Agent Implementation Guide: `src/agent/mod.rs`
- Router Documentation: `src/routing/mod.rs`

**Related Features**:
- F01: Chat Completions API (routing patterns)
- F05: Model Registry (model capabilities)
- F10: Health Checking (backend status)

---

**Document Version**: 1.0  
**Created**: 2025-02-17  
**Type**: Retrospective Research Documentation
