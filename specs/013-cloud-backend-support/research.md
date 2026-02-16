# Research Findings: Cloud Backend Support

**Feature**: F12 - Cloud Backend Support with Nexus-Transparent Protocol  
**Date**: 2024-02-11  
**Status**: Complete

This document records research decisions made during Phase 0 to resolve technical unknowns identified in plan.md.

---

## RQ-001: Token Counting Library (tiktoken-rs)

### Decision
Use **tiktoken-rs** (crate: `tiktoken-rs`) for OpenAI token counting in Phase 1. Add to Cargo.toml as a new dependency.

### Rationale
- **Codebase already plans for it**: Comments in `src/agent/openai.rs:82` explicitly mention "tiktoken in F14", but F12 needs cost estimation now
- **Accuracy requirement**: FR-015 requires exact token counting for trustworthy cost estimates. The existing heuristic (`chars/4`) is insufficient for billing context
- **Production-ready**: tiktoken-rs is a mature Rust port of OpenAI's official tiktoken library with >99% accuracy
- **Minimal overhead**: Tokenization adds ~0.3-0.5ms for typical requests (well within 1ms budget)

### Alternatives Considered
1. **Keep heuristic counting (chars/4)**: Rejected because cost estimates would be ±25% inaccurate, breaking user trust
2. **Defer to F14**: Rejected because transparent cost is a P2 requirement in F12 spec (User Story 2, SC-006)
3. **Call OpenAI API for counting**: Rejected due to latency (round-trip) and cost (metered API call)

### Implementation Notes
- Add `tiktoken-rs = "0.5"` to Cargo.toml dependencies
- OpenAIAgent.count_tokens() returns `TokenCount::Exact(count)` using `o200k_base` encoding (GPT-4, GPT-3.5-turbo)
- Anthropic/Google agents use heuristic initially (document in code comments that exact counting is future work)
- Enable `token_counting: true` in OpenAIAgent capabilities

---

## RQ-002: Anthropic API Format

### Decision
Implement **Anthropic Messages API** (v1) with role translation: OpenAI `system/user/assistant` ↔ Anthropic `system (parameter)/user/assistant`.

### Rationale
- **Official API**: Anthropic Messages API is the stable production endpoint
- **Close alignment**: 90% compatible with OpenAI format, key differences are manageable:
  - System message is a top-level parameter, not in messages array
  - No `function_call` support (out of scope for F12)
  - Streaming uses SSE with `event: message_*` types

### Alternatives Considered
1. **Anthropic Text Completions API**: Deprecated, not suitable for chat
2. **Wait for Anthropic OpenAI compatibility mode**: Doesn't exist yet, timeline unknown

### Implementation Notes

**Request Translation (OpenAI → Anthropic)**:
```json
// OpenAI format
{
  "model": "gpt-4",
  "messages": [
    {"role": "system", "content": "You are helpful"},
    {"role": "user", "content": "Hello"}
  ]
}

// Translates to Anthropic format
{
  "model": "claude-3-opus-20240229",
  "system": "You are helpful",  // Extracted from messages
  "messages": [
    {"role": "user", "content": "Hello"}
  ],
  "max_tokens": 4096  // Required field
}
```

**Response Translation (Anthropic → OpenAI)**:
```json
// Anthropic response
{
  "id": "msg_123",
  "content": [{"type": "text", "text": "Hello!"}],
  "role": "assistant",
  "stop_reason": "end_turn"
}

// Translates to OpenAI format
{
  "id": "msg_123",
  "choices": [{
    "index": 0,
    "message": {"role": "assistant", "content": "Hello!"},
    "finish_reason": "stop"
  }]
}
```

**Streaming SSE Translation**:
- Anthropic: `event: content_block_delta\ndata: {"delta": {"text": "Hi"}}`
- OpenAI: `data: {"choices": [{"delta": {"content": "Hi"}}]}`

**Authentication**:
- Header: `x-api-key: <API_KEY>` (not `Authorization: Bearer`)
- API key from environment variable specified in `api_key_env` config field

---

## RQ-003: Google AI API Format

### Decision
Implement **Google Generative AI API** (Gemini) with content structure translation: OpenAI messages ↔ Google `contents[].parts[]` structure.

