# Feature Specification: Embeddings API (Retrospective)

**Feature Branch**: `020-embeddings-api`  
**Created**: 2025-02-17  
**Status**: Implemented  
**Type**: Retrospective Documentation  
**Feature ID**: F17  
**Input**: Document the already-implemented Embeddings API feature

> **Note**: This is a retrospective specification documenting an already-implemented feature. It describes what was built, not what should be built.

## Executive Summary

The Embeddings API (F17) implements an OpenAI-compatible endpoint (`POST /v1/embeddings`) that routes embedding requests to capable backend agents (OpenAI, Ollama). The feature integrates with the existing Nexus Inference Integration (NII) architecture defined in RFC-001, using the unified Router with capability-based backend selection.

## User Scenarios & Testing *(implemented)*

### User Story 1 - Single Text Embedding (Priority: P1)

API users (developers, applications) can generate embeddings for a single text input using a simple POST request to `/v1/embeddings`.

**Why this priority**: Core functionality - enables the most common use case of embedding a single piece of text for similarity search, clustering, or other ML operations.

**Independent Test**: Can be fully tested by sending a single text string and receiving a vector representation.

**Acceptance Scenarios**:

1. **Given** a valid model name and single text input, **When** user sends POST to `/v1/embeddings`, **Then** returns 200 OK with embedding vector in response
2. **Given** an empty string input, **When** user sends POST to `/v1/embeddings`, **Then** returns 400 Bad Request with error message
3. **Given** a model that doesn't exist, **When** user sends POST to `/v1/embeddings`, **Then** returns 404 Not Found

---

### User Story 2 - Batch Text Embedding (Priority: P2)

API users can generate embeddings for multiple text inputs in a single request to improve efficiency and reduce network overhead.

**Why this priority**: Important for performance - allows processing multiple texts together, which is common when embedding document collections, search queries, or datasets.

**Independent Test**: Can be tested by sending an array of strings and receiving corresponding embedding vectors indexed by position.

**Acceptance Scenarios**:

1. **Given** a valid model name and array of text inputs, **When** user sends POST to `/v1/embeddings`, **Then** returns 200 OK with multiple embedding objects, each indexed by position
2. **Given** a batch with some empty strings, **When** user sends POST to `/v1/embeddings`, **Then** returns 400 Bad Request
3. **Given** a large batch of inputs, **When** backend supports native batching (OpenAI), **Then** sends single request to backend
4. **Given** a large batch of inputs, **When** backend doesn't support native batching (Ollama), **Then** iterates through each input individually

---

### User Story 3 - Multi-Backend Routing (Priority: P1)

Nexus automatically routes embedding requests to a capable backend based on model name and backend capabilities.

**Why this priority**: Critical infrastructure - enables the core value proposition of Nexus as a unified gateway that abstracts away backend differences.

**Independent Test**: Can be tested by configuring different backends and verifying requests are routed to backends that declare embeddings capability.

**Acceptance Scenarios**:

1. **Given** multiple backends with embeddings capability, **When** user requests embedding with model "text-embedding-3-small", **Then** Router selects appropriate OpenAI backend
2. **Given** no backends with embeddings capability, **When** user sends embedding request, **Then** returns 503 Service Unavailable
3. **Given** a backend declares embeddings:false, **When** Router evaluates backends, **Then** excludes that backend from consideration
4. **Given** a model registered to specific backend, **When** user requests that model, **Then** Router directs request to that backend even if other embeddings-capable backends exist

---

### Edge Cases

- **Empty input validation**: Returns 400 when input array is empty or contains only empty strings
- **Unregistered agent**: Returns 502 Bad Gateway when selected agent ID is not found in registry
- **No capable backends**: Returns 503 Service Unavailable with clear error message when no backends declare embeddings:true
- **Model not found**: Returns 404 Not Found when requested model doesn't exist in any backend
- **Backend errors**: Properly propagates errors from backend agents (OpenAI rate limits, Ollama connection failures)
- **Mixed batch handling**: Both Ollama (iterative) and OpenAI (native batch) approaches correctly handle multi-input requests

## Requirements *(implemented)*

### Functional Requirements

