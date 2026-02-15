# Research: Cloud Backend Support with Nexus-Transparent Protocol

**Feature**: F12 - Cloud Backend Support  
**Phase**: 0 - Research & Decision Making  
**Date**: 2025-02-15

## Overview

This document consolidates research findings for implementing cloud backend support in Nexus. All technical unknowns from the initial planning phase have been resolved through codebase analysis and API research.

---

## 1. Anthropic API Format Translation

### Decision
Implement bidirectional translation between OpenAI chat completion format and Anthropic Messages API format in a new `src/agent/anthropic.rs` module.

### Rationale
- **Format Differences**: Anthropic uses `/v1/messages` endpoint with different field structure:
  - System messages: Top-level `system` field (string) vs OpenAI's messages array
  - Roles: Both use "user"/"assistant" but Anthropic has no "system" role in messages array
  - Content structure: Anthropic uses `content: [{type: "text", text: "..."}]` array
- **Streaming**: Anthropic uses Server-Sent Events with different event types (`content_block_delta`, `message_stop`) vs OpenAI's `chat.completion.chunk` format
- **Authentication**: Anthropic requires `x-api-key` header + `anthropic-version: 2023-06-01` vs OpenAI's `Authorization: Bearer`

### Key Translation Mappings

| Aspect | OpenAI → Anthropic | Anthropic → OpenAI |
|--------|-------------------|-------------------|
| **Endpoint** | `/v1/chat/completions` → `/v1/messages` | - |
| **System Message** | Extract from messages[] → top-level `system` field | - |
| **Messages** | Filter out system messages | Keep as-is |
| **Response Content** | - | Flatten `content[0].text` → `message.content` |
| **Finish Reason** | - | `stop_reason: "end_turn"` → `finish_reason: "stop"` |
| **Streaming** | - | Aggregate events → SSE `data:` chunks |
| **Auth** | `Authorization: Bearer` → `x-api-key` + version header | - |

### Implementation Approach
```rust
// src/agent/anthropic.rs
pub struct AnthropicAgent {
    id: String,
    name: String,
    base_url: String,
    api_key: String,
    client: Arc<Client>,
}

impl InferenceAgent for AnthropicAgent {
    async fn chat_completion(&self, request: ChatCompletionRequest, ...) -> ... {
        // 1. Extract system message from request.messages
        // 2. Build Anthropic request format
        // 3. Send with x-api-key and anthropic-version headers
        // 4. Translate response back to OpenAI format
    }
}
```

### Alternatives Considered
- **Generic wrapper**: Would add abstraction layer violating Constitution Anti-Abstraction Gate
- **Runtime format detection**: Adds complexity; translation at agent level is cleaner
- **Proxy-only (no translation)**: Breaks OpenAI compatibility promise (Constitution Principle III)

---

## 2. Google AI (Gemini) API Format Translation

### Decision
Implement translation layer for Google AI's `generateContent` API format, following similar pattern to Anthropic but with Google-specific field mappings.

### Rationale
- **Format Differences**: Google uses significantly different terminology:
  - Endpoint: `POST /v1beta/models/{model}:generateContent`
  - Field names: `contents` (not messages), `candidates` (not choices), `parts` (not content)
  - Roles: "model" instead of "assistant"
  - System prompt: Top-level `systemInstruction` field
- **Model Versioning**: Different Gemini versions (1.5, 2.0) use same API but different capabilities
- **Authentication**: API key via `x-goog-api-key` header or `Authorization: Bearer`

### Key Translation Mappings

| OpenAI Field | Google AI Field | Notes |
|-------------|----------------|-------|
| `messages` | `contents` | Array of content objects |
| `role: "assistant"` | `role: "model"` | Direct string replacement |
| `content` (string) | `parts: [{text: "..."}]` | Wrap in parts array |
| `system` message | `systemInstruction` | Top-level field like Anthropic |
| `choices[0].message` | `candidates[0].content` | Extract from candidates |
| `finish_reason` | `finishReason` | Map STOP→"stop", MAX_TOKENS→"length" |
| `usage` | `usageMetadata` | Rename + field mapping |