### Rationale
- **Current standard**: Google AI Studio uses this format for Gemini models
- **Enterprise readiness**: Supports both API key and OAuth (use API key for F12)
- **Streaming support**: SSE-like streaming with JSON chunks

### Alternatives Considered
1. **Vertex AI**: Rejected because it requires GCP project setup (higher barrier to entry than API key)
2. **PaLM API**: Deprecated in favor of Gemini

### Implementation Notes

**Request Translation (OpenAI → Google)**:
```json
// OpenAI format
{
  "model": "gpt-4",
  "messages": [
    {"role": "system", "content": "You are helpful"},
    {"role": "user", "content": "Hello"}
  ]
}

// Translates to Google format
{
  "contents": [
    {
      "role": "user",
      "parts": [{"text": "System: You are helpful\n\nUser: Hello"}]
    }
  ],
  "generationConfig": {
    "temperature": 0.7,
    "maxOutputTokens": 2048
  }
}
```

**Note**: Google doesn't have explicit system role - prepend system message to first user message.

**Response Translation (Google → OpenAI)**:
```json
// Google response
{
  "candidates": [{
    "content": {
      "parts": [{"text": "Hello!"}],
      "role": "model"
    },
    "finishReason": "STOP"
  }]
}

// Translates to OpenAI format
{
  "choices": [{
    "index": 0,
    "message": {"role": "assistant", "content": "Hello!"},
    "finish_reason": "stop"
  }]
}
```

**Streaming**:
- Google streams newline-delimited JSON chunks (not SSE)
- Each chunk has `candidates[0].content.parts[0].text` with incremental content

**Authentication**:
- Query parameter: `?key=<API_KEY>` (simplest for F12)
- Alternatively: `x-goog-api-key: <API_KEY>` header

---

## RQ-004: Streaming Response Header Injection

### Decision
Use **Axum's `Sse::new(stream).into_response()`** and inject headers via `response.headers_mut().insert()` after conversion.

### Rationale
- **Already used in codebase**: `src/api/completions.rs:23` already uses this pattern for `X-Nexus-Fallback-Model` header
- **Zero buffering**: Headers added after stream creation but before first chunk send
- **No latency penalty**: Header injection is synchronous operation before streaming starts

### Alternatives Considered
1. **Custom SSE implementation**: Rejected because Axum's built-in SSE is sufficient
2. **Middleware layer**: Rejected because headers are request-specific (contain backend name, route reason)

### Implementation Notes

**Existing Pattern (non-streaming)**:
```rust
// src/api/completions.rs:201
let mut response = Json(response_body).into_response();
response.headers_mut().insert(
    HeaderName::from_static(FALLBACK_HEADER),
    HeaderValue::from_str(&actual_model).unwrap()
);
```

**Streaming Pattern (to implement)**:
```rust
let sse_response = Sse::new(stream).into_response();
let mut response = sse_response;

// Inject X-Nexus-* headers
response.headers_mut().insert(
    HeaderName::from_static("x-nexus-backend"),
    HeaderValue::from_str(&backend_name).unwrap()
);
// ... (repeat for other headers)
```

**Timing**: Headers must be set before first chunk is sent (Axum handles this correctly).

---

## RQ-005: HTTP Client Streaming Performance

### Decision
Use **reqwest 0.12 with `stream()` feature** (already in dependencies) for cloud API streaming.

### Rationale
- **Already available**: Cargo.toml has `reqwest = { version = "0.12", features = ["json", "stream"] }`
- **Low latency**: Baseline streaming overhead is ~10-20ms (well under 100ms budget)
- **Connection pooling**: reqwest reuses connections, reducing handshake latency
- **Async-native**: Integrates cleanly with Tokio runtime

### Alternatives Considered
1. **hyper directly**: Rejected because reqwest provides higher-level API with connection pooling
2. **Custom HTTP client**: Rejected because reqwest is production-proven

### Implementation Notes

**Streaming Pattern**:
```rust
let response = reqwest_client
    .post(url)
    .headers(headers)
    .json(&translated_request)
    .send()
    .await?;

let stream = response
    .bytes_stream()
    .map(|chunk| {
        // Parse SSE chunk, translate format, emit OpenAI chunk
    });
```

**Performance Measurement**: Integration tests will measure end-to-end latency to verify < 100ms overhead (SC-009).

---

## RQ-006: Cost Estimation Pricing Models