- **FR-001**: System MUST expose POST endpoint at `/v1/embeddings` that accepts OpenAI-compatible requests
- **FR-002**: System MUST support both single string input (`"input": "text"`) and batch array input (`"input": ["text1", "text2"]`)
- **FR-003**: System MUST return OpenAI-compatible response format with `data`, `model`, `usage`, and `object` fields
- **FR-004**: System MUST validate requests and return 400 for empty or invalid inputs
- **FR-005**: System MUST use Router to select backend based on model name and embeddings capability
- **FR-006**: System MUST check `AgentCapabilities.embeddings` boolean before routing to a backend
- **FR-007**: System MUST return 503 when no backends have embeddings capability enabled
- **FR-008**: System MUST return 404 when requested model is not found in any capable backend
- **FR-009**: System MUST return 502 when selected agent is not registered in the agent registry
- **FR-010**: OpenAI agent MUST implement embeddings by forwarding to `/v1/embeddings` with bearer token authentication
- **FR-011**: Ollama agent MUST implement embeddings by forwarding to `/api/embed`, iterating per-input for batch requests
- **FR-012**: LMStudio and Generic agents MUST return Unsupported error for embeddings requests
- **FR-013**: System MUST estimate token usage using chars/4 heuristic for routing decisions
- **FR-014**: System MUST preserve request headers (Authorization, custom headers) when forwarding to backends

### Implementation Components

**API Layer** (`src/api/embeddings.rs` - 301 lines):
- `EmbeddingInput` enum: Single(String) | Batch(Vec<String>)
- `EmbeddingRequest` struct: model, input, encoding_format (optional)
- `EmbeddingResponse` struct: object="list", data, model, usage
- `EmbeddingObject` struct: object="embedding", embedding (Vec<f32>), index
- `EmbeddingUsage` struct: prompt_tokens, total_tokens
- `embeddings_handler()`: Main endpoint handler with validation, routing, error handling
- Unit tests: 8 tests covering input conversion, validation, error scenarios

**Agent Trait** (`src/agent/mod.rs`):
- `embeddings()` method on `InferenceAgent` trait
- Default implementation returns `Unsupported` error

**OpenAI Agent** (`src/agent/openai.rs`, lines 357-422):
- Implements `embeddings()` by forwarding to OpenAI `/v1/embeddings`
- Uses bearer token authentication
- Handles both single and batch inputs natively
- Preserves OpenAI response format