### Implementation Approach
```rust
// src/agent/google.rs
pub struct GoogleAgent {
    id: String,
    name: String,
    base_url: String, // e.g., "https://generativelanguage.googleapis.com"
    api_key: String,
    client: Arc<Client>,
}

impl InferenceAgent for GoogleAgent {
    async fn chat_completion(&self, request: ChatCompletionRequest, ...) -> ... {
        // 1. Build URL with model in path: /v1beta/models/{model}:generateContent
        // 2. Translate messages → contents with role mapping
        // 3. Extract system instruction
        // 4. Send with x-goog-api-key header
        // 5. Translate candidates → choices format
    }
}
```

### Alternatives Considered
- **Use OpenAI-compatible proxy**: Google doesn't provide one officially
- **Shared translator with Anthropic**: APIs too different; separate implementations cleaner
- **Support only latest Gemini 2.0**: Users may want 1.5 Flash for cost; support both

---

## 3. Token Counting with tiktoken-rs

### Decision
Integrate tiktoken-rs crate for exact OpenAI token counting in `OpenAIAgent::count_tokens()` method. Use encoding selection based on model family.

### Rationale
- **Accuracy**: Constitution Principle X requires "audit-grade token counting"
- **Performance**: tiktoken-rs provides <1ms counting for typical messages
- **OpenAI Compatibility**: Official OpenAI tokenizer algorithm (BPE)
- **Current State**: Nexus currently uses heuristic (chars / 4) which is ~20-30% inaccurate

### Implementation Details

**Dependency**:
```toml
[dependencies]
tiktoken-rs = "0.5"
```

**Encoding Selection**:
| Model Family | Encoding | When to Use |
|-------------|----------|-------------|
| GPT-4o, GPT-4o-mini | `o200k_base` | Latest models (2024+) |
| GPT-4-turbo, GPT-3.5-turbo | `cl100k_base` | Earlier OpenAI models |

**Code Structure**:
```rust
// src/agent/openai.rs - enhance existing struct
pub struct OpenAIAgent {
    // ... existing fields ...
    encoding_o200k: Arc<Encoding>,  // Cache for GPT-4o
    encoding_cl100k: Arc<Encoding>, // Cache for GPT-3.5/GPT-4
}

async fn count_tokens(&self, model_id: &str, text: &str) -> TokenCount {
    let encoding = if model_id.contains("gpt-4o") {
        &self.encoding_o200k
    } else {
        &self.encoding_cl100k
    };
    
    let tokens = encoding.encode_ordinary(text);
    TokenCount::Exact(tokens.len() as u32)
}
```

**Performance Characteristics**:
- Encoding load: ~50-100ms (one-time, cached in struct)
- Token counting: <1ms per message (typical <2KB)
- Memory: ~5-10MB per encoding (acceptable per Constitution: <10KB per backend target applies to state, not cached data)

### Alternatives Considered
- **Continue with heuristic**: Violates accuracy requirement (SC-008: within 5%)
- **External tokenizer service**: Adds network dependency, violates local-first principle
- **Use Anthropic/Google tokenizers**: Not OpenAI-compatible; separate implementations needed

---

## 4. Cost Estimation Strategy

### Decision
Store pricing data in TOML configuration file with per-model input/output token rates. Calculate cost using response `usage` field. Omit X-Nexus-Cost-Estimated header if pricing unknown.

### Rationale
- **Maintainability**: TOML config allows updates without recompilation
- **Accuracy**: Using actual token counts from LLM responses (SC-008: within 5%)
- **Graceful Degradation**: Missing pricing = omit header (per spec edge case handling)
- **Multi-Provider**: Single config structure supports OpenAI, Anthropic, Google

### Pricing Configuration Structure
```toml
# nexus.toml
[pricing.openai]
"gpt-4" = { input = 0.03, output = 0.06 }  # per 1K tokens
"gpt-4-turbo" = { input = 0.01, output = 0.03 }
"gpt-3.5-turbo" = { input = 0.0005, output = 0.0015 }

[pricing.anthropic]
"claude-3-opus" = { input = 0.015, output = 0.075 }
"claude-3-sonnet" = { input = 0.003, output = 0.015 }

[pricing.google]
"gemini-2.0-flash" = { input = 0.0001, output = 0.0004 }
```

