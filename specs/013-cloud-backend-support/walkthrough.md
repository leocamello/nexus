# F12: Cloud Backend Support — Implementation Walkthrough

## 1. Overview

F12 adds cloud LLM providers (OpenAI, Anthropic, Google) as first-class backends alongside local inference engines. Each cloud provider is implemented as an `InferenceAgent` — the same trait used by Ollama, vLLM, and other local backends — so the router and API layer are completely provider-agnostic. Translation between OpenAI-compatible format and vendor-specific APIs happens inside each agent (no standalone translators), following the anti-abstraction principle.

Key additions: three cloud agent implementations, a cost estimation system, a transparent protocol for exposing routing metadata via HTTP headers, and actionable 503 error responses.

## 2. Architecture

Cloud backends plug into the existing NII (Normalized Inference Interface) architecture:

```
Client Request (OpenAI format)
       │
       ▼
  ┌─────────┐     ┌────────────┐     ┌─────────────────────┐
  │ API Layer│────▶│   Router   │────▶│  InferenceAgent      │
  │ (axum)   │     │ (scoring)  │     │  ┌─OllamaAgent      │
  │          │     │            │     │  ├─OpenAIAgent       │
  │ Injects  │     │ Capability │     │  ├─AnthropicAgent    │
  │ X-Nexus  │     │ matching   │     │  ├─GoogleAIAgent     │
  │ headers  │     │ + privacy  │     │  └─GenericAgent      │
  └─────────┘     └────────────┘     └─────────────────────┘
                                              │
                                    ┌─────────┴──────────┐
                                    ▼                    ▼
                              Local backends       Cloud APIs
                              (Ollama, vLLM)    (OpenAI, Anthropic, Google)
```

All agents implement `InferenceAgent` (defined in `src/agent/mod.rs`). The factory (`src/agent/factory.rs`) instantiates the correct agent based on `BackendType`. The router selects backends by capability, load, and privacy zone — it never knows whether the target is local or cloud.

## 3. Key Files

| File | Purpose |
|------|---------|
| `src/agent/mod.rs` | `InferenceAgent` trait definition, `AgentProfile`, `HealthStatus`, `TokenCount` |
| `src/agent/factory.rs` | `create_agent()` — dispatches on `BackendType` to build the right agent |
| `src/agent/openai.rs` | OpenAI agent — Bearer auth, tiktoken counting, passthrough API |
| `src/agent/anthropic.rs` | Anthropic agent — `x-api-key` auth, request/response translation, SSE conversion |
| `src/agent/google.rs` | Google AI agent — query-param auth, role mapping, `systemInstruction` extraction |
| `src/agent/pricing.rs` | `PricingTable` — per-model cost estimation from token usage |
| `src/agent/types.rs` | `PrivacyZone` enum (`Restricted`, `Open`) |
| `src/api/headers.rs` | `NexusTransparentHeaders` — X-Nexus-* header injection |
| `src/api/error.rs` | `ActionableErrorContext`, `ServiceUnavailableError` — contextual 503s |
| `src/api/completions.rs` | Request handler — routes to agent, injects headers after response |
| `src/registry/backend.rs` | `BackendType` enum — includes `Anthropic` and `Google` variants |
| `src/config/backend.rs` | `BackendConfig` — `api_key_env`, `zone`, `tier` fields for cloud config |

## 4. Data Flow

### Non-Streaming Request

```
1. POST /v1/chat/completions  (OpenAI-format JSON body)
2. Router selects backend (capability match + privacy zone filter)
3. API layer calls agent.chat_completion(&request)
4. Agent translates request → vendor format (Anthropic/Google only)
5. Agent sends HTTP request to cloud API with auth
6. Agent translates response → OpenAI format
7. API layer builds axum::Response with JSON body
8. NexusTransparentHeaders::inject_into_response() adds X-Nexus-* headers
9. Response returned to client (JSON body untouched, metadata in headers)
```

### Streaming Request