**Ollama Agent** (`src/agent/ollama.rs`, lines 291-353):
- Implements `embeddings()` by forwarding to Ollama `/api/embed`
- Iterates through batch inputs (Ollama doesn't support native batching)
- Transforms Ollama response format to OpenAI-compatible format
- Builds response with indexed embedding objects

**LMStudio and Generic Agents**:
- Return `Unsupported` error (use default trait implementation)
- Set `embeddings: false` in `AgentCapabilities`

**Routing** (`src/routing/mod.rs`):
- Uses unified Router with capability checking
- Filters backends by `capabilities.embeddings == true`
- Estimates tokens using `chars / 4` heuristic
- Returns descriptive errors for no capable backends

**Integration Tests** (`tests/embeddings_test.rs` - 146 lines, 5 tests):
1. `embeddings_route_exists`: Verifies endpoint is registered
2. `embeddings_returns_valid_response`: Tests successful embedding with mock backend
3. `embeddings_handles_batch_input`: Tests array input processing
4. `embeddings_validates_empty_input`: Tests 400 error for empty input
5. `embeddings_returns_404_for_unknown_model`: Tests model not found handling

### Key Entities

- **EmbeddingRequest**: Represents incoming API request with model name and text input(s)
- **EmbeddingInput**: Union type supporting both single string and array of strings
- **EmbeddingObject**: Individual embedding result with vector, index, and metadata
- **EmbeddingResponse**: Complete API response with embeddings array, model info, and token usage
- **AgentCapabilities**: Declares whether an agent supports embeddings via boolean flag

## Success Criteria *(achieved)*

### Measurable Outcomes

- **SC-001**: ✅ API clients can generate embeddings through a single unified endpoint regardless of backend
- **SC-002**: ✅ System correctly routes requests only to backends with embeddings capability enabled
- **SC-003**: ✅ Batch embedding requests reduce network overhead compared to individual requests (when backend supports native batching)
- **SC-004**: ✅ Response format matches OpenAI specification for drop-in compatibility with existing tools
- **SC-005**: ✅ Unsupported backends (LMStudio, Generic) gracefully return 503 with clear error message
- **SC-006**: ✅ All edge cases (empty input, missing model, no backends) return appropriate HTTP status codes
- **SC-007**: ✅ 100% test coverage for critical paths (13 total tests: 5 integration + 8 unit)

### Architecture Compliance

- **AC-001**: ✅ Follows RFC-001 NII architecture with capability-based routing
- **AC-002**: ✅ Uses unified Router for backend selection (same as chat completions)
- **AC-003**: ✅ Agent trait provides default implementation allowing agents to opt-in
- **AC-004**: ✅ OpenAI-compatible format ensures ecosystem compatibility

## Implementation Notes

### Design Decisions

1. **OpenAI Compatibility**: Chose OpenAI format as the standard to maximize ecosystem compatibility
2. **Capability-Based Routing**: Extended existing AgentCapabilities pattern to include embeddings flag
3. **Unified Router**: Reused chat completion routing logic for consistency
4. **Ollama Iteration**: Implemented per-input iteration for Ollama since it doesn't support batch requests natively
5. **Token Estimation**: Used simple chars/4 heuristic (same as chat) for routing decisions
6. **Default Unsupported**: Made embeddings opt-in via agent trait, with default returning Unsupported error

### Known Limitations

1. **Ollama Batching**: Ollama agent makes N sequential requests for N inputs, which is less efficient than OpenAI's native batch support
2. **Token Estimation**: The chars/4 heuristic is approximate and may not match actual tokenization
3. **Encoding Format**: The `encoding_format` parameter is parsed but not enforced (always returns float arrays)
4. **No Caching**: Embeddings are not cached, so identical inputs generate new requests each time
5. **Limited Validation**: No validation of embedding dimensions or vector normalization

### Testing Strategy

**Unit Tests** (8 tests in `src/api/embeddings.rs`):
- Input conversion (single → vec, batch → vec)
- Empty input detection
- Request/response serialization
- Error handling

**Integration Tests** (5 tests in `tests/embeddings_test.rs`):
- Endpoint registration
- End-to-end request/response flow with mock backend
- Batch input handling
- Empty input validation
- Model not found errors

**Manual Testing**:
- Tested with real OpenAI backend
- Tested with real Ollama backend
- Verified LMStudio returns Unsupported
- Verified multi-backend routing behavior

## Dependencies

- **Internal**: Depends on Router, AgentRegistry, InferenceAgent trait, ApiError types
- **External**: Uses Axum for HTTP handling, Serde for JSON serialization, reqwest for backend forwarding
- **RFC Alignment**: Implements RFC-001 NII architecture patterns

## Future Enhancements (Not Implemented)

While the current implementation is complete and functional, these enhancements could be considered:

1. **Embedding Caching**: Cache frequently-requested embeddings to reduce backend load
2. **Dimension Validation**: Validate embedding dimensions match model specifications
3. **Encoding Format Support**: Implement base64 encoding format option
4. **Streaming Support**: Support streaming for very large batch requests
5. **Ollama Batch Optimization**: Parallel processing for Ollama batch requests
6. **Token Counter Integration**: Use actual tokenizer instead of chars/4 heuristic
7. **Rate Limiting**: Per-model or per-user rate limiting for embeddings
8. **Metrics**: Track embedding generation times, token usage, cache hit rates

## Related Documentation

- **RFC-001**: Nexus Inference Integration (NII) architecture
- **Agent Implementation Guide**: See `src/agent/mod.rs` for trait details
- **Router Documentation**: See `src/routing/mod.rs` for capability-based routing
- **OpenAI API Spec**: https://platform.openai.com/docs/api-reference/embeddings
- **Ollama API Spec**: https://github.com/ollama/ollama/blob/main/docs/api.md#generate-embeddings

## Verification

This specification was created retrospectively based on the implemented code:

- ✅ All functionality described is present in the codebase
- ✅ All tests pass (5 integration + 8 unit tests)
- ✅ Code references verified: `src/api/embeddings.rs`, `src/agent/{openai,ollama,mod}.rs`, `tests/embeddings_test.rs`
- ✅ Architecture alignment with RFC-001 confirmed
- ✅ OpenAI compatibility verified through response format matching

---

**Document Version**: 1.0  
**Last Updated**: 2025-02-17  
**Specification Type**: Retrospective (Post-Implementation Documentation)