### Calculation Formula
```rust
// src/pricing/mod.rs (new module)
pub fn estimate_cost(
    provider: &str,
    model: &str,
    prompt_tokens: u32,
    completion_tokens: u32,
) -> Option<f32> {
    let pricing = PRICING_CONFIG.get(provider)?.get(model)?;
    
    let input_cost = (prompt_tokens as f32 / 1000.0) * pricing.input;
    let output_cost = (completion_tokens as f32 / 1000.0) * pricing.output;
    
    Some(input_cost + output_cost)
}
```

### Integration Point
```rust
// src/api/completions.rs - after receiving response
if let Some(cost) = estimate_cost(backend_type, model, prompt_tokens, completion_tokens) {
    response_headers.insert(
        HeaderName::from_static("x-nexus-cost-estimated"),
        HeaderValue::from_str(&format!("${:.4}", cost))?,
    );
}
// If None, gracefully omit header (per FR-010 and edge case handling)
```

### Current Pricing Rates (2024)
| Provider | Model | Input/1K | Output/1K |
|----------|-------|---------|-----------|
| OpenAI | GPT-4 | $0.03 | $0.06 |
| OpenAI | GPT-4-turbo | $0.01 | $0.03 |
| OpenAI | GPT-3.5-turbo | $0.0005 | $0.0015 |
| Anthropic | Claude 3 Opus | $0.015 | $0.075 |
| Anthropic | Claude 3 Sonnet | $0.003 | $0.015 |
| Google | Gemini 2.0 Flash | $0.0001 | $0.0004 |

### Alternatives Considered
- **External pricing API**: Adds network dependency, increases latency
- **Hardcode in agent**: Requires recompilation for updates, violates maintainability
- **Real-time billing API**: Most providers don't offer, adds complexity
- **Database storage**: Violates stateless principle (Constitution Principle VIII)

---

## 5. Backend Configuration Schema

### Decision
Extend existing `BackendConfig` struct with `zone` (PrivacyZone enum) and `tier` (u8) fields. Keep `api_key_env` for environment variable reference.

### Rationale
- **Privacy Integration**: F13 (Privacy-Aware Routing) dependency requires zone classification
- **Capability Matching**: Tier field enables routing based on model capability
- **Security**: Existing `api_key_env` pattern prevents secrets in config files
- **Minimal Change**: Extends existing structure rather than creating new one

### Configuration Schema
```toml
[[backends]]
name = "openai-gpt4"
url = "https://api.openai.com"
type = "openai"
api_key_env = "OPENAI_API_KEY"  # Existing field
zone = "open"                   # NEW: "restricted" or "open"
tier = 4                        # NEW: 0-4 capability tier
priority = 70                   # Existing field
```

### Code Changes
```rust
// src/config/backend.rs - extend existing struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    pub name: String,
    pub url: String,
    #[serde(rename = "type")]
    pub backend_type: BackendType,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default = "default_zone")]    // NEW
    pub zone: PrivacyZone,
    #[serde(default)]                     // NEW
    pub tier: Option<u8>,
}

fn default_zone() -> PrivacyZone {
    PrivacyZone::Open  // Cloud backends default to open zone
}
```

### Alternatives Considered
- **Separate CloudBackendConfig**: Would duplicate fields, violate DRY principle
- **Auto-detect zone**: Cannot be determined automatically; must be explicit (Constitution Principle IX)
- **Omit tier field**: Routing needs capability matching; tier is simplest representation

---

## 6. X-Nexus-* Response Headers

### Decision
Implement header injection at API response layer (`src/api/completions.rs`) using Axum's `HeaderMap`. Never modify JSON response body (Constitution Principle III).

### Rationale
- **OpenAI Compatibility**: Headers-only transparency preserves response body format
- **Routing Visibility**: Enables debugging without response parsing (User Story 2)
- **Constitution Compliance**: Principle III explicitly requires headers-only approach
- **Integration Point**: Completions handler already returns `impl IntoResponse`, can add headers

### Header Definitions

| Header Name | Type | Example | When Included |
|------------|------|---------|---------------|
| `X-Nexus-Backend` | String | `openai-gpt4` | Always |
| `X-Nexus-Backend-Type` | Enum | `cloud` or `local` | Always |
| `X-Nexus-Route-Reason` | Enum | `capacity-overflow` | Always |
| `X-Nexus-Privacy-Zone` | Enum | `open` | Always |
| `X-Nexus-Cost-Estimated` | Currency | `$0.0042` | Cloud backends only |