```
1. POST /v1/chat/completions  { "stream": true }
2. Router selects backend
3. API layer calls agent.chat_completion_stream(&request)
4. Agent opens SSE connection to vendor API
5. Agent translates each vendor SSE chunk → OpenAI SSE format
6. API layer wraps stream in axum::Response (SSE content-type)
7. X-Nexus-* headers injected on the response (before body streams)
8. SSE chunks forwarded to client in OpenAI format
```

## 5. Cloud Agent Implementations

### OpenAI (`src/agent/openai.rs`)

**Auth**: Bearer token in `Authorization` header. API key resolved from env var specified in `api_key_env`.

**Translation**: None needed — OpenAI format is the native format. Requests and responses pass through directly.

**Token counting**: Uses `tiktoken-rs` with `o200k_base` encoding for exact counts (`TokenCount::Exact`), replacing the default heuristic.

**Streaming**: `response.bytes_stream()` forwarded directly as SSE — no translation needed.

**Health check**: `GET /v1/models` — counts returned models; 401 → `Unhealthy`.

### Anthropic (`src/agent/anthropic.rs`)

**Auth**: Custom `x-api-key` header (not Bearer). Also sends `anthropic-version: 2023-06-01`.

**Translation** (request):
- System messages extracted from the messages array → top-level `system` parameter
- `max_tokens` added if absent (defaults to 4096, required by Anthropic API)
- Messages posted to `POST /v1/messages`

**Translation** (response):
- `content[].text` → OpenAI `choices[].message.content`
- `stop_reason` → `finish_reason`
- Usage mapped to OpenAI `usage` object

**Streaming**: Anthropic SSE events (`message_start`, `content_block_delta`, `message_delta`, `message_stop`) parsed and translated to OpenAI `chat.completion.chunk` format.

**Models**: Hardcoded Claude 3 family (Opus, Sonnet, Haiku, 3.5-Sonnet) with capability metadata.

### Google AI (`src/agent/google.rs`)

**Auth**: API key as URL query parameter (`?key={api_key}`).