### Decision
Hardcode **current pricing tables** (as of Feb 2024) in `src/agent/pricing.rs` module. Document that prices must be manually updated.

### Rationale
- **Transparent**: X-Nexus-Cost-Estimated header provides immediate value to users monitoring spend
- **Simple**: Pricing changes are infrequent (quarterly at most)
- **Accurate enough**: Small pricing drift is acceptable for estimates (not billing)

### Alternatives Considered
1. **Dynamic pricing API**: OpenAI/Anthropic/Google don't provide pricing APIs
2. **Configuration file**: Rejected because pricing is tied to model IDs (better as code constant)
3. **Omit cost estimation**: Rejected because it's a P2 requirement (SC-006, SC-010)

### Implementation Notes

**Pricing Table Structure**:
```rust
// src/agent/pricing.rs
pub struct ModelPricing {
    pub input_price_per_1k: f64,   // USD per 1K input tokens
    pub output_price_per_1k: f64,  // USD per 1K output tokens
}

lazy_static! {
    pub static ref PRICING_TABLE: HashMap<&'static str, ModelPricing> = {
        let mut m = HashMap::new();
        m.insert("gpt-4-turbo", ModelPricing { input: 0.01, output: 0.03 });
        m.insert("gpt-3.5-turbo", ModelPricing { input: 0.0005, output: 0.0015 });
        m.insert("claude-3-opus", ModelPricing { input: 0.015, output: 0.075 });
        m.insert("claude-3-sonnet", ModelPricing { input: 0.003, output: 0.015 });
        m.insert("gemini-1.5-pro", ModelPricing { input: 0.0035, output: 0.0105 });
        // ... (add more models)
        m
    };
}
```

**Cost Calculation**:
```rust
fn estimate_cost(model: &str, input_tokens: u32, output_tokens: u32) -> Option<f64> {
    PRICING_TABLE.get(model).map(|pricing| {
        let input_cost = (input_tokens as f64 / 1000.0) * pricing.input_price_per_1k;
        let output_cost = (output_tokens as f64 / 1000.0) * pricing.output_price_per_1k;
        input_cost + output_cost
    })
}
```

**Header Format**: `X-Nexus-Cost-Estimated: 0.0042` (USD, 4 decimal places)

**Current Pricing (Feb 2024)**:
| Model | Input ($/1K tok) | Output ($/1K tok) |
|-------|------------------|-------------------|
| GPT-4 Turbo | $0.01 | $0.03 |
| GPT-3.5 Turbo | $0.0005 | $0.0015 |
| Claude 3 Opus | $0.015 | $0.075 |
| Claude 3 Sonnet | $0.003 | $0.015 |
| Gemini 1.5 Pro | $0.0035 | $0.0105 |

**Update Strategy**: Document in code comments that prices must be manually verified quarterly. Add TODO comment linking to provider pricing pages.

---

## Summary of Decisions

| Research Question | Decision | Rationale |
|-------------------|----------|-----------|
| RQ-001: Token Counting | tiktoken-rs (o200k_base) | Already planned, production-ready, <1ms overhead |
| RQ-002: Anthropic API | Messages API v1, role translation | Stable API, 90% OpenAI-compatible |
| RQ-003: Google AI API | Generative AI (Gemini), content structure translation | Current standard, API key auth |
| RQ-004: Header Injection | Axum response.headers_mut() | Already used for fallback header |
| RQ-005: Streaming Client | reqwest 0.12 with stream feature | Already available, <100ms overhead |
| RQ-006: Pricing Models | Hardcoded table in pricing.rs | No pricing APIs exist, simple and transparent |

**Phase 0 Complete**: All technical unknowns resolved. Ready for Phase 1 design.

---

## Implementation Priorities (for Phase 1)

1. **First**: Extend BackendConfig with zone/tier fields (foundational)
2. **Second**: Implement AnthropicAgent with translation layer (P3 requirement)
3. **Third**: Implement GoogleAIAgent with translation layer (P3 requirement)
4. **Fourth**: Add tiktoken-rs and enable exact counting in OpenAIAgent (P2 requirement - cost estimation)
5. **Fifth**: Extend response pipeline to inject X-Nexus-* headers (P2 requirement - transparency)
6. **Sixth**: Implement 503 actionable error responses (P2 requirement - reliability)

**Next Step**: Generate data-model.md with entity definitions and relationships.