### Implementation
```rust
// src/api/headers.rs (NEW)
pub struct NexusHeaders {
    pub backend: String,
    pub backend_type: BackendType,
    pub route_reason: RouteReason,
    pub privacy_zone: PrivacyZone,
    pub cost: Option<f32>,
}

impl NexusHeaders {
    pub fn inject_into(self, headers: &mut HeaderMap) {
        headers.insert("x-nexus-backend", HeaderValue::from_str(&self.backend));
        headers.insert("x-nexus-backend-type", HeaderValue::from_str(&self.backend_type.as_str()));
        // ... etc
    }
}

// src/api/completions.rs - in chat_completion handler
let mut response = agent.chat_completion(request, headers).await?;
let nexus_headers = NexusHeaders::from_routing_decision(&routing_decision, &backend);
nexus_headers.inject_into(response.headers_mut());
```

### Streaming Consideration
Headers must be sent before first SSE chunk. Implementation:
```rust
// For streaming responses
let stream = agent.chat_completion_stream(request, headers).await?;
let mut response = Response::new(Body::from_stream(stream));
nexus_headers.inject_into(response.headers_mut());  // Headers sent immediately
```

### Alternatives Considered
- **Include in JSON body**: Violates Constitution Principle III, breaks OpenAI compatibility
- **Separate metadata endpoint**: Requires correlation, adds latency
- **Logging only**: Insufficient for client debugging (User Story 2 requirement)

---

## 7. Actionable 503 Error Context

### Decision
Implement structured error context object in OpenAI error envelope for 503 Service Unavailable responses. Include `required_tier`, `available_backends`, and `eta_seconds` fields.

### Rationale
- **Constitution Principle IX**: Explicit contracts require actionable errors
- **User Story 4**: Clients need structured data for retry logic
- **OpenAI Compatibility**: Wraps context in standard `error` object, preserves format
- **Minimal Impact**: Only affects error path, not success cases

### Error Format
```json
{
  "error": {
    "message": "No backend available for request",
    "type": "service_unavailable",
    "code": "capacity_exceeded",
    "context": {
      "required_tier": 4,
      "available_backends": [
        {"name": "openai-gpt4", "status": "at_capacity", "zone": "open"},
        {"name": "local-llama", "status": "unhealthy", "zone": "restricted"}
      ],
      "eta_seconds": 45
    }
  }
}
```

### Implementation
```rust
// src/api/error.rs (NEW)
#[derive(Serialize)]
pub struct ActionableErrorContext {
    pub required_tier: Option<u8>,
    pub available_backends: Vec<BackendStatus>,
    pub eta_seconds: Option<u32>,
}

#[derive(Serialize)]
pub struct BackendStatus {
    pub name: String,
    pub status: String,  // "healthy", "unhealthy", "at_capacity"
    pub zone: String,
}

pub fn create_503_response(context: ActionableErrorContext) -> Response {
    let error = OpenAIError {
        message: "No backend available for request".to_string(),
        error_type: "service_unavailable".to_string(),
        code: Some("capacity_exceeded".to_string()),
        param: None,
        context: Some(context),  // Extension field
    };
    
    Response::builder()
        .status(StatusCode::SERVICE_UNAVAILABLE)
        .json(ErrorResponse { error })
}
```

### Alternatives Considered
- **Non-structured error**: Violates Constitution Principle IX (explicit contracts)
- **HTTP 429 (rate limit)**: Semantically incorrect (429 is client rate limit, 503 is capacity)
- **Custom error format**: Breaks OpenAI compatibility

---

## Summary of Decisions

| Area | Decision | Key Rationale |
|------|----------|---------------|
| **Anthropic Translation** | Bidirectional format translation in agent | Different API structure requires translation |
| **Google Translation** | Similar pattern to Anthropic but separate | APIs too different to share translator |
| **Token Counting** | tiktoken-rs with encoding cache | Audit-grade accuracy, <1ms performance |
| **Cost Estimation** | TOML config + response usage field | Maintainable, accurate, graceful degradation |
| **Config Schema** | Extend BackendConfig with zone/tier | Minimal change, integrates with routing |
| **Response Headers** | Inject at API layer, never modify body | Constitution compliance, OpenAI compatibility |
| **Error Context** | Structured 503 with actionable fields | Explicit contracts, enables client retry logic |

All decisions resolve technical unknowns and enable progression to Phase 1 (Design & Contracts).