**Translation** (request):
- System messages extracted → `systemInstruction` field with `parts` array
- Role mapping: `"assistant"` → `"model"` (Google's role name)
- Posted to `POST /v1beta/models/{model}:generateContent?key=...`

**Translation** (response):
- `candidates[0].content.parts[]` → OpenAI `choices[].message.content`
- Finish reasons mapped: `STOP`, `MAX_TOKENS`, `SAFETY` → OpenAI equivalents

**Streaming**: `POST /v1beta/models/{model}:streamGenerateContent?alt=sse&key=...` — SSE chunks translated to OpenAI format.

**Models**: Discovered via `GET /v1beta/models` — filtered to those supporting `generateContent`. Supports embeddings.

## 6. Nexus-Transparent Protocol

All responses include `X-Nexus-*` headers. Headers are injected AFTER the response body is built — the JSON body is never modified (OpenAI strict compatibility).

Implementation in `src/api/headers.rs`:

| Header | Values | Description |
|--------|--------|-------------|
| `X-Nexus-Backend` | e.g. `"openai-gpt4"` | Name of the backend that served this request |
| `X-Nexus-Backend-Type` | `"local"` or `"cloud"` | Classification based on `BackendType` |
| `X-Nexus-Route-Reason` | `capability-match`, `capacity-overflow`, `privacy-requirement`, `failover` | Why this backend was selected |
| `X-Nexus-Privacy-Zone` | `"restricted"` or `"open"` | Privacy zone of the serving backend |
| `X-Nexus-Cost-Estimated` | e.g. `"0.0042"` | Estimated USD cost (cloud only, 4 decimal places) |

Additionally, `x-nexus-fallback-model` is set when a fallback route was used (defined in `src/api/completions.rs`).

Injection happens via `NexusTransparentHeaders::inject_into_response(&mut resp)` — a single method that writes all headers, ensuring consistency across streaming and non-streaming paths.

## 7. Cost Estimation

`PricingTable` (`src/agent/pricing.rs`) holds hardcoded per-model pricing:

```rust
pub struct ModelPricing {
    pub input_price_per_1k: f64,   // USD per 1K input tokens
    pub output_price_per_1k: f64,  // USD per 1K output tokens
}
```

**Cost formula**: `(input_tokens / 1000 × input_rate) + (output_tokens / 1000 × output_rate)`

Cost is computed from the response's `usage` field (actual tokens consumed, not estimates). Returns `None` for unknown models — the header is omitted in that case.

**Covered models**: GPT-4-turbo, GPT-3.5-turbo, Claude 3 family, Gemini 1.5 Pro/Flash.

## 8. Actionable Errors

When no backend can serve a request, Nexus returns a 503 with machine-readable context (defined in `src/api/error.rs`):

```rust
pub struct ActionableErrorContext {
    pub required_tier: Option<u8>,            // Tier the request needed
    pub available_backends: Vec<String>,      // What backends exist in the fleet
    pub eta_seconds: Option<u64>,             // Estimated recovery time
    pub privacy_zone_required: Option<String>, // Privacy constraint that filtered out backends
}

pub struct ServiceUnavailableError {
    pub error: ApiErrorBody,            // Standard OpenAI-format error body
    pub context: ActionableErrorContext, // Nexus-specific retry intelligence
}
```

Factory methods for common scenarios:
- `ServiceUnavailableError::tier_unavailable(tier, backends)` — capability tier not available
- `ServiceUnavailableError::privacy_unavailable(zone, backends)` — privacy zone constraint
- `ServiceUnavailableError::all_backends_down()` — total fleet outage

This enables clients to make informed retry decisions without guessing.

## 9. Configuration

Cloud backends are configured in TOML with `api_key_env`, `zone`, and `tier`:

```toml
[[backends]]
name = "openai-gpt4"
url = "https://api.openai.com"
type = "openai"
api_key_env = "OPENAI_API_KEY"    # Env var holding the API key (required for cloud)
zone = "open"                      # Privacy zone (default: "open" for cloud types)
tier = 5                           # Capability tier 1-5 (default: 3)

[[backends]]
name = "anthropic-claude"
url = "https://api.anthropic.com"
type = "anthropic"
api_key_env = "ANTHROPIC_API_KEY"

[[backends]]
name = "google-gemini"
url = "https://generativelanguage.googleapis.com"
type = "google"
api_key_env = "GOOGLE_API_KEY"
```

**Validation**: Cloud backends (`openai`, `anthropic`, `google`) require `api_key_env`. Local backends ignore it.

**Privacy zone defaults**: Local types (Ollama, vLLM, llama.cpp, Exo, LMStudio, Generic) → `Restricted`. Cloud types (OpenAI, Anthropic, Google) → `Open`. The `zone` field overrides the default.

## 10. Testing

| File/Module | Scope | What It Tests |
|-------------|-------|---------------|
| `src/agent/openai.rs` (unit) | OpenAI agent | Health check, model listing with capability heuristics, Bearer auth, 401 handling, network errors |
| `src/agent/anthropic.rs` (unit) | Anthropic agent | System message extraction, response translation, profile metadata, health check |
| `src/agent/google.rs` (unit) | Google agent | `systemInstruction` mapping, role translation, model filtering, embeddings capability |
| `src/agent/pricing.rs` (unit) | Cost estimation | Price lookup for each provider, unknown model returns `None` |
| `src/agent/factory.rs` (unit) | Agent factory | All 9 `BackendType` variants instantiate correctly, env var override, missing API key errors |
| `src/api/headers.rs` (unit) | Header protocol | Header injection, `RouteReason` serialization, cloud/local classification |
| `tests/cloud_backends_test.rs` | Integration | Privacy zone defaults (cloud → Open, local → Restricted), backend type classification |
| `tests/agent_integration.rs` | Integration | Dual storage (Backend + Agent), registry cleanup, agent profile mapping, cancellation safety |
| `tests/pricing_test.rs` | Integration | End-to-end cost estimation |
| `tests/transparent_protocol_test.rs` | Integration | X-Nexus-* header injection in responses |

All cloud agent unit tests use `mockito::Server` to simulate vendor APIs. Integration tests use `wiremock` for the HTTP layer. Tests marked `#[ignore]` require live API keys.
