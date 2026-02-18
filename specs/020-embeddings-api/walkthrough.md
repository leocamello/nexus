# F17: Embeddings API — Code Walkthrough

**Feature**: Embeddings API (F17)  
**Audience**: Junior developers joining the project  
**Last Updated**: 2025-07-18

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: api/embeddings.rs — The Intake Desk](#file-1-apiembeddingsrs--the-intake-desk)
4. [File 2: agent/mod.rs — The Translation Contract](#file-2-agentmodrs--the-translation-contract)
5. [File 3: agent/openai.rs — The Express Counter](#file-3-agentopenairs--the-express-counter)
6. [File 4: agent/ollama.rs — The Specialist Window](#file-4-agentollamars--the-specialist-window)
7. [File 5: api/headers.rs — The Return Label](#file-5-apiheadersrs--the-return-label)
8. [Understanding the Tests](#understanding-the-tests)
9. [Key Rust Concepts](#key-rust-concepts)
10. [Common Patterns in This Codebase](#common-patterns-in-this-codebase)
11. [Next Steps](#next-steps)

---

## The Big Picture

Imagine Nexus is a **translation bureau**. Clients walk in with documents (text) and say "I need these translated into coordinates" — that is, converted from human-readable words into dense numeric vectors that machines can compare, cluster, and search. The bureau doesn't do the translation itself; it has specialist translators in the back (OpenAI, Ollama) who each speak different protocols. The bureau's job is to accept the request in a standard format, find the right translator, hand off the work, and return the result — all without the client knowing which translator was used.

That's what the **Embeddings API** (F17) does. It exposes a single `POST /v1/embeddings` endpoint that accepts OpenAI-formatted requests, routes them to the right backend via the same intelligent router used for chat completions, and returns OpenAI-formatted responses — regardless of whether the actual work was done by OpenAI's cloud API or a local Ollama instance.

### What Problem Does This Solve?

Without F17, a developer using Nexus for RAG (retrieval-augmented generation) or semantic search would need to maintain separate client configurations for each embedding backend. If their Ollama instance went down, they'd need to manually switch to OpenAI. If they wanted to keep sensitive documents local, they'd need to manage that routing themselves.

F17 makes embedding requests **backend-agnostic** — the same URL works regardless of which backend is available, and the existing router handles health checks, capability matching, and privacy zones automatically.

### How F17 Fits Into Nexus

```
┌──────────────────────────────────────────────────────────────────────────┐
│                              Nexus                                      │
│                                                                         │
│  Client Request                                                         │
│    │  POST /v1/embeddings                                               │
│    │  {                                                                  │
│    │    "model": "text-embedding-ada-002",                               │
│    │    "input": ["hello world", "goodbye"]                              │
│    │  }                                                                  │
│    ▼                                                                    │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │  ① Axum handler: handle()                          ◄── F17    │    │
│  │     Validates and normalizes the request:                      │    │
│  │     • Deserialize JSON → EmbeddingRequest                     │    │
│  │     • EmbeddingInput::into_vec() normalizes single/batch      │    │
│  │     • Validate: non-empty, ≤ 2048 batch size                  │    │
│  │     • Estimate tokens: sum(chars / 4)                         │    │
│  └──┼──────────────────────────────────────────────────────────────┘    │
│     │                                                                   │
│     ▼                                                                   │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │  ② Router::select_backend()                                    │    │
│  │     Same router used by chat completions:                      │    │
│  │     • Build RequestRequirements (no vision/tools/json needed)  │    │
│  │     • Alias resolution, capability filtering, scoring          │    │
│  │     • Returns backend + cost estimate                          │    │
│  └──┼──────────────────────────────────────────────────────────────┘    │
│     │                                                                   │
│     ▼                                                                   │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │  ③ Capability gate                                 ◄── F17    │    │
│  │     • Registry::get_agent() → get NII agent                   │    │
│  │     • Check agent.profile().capabilities.embeddings == true    │    │
│  │     • Track pending: increment_pending / decrement_pending     │    │
│  └──┼──────────────────────────────────────────────────────────────┘    │
│     │                                                                   │
│     ▼                                                                   │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │  ④ agent.embeddings(model, input)                  ◄── F17    │    │
│  │     Backend-specific HTTP call:                                │    │
│  │     • OpenAI: POST /v1/embeddings (batch, Bearer auth)        │    │
│  │     • Ollama: POST /api/embed (per-input iteration)           │    │
│  └──┼──────────────────────────────────────────────────────────────┘    │
│     │                                                                   │
│     ▼                                                                   │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │  ⑤ Build response                                 ◄── F17    │    │
│  │     • Vec<Vec<f32>> → EmbeddingResponse (OpenAI format)       │    │
│  │     • Inject X-Nexus-* transparent headers                     │    │
│  │     • Return 200 OK with JSON body                             │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                                                         │
│  Data Flow: Request JSON → EmbeddingRequest (types)                    │
│             → RequestRequirements (routing input)                      │
│             → InferenceAgent::embeddings() (backend call)              │
│             → EmbeddingResponse (OpenAI format)                        │
│             → X-Nexus-* headers (routing metadata)                     │
└──────────────────────────────────────────────────────────────────────────┘
```

### Key Design Decisions

| Decision | Why |
|----------|-----|
| OpenAI format as canonical wire format | Any OpenAI-compatible client works out of the box. Backend-specific formats (Ollama's `/api/embed`) are translated by the agent layer |
| `embeddings()` is an optional trait method | New agent types work without embeddings support. They return `AgentError::Unsupported` by default until explicitly overridden |
| Capability gate *after* routing | Router selects by model availability; the `capabilities.embeddings` check is a second-stage safety net |
| Batch size capped at 2048 | Matches OpenAI's limit. Prevents resource exhaustion from unbounded input arrays |
| `model` parameter forwarded to agent | Agent uses the client's requested model, not a hardcoded default. Supports multiple embedding models per backend |
| Pending request tracking | `increment_pending` / `decrement_pending` feeds load-aware routing so embedding requests count toward backend load |

---

## File Structure

```
src/api/
├── embeddings.rs            ← F17: Handler + types (334 lines, 8 tests)  NEW

src/agent/
├── mod.rs                   ← F17: embeddings() trait method (431 lines, 0 F17 tests)  MODIFIED
├── openai.rs                ← F17: OpenAI embeddings impl (876 lines, in agent tests)  MODIFIED
├── ollama.rs                ← F17: Ollama embeddings impl (844 lines, in agent tests)  MODIFIED

src/api/
├── headers.rs               ← F12: NexusTransparentHeaders (208 lines, 2 tests)  EXISTING

tests/
├── embeddings_test.rs       ← F17: Integration tests (146 lines, 5 tests)  NEW
```

**F17 Contribution**: 1 new handler file (`embeddings.rs`), 1 new integration test file, 3 modified files (trait + 2 agents). ~480 lines added, 8 unit tests, 5 integration tests.

---

## File 1: api/embeddings.rs — The Intake Desk

**Purpose**: The front desk of the translation bureau — accepts embedding requests, validates them, routes to the right translator, and assembles the response.  
**Lines**: 334  |  **Tests**: 8  |  **Status**: NEW

### Why Does This Exist?

Every embedding request enters Nexus through this handler. It's responsible for the entire lifecycle: deserializing the request, validating input, routing to a backend, delegating to the right agent, building the OpenAI-compatible response, and injecting transparent headers. Without this file, Nexus would have no embeddings endpoint.

The critical design constraint: **stay OpenAI-compatible**. The request and response formats must match OpenAI's `/v1/embeddings` spec exactly, so any client library (LangChain, LlamaIndex, etc.) works without modification.

### The Types

```rust
// src/api/embeddings.rs (lines 17–65)

/// Input format — OpenAI accepts both a single string and an array
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    Single(String),
    Batch(Vec<String>),
}

impl EmbeddingInput {
    /// Convert to a Vec<String> regardless of variant.
    pub fn into_vec(self) -> Vec<String> {
        match self {
            EmbeddingInput::Single(s) => vec![s],
            EmbeddingInput::Batch(v) => v,
        }
    }
}

/// Embedding request matching OpenAI format.
pub struct EmbeddingRequest {
    pub model: String,
    pub input: EmbeddingInput,
    pub encoding_format: Option<String>,  // e.g. "float"
}

/// A single embedding object in the response.
pub struct EmbeddingObject {
    pub object: String,       // always "embedding"
    pub embedding: Vec<f32>,  // the actual vector
    pub index: usize,         // position in batch
}

/// Embedding response matching OpenAI format.
pub struct EmbeddingResponse {
    pub object: String,            // always "list"
    pub data: Vec<EmbeddingObject>,
    pub model: String,
    pub usage: EmbeddingUsage,
}
```

`EmbeddingInput` uses `#[serde(untagged)]` to accept both `"input": "hello"` and `"input": ["hello", "world"]` — the two formats OpenAI supports. The `into_vec()` method normalizes both into `Vec<String>` so the handler doesn't care which format the client used.

### The Handler

```rust
// src/api/embeddings.rs (lines 68–189)

const MAX_EMBEDDING_BATCH_SIZE: usize = 2048;

pub async fn handle(
    State(state): State<Arc<AppState>>,
    _headers: HeaderMap,
    Json(request): Json<EmbeddingRequest>,
) -> Result<Response, ApiError> {
    info!(model = %request.model, "Embedding request");

    // ── Validation ──────────────────────────────────────────────
    let input_texts = request.input.into_vec();
    if input_texts.is_empty() {
        return Err(ApiError::bad_request("Input must not be empty"));
    }
    if input_texts.len() > MAX_EMBEDDING_BATCH_SIZE {
        return Err(ApiError::bad_request(&format!(
            "Batch size {} exceeds maximum of {}",
            input_texts.len(), MAX_EMBEDDING_BATCH_SIZE
        )));
    }

    // ── Token estimation for routing ────────────────────────────
    let estimated_tokens: u32 = input_texts.iter().map(|s| s.len() as u32 / 4).sum();

    // ── Route to a backend ──────────────────────────────────────
    let requirements = RequestRequirements {
        model: request.model.clone(),
        estimated_tokens,
        needs_vision: false,       // embeddings don't need any
        needs_tools: false,        // special capabilities —
        needs_json_mode: false,    // just model availability
        prefers_streaming: false,
    };

    let routing_result = state.router.select_backend(&requirements, None)
        .map_err(|e| match e {
            RoutingError::ModelNotFound { model } =>
                ApiError::model_not_found(&model, &[]),
            RoutingError::NoHealthyBackend { model } =>
                ApiError::service_unavailable(&format!(
                    "No healthy backend available for model '{}'", model)),
            _ => ApiError::bad_gateway(&format!("Routing error: {}", e)),
        })?;

    let backend = &routing_result.backend;

    // ── Capability gate ─────────────────────────────────────────
    let agent = state.registry.get_agent(&backend.id).ok_or_else(|| {
        ApiError::bad_gateway(&format!("No agent registered for backend '{}'", backend.id))
    })?;

    if !agent.profile().capabilities.embeddings {
        return Err(ApiError::service_unavailable(&format!(
            "Backend '{}' does not support embeddings", backend.id
        )));
    }

    // ── Delegate to agent (with pending tracking) ───────────────
    let _ = state.registry.increment_pending(&backend.id);

    let vectors = agent.embeddings(&request.model, input_texts).await
        .map_err(|e| {
            let _ = state.registry.decrement_pending(&backend.id);
            ApiError::from_agent_error(e)
        })?;

    let _ = state.registry.decrement_pending(&backend.id);

    // ── Build OpenAI-compatible response ────────────────────────
    let data: Vec<EmbeddingObject> = vectors.into_iter().enumerate()
        .map(|(i, embedding)| EmbeddingObject {
            object: "embedding".to_string(),
            embedding,
            index: i,
        })
        .collect();

    let response = EmbeddingResponse {
        object: "list".to_string(),
        data,
        model: request.model,
        usage: EmbeddingUsage {
            prompt_tokens: estimated_tokens,
            total_tokens: estimated_tokens,
        },
    };

    let mut resp = Json(response).into_response();

    // ── Inject X-Nexus-* transparent headers ────────────────────
    let privacy_zone = agent.profile().privacy_zone;
    let nexus_headers = NexusTransparentHeaders::new(
        backend.id.clone(),
        backend.backend_type,
        RouteReason::CapabilityMatch,
        privacy_zone,
        routing_result.cost_estimated,
    );
    nexus_headers.inject_into_response(&mut resp);

    Ok(resp)
}
```

Let's trace through the handler step by step:

1. **Validation**: Normalize input via `into_vec()`, reject empty input and batches larger than 2048.
2. **Token estimation**: The `chars / 4` heuristic — same one used throughout Nexus. This is for routing decisions, not billing.
3. **Routing**: Build `RequestRequirements` with all capability flags set to `false` (embeddings don't need vision, tools, or JSON mode). The router picks a backend based on model availability and health.
4. **Capability gate**: After routing finds a backend, verify its agent actually supports embeddings. This two-stage approach means the router stays generic.
5. **Pending tracking**: `increment_pending()` before the call, `decrement_pending()` after (including on error). This feeds load-aware routing so embedding requests count toward backend load.
6. **Delegation**: Call `agent.embeddings(&request.model, input_texts)` — the model parameter is forwarded from the client request, not hardcoded.
7. **Response assembly**: Convert `Vec<Vec<f32>>` into `EmbeddingResponse` with proper indexing and token usage.
8. **Header injection**: Add X-Nexus-* headers so clients can see which backend served the request.

### Key Tests

```rust
#[test]
fn embedding_request_deserialize_single_input() {
    let json = r#"{"model":"text-embedding-ada-002","input":"hello world"}"#;
    let req: EmbeddingRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.model, "text-embedding-ada-002");
    match &req.input {
        EmbeddingInput::Single(s) => assert_eq!(s, "hello world"),
        _ => panic!("Expected Single variant"),
    }
}

#[test]
fn embedding_response_serialization_matches_openai() {
    let response = EmbeddingResponse {
        object: "list".to_string(),
        data: vec![EmbeddingObject {
            object: "embedding".to_string(),
            embedding: vec![0.1, 0.2, 0.3],
            index: 0,
        }],
        model: "text-embedding-ada-002".to_string(),
        usage: EmbeddingUsage { prompt_tokens: 10, total_tokens: 10 },
    };

    let json = serde_json::to_value(&response).unwrap();
    assert_eq!(json["object"], "list");
    assert_eq!(json["data"][0]["object"], "embedding");
    assert_eq!(json["usage"]["prompt_tokens"], 10);
    // ◄── Verifies wire format matches OpenAI exactly
}

#[test]
fn embedding_object_serialization() {
    let obj = EmbeddingObject {
        object: "embedding".to_string(),
        embedding: vec![0.0; 1536],  // standard OpenAI dimension
        index: 0,
    };
    let json = serde_json::to_value(&obj).unwrap();
    assert_eq!(json["embedding"].as_array().unwrap().len(), 1536);
    // ◄── 1536-dim vectors serialize without truncation
}
```

The tests focus on serialization correctness — verifying that Nexus produces JSON that any OpenAI client library can parse. The `embedding_object_serialization` test with a 1536-dimensional vector is particularly important since that's OpenAI's standard embedding size.

---

## File 2: agent/mod.rs — The Translation Contract

**Purpose**: Defines the contract that every translator (agent) must follow — including the optional `embeddings()` method.  
**Lines**: 431  |  **F17 Addition**: ~10 lines  |  **Status**: MODIFIED

### Why Does This Exist?

The `InferenceAgent` trait is the heart of Nexus's backend abstraction. Every backend type (Ollama, OpenAI, LM Studio, etc.) implements this trait. F17 adds `embeddings()` as an **optional capability** — agents that don't support embeddings get a free default implementation that returns `AgentError::Unsupported`.

This is the "translation contract" — it defines what services the bureau offers, and each translator decides which services they can perform.

### The Trait Method

```rust
// src/agent/mod.rs (lines 209–219)

/// Generate embeddings for input text (F17: Embeddings, v0.4).
///
/// Default implementation returns `Unsupported`. Override in OpenAIAgent
/// and backends that support /v1/embeddings endpoint.
async fn embeddings(
    &self,
    _model: &str,
    _input: Vec<String>,
) -> Result<Vec<Vec<f32>>, AgentError> {
    Err(AgentError::Unsupported("embeddings"))
}
```

Key design decisions in this signature:

- **`_model: &str`**: The model name is forwarded from the client request. This lets agents support multiple embedding models (e.g., `text-embedding-ada-002` vs `text-embedding-3-small`) without hardcoding.
- **`_input: Vec<String>`**: Already normalized by the handler — agents don't need to handle single-vs-batch.
- **`Vec<Vec<f32>>`**: One vector per input string. The handler wraps these in `EmbeddingObject` structs for the response.
- **Default returns `Unsupported`**: New agent types (e.g., `GenericOpenAIAgent`) automatically decline embedding requests. The handler's capability gate catches this before the call is even made.

### Key Tests

The trait method itself has no tests — it's tested indirectly through the concrete implementations in `openai.rs` and `ollama.rs`, and through the integration tests that exercise the full request flow.

---

## File 3: agent/openai.rs — The Express Counter

**Purpose**: The OpenAI agent's implementation of `embeddings()` — sends the entire batch to OpenAI's cloud API in a single HTTP request.  
**Lines**: 876  |  **F17 Addition**: ~70 lines  |  **Status**: MODIFIED

### Why Does This Exist?

OpenAI's `/v1/embeddings` endpoint natively supports batch embedding — you send an array of strings and get back an array of vectors. This is the "express counter" of the translation bureau: hand over everything at once, get everything back at once. No iteration needed.

### The Implementation

```rust
// src/agent/openai.rs (lines 357–426)

async fn embeddings(
    &self,
    model: &str,
    input: Vec<String>,
) -> Result<Vec<Vec<f32>>, AgentError> {
    let url = format!("{}/v1/embeddings", self.base_url);

    let body = serde_json::json!({
        "model": model,        // ◄── forwarded from client request
        "input": input,        // ◄── entire batch in one request
    });

    let response = self.client
        .post(&url)
        .header("authorization", format!("Bearer {}", self.api_key))
        .json(&body)
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                AgentError::Timeout(60000)
            } else {
                AgentError::Network(e.to_string())
            }
        })?;

    let status = response.status();
    if !status.is_success() {
        let error_body = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(AgentError::Upstream {
            status: status.as_u16(),
            message: error_body,
        });
    }

    let body: serde_json::Value = response.json().await.map_err(|e| {
        AgentError::InvalidResponse(format!(
            "Failed to parse OpenAI embeddings response: {}", e
        ))
    })?;

    let data = body["data"].as_array().ok_or_else(|| {
        AgentError::InvalidResponse(
            "Missing data array in OpenAI embeddings response".to_string(),
        )
    })?;

    let mut results = Vec::with_capacity(data.len());
    for item in data {
        let embedding = item["embedding"].as_array().ok_or_else(|| {
            AgentError::InvalidResponse(
                "Missing embedding array in response item".to_string(),
            )
        })?;

        let vector: Vec<f32> = embedding.iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        results.push(vector);
    }

    Ok(results)
}
```

Let's trace through this implementation:

1. **URL construction**: Appends `/v1/embeddings` to the configured base URL.
2. **Body**: Uses the `model` parameter from the client request (not hardcoded). Sends the entire `input` array in one request — OpenAI handles batching natively.
3. **Authentication**: Bearer token from the agent's configured API key.
4. **Timeout**: 60 seconds — embedding large batches can take time.
5. **Error handling**: Distinguishes timeout (`AgentError::Timeout`) from network errors (`AgentError::Network`) from upstream failures (`AgentError::Upstream` with status code).
6. **Response parsing**: Navigates `response.data[i].embedding` arrays, converting each `f64` JSON number to `f32`.

### Key Tests

The OpenAI agent's embedding tests live in the agent test module and use mock HTTP servers to verify:
- Successful batch embedding returns the correct number of vectors
- Authentication headers are included in the request
- Timeout and error responses are properly mapped to `AgentError` variants

---

## File 4: agent/ollama.rs — The Specialist Window

**Purpose**: The Ollama agent's implementation of `embeddings()` — sends one request per input text because Ollama's native API doesn't support batch embedding.  
**Lines**: 844  |  **F17 Addition**: ~70 lines  |  **Status**: MODIFIED

### Why Does This Exist?

Ollama uses a different endpoint (`/api/embed`) with a different response format (`{ "embeddings": [[...]] }`). It also doesn't natively support batch input the way OpenAI does. This is the "specialist window" — it handles one document at a time, carefully, using Ollama's native protocol.

### The Implementation

```rust
// src/agent/ollama.rs (lines 291–357)

async fn embeddings(
    &self,
    model: &str,
    input: Vec<String>,
) -> Result<Vec<Vec<f32>>, AgentError> {
    let mut results = Vec::with_capacity(input.len());

    for text in &input {
        let url = format!("{}/api/embed", self.base_url);
        let body = serde_json::json!({
            "model": model,        // ◄── forwarded from client request
            "input": text,         // ◄── one input at a time
        });

        let response = self.client
            .post(&url)
            .json(&body)
            .timeout(Duration::from_secs(60))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AgentError::Timeout(60000)
                } else {
                    AgentError::Network(e.to_string())
                }
            })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let error_body = response.text().await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AgentError::Upstream { status, message: error_body });
        }

        let body: serde_json::Value = response.json().await.map_err(|e| {
            AgentError::InvalidResponse(format!(
                "Failed to parse Ollama embed response: {}", e
            ))
        })?;

        // Ollama returns { "embeddings": [[...]] }
        let embeddings = body["embeddings"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                AgentError::InvalidResponse(
                    "Missing embeddings array in Ollama response".to_string(),
                )
            })?;

        let vector: Vec<f32> = embeddings.iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        results.push(vector);
    }

    Ok(results)
}
```

Key differences from the OpenAI implementation:

| Aspect | OpenAI | Ollama |
|--------|--------|--------|
| Endpoint | `/v1/embeddings` | `/api/embed` |
| Batch support | Native (all inputs in one request) | Per-input iteration (N requests for N inputs) |
| Response format | `{ "data": [{"embedding": [...]}] }` | `{ "embeddings": [[...]] }` |
| Authentication | Bearer token | None (local backend) |
| Response navigation | `data[i].embedding` | `embeddings[0]` (first array element) |

The per-input iteration is a trade-off:
- **Pro**: Works with Ollama's native API without modifications
- **Con**: N HTTP requests for N inputs (higher latency for large batches)
- **Future**: If Ollama adds batch support, only this method needs updating — the handler stays unchanged

---

## File 5: api/headers.rs — The Return Label

**Purpose**: Stamps every response with routing metadata — which backend handled the request, why it was chosen, and what privacy zone it belongs to.  
**Lines**: 208  |  **Tests**: 2  |  **Status**: EXISTING (used by F17)

### Why Does This Exist?

Nexus never modifies the JSON response body (Constitution Principle III — OpenAI compatibility). All routing metadata goes into HTTP headers with the `X-Nexus-*` prefix. This is the "return label" — when a translated document comes back, the label tells you which translator handled it, without opening the envelope.

### The Implementation

The embeddings handler constructs and injects these headers after building the response:

```rust
// src/api/embeddings.rs (lines 177–187)

let privacy_zone = agent.profile().privacy_zone;
let nexus_headers = NexusTransparentHeaders::new(
    backend.id.clone(),            // X-Nexus-Backend: "ollama-local"
    backend.backend_type,          // X-Nexus-Backend-Type: "local" or "cloud"
    RouteReason::CapabilityMatch,  // X-Nexus-Route-Reason: "capability-match"
    privacy_zone,                  // X-Nexus-Privacy-Zone: "restricted" or "open"
    routing_result.cost_estimated, // X-Nexus-Cost-Estimated: "0.0042" (cloud only)
);
nexus_headers.inject_into_response(&mut resp);
```

The `inject_into_response` method (in `headers.rs`, line 120) inserts all five headers into the response. Backend type is classified as `"local"` or `"cloud"` based on the `BackendType` enum — Ollama, vLLM, LlamaCpp, Exo, LMStudio, and Generic are `"local"`; OpenAI, Anthropic, and Google are `"cloud"`.

Example response headers:

```http
HTTP/1.1 200 OK
X-Nexus-Backend: openai-prod
X-Nexus-Backend-Type: cloud
X-Nexus-Route-Reason: capability-match
X-Nexus-Privacy-Zone: open
X-Nexus-Cost-Estimated: 0.0001
Content-Type: application/json

{"object":"list","data":[{"object":"embedding","embedding":[0.1,0.2,...],...}],...}
```

---

## Understanding the Tests

### Test Helpers

The integration tests use a shared `common` module that provides `make_app_with_mock()` — a helper that creates a fully wired Axum app backed by a `wiremock::MockServer`:

```rust
// tests/embeddings_test.rs
use common::make_app_with_mock;

let mock_server = wiremock::MockServer::start().await;
let (mut app, _registry) = make_app_with_mock(&mock_server).await;
```

This gives each test a clean app instance with a mock backend, so tests can control exactly what the "backend" returns.

### Test Organization

| Module | File | Test Count | What It Covers |
|--------|------|------------|----------------|
| `api::embeddings::tests` | `src/api/embeddings.rs` | 8 | Serialization/deserialization of all embedding types |
| Integration tests | `tests/embeddings_test.rs` | 5 | Full HTTP stack with mock backends |

### Testing Patterns

**Pattern 1: Serialization Round-Trip**

Many unit tests verify that types serialize and deserialize correctly — critical for OpenAI compatibility:

```rust
#[test]
fn embedding_response_roundtrip() {
    let response = EmbeddingResponse {
        object: "list".to_string(),
        data: vec![EmbeddingObject {
            object: "embedding".to_string(),
            embedding: vec![1.0, 2.0],
            index: 0,
        }],
        model: "test-model".to_string(),
        usage: EmbeddingUsage { prompt_tokens: 5, total_tokens: 5 },
    };

    let json = serde_json::to_string(&response).unwrap();
    let deserialized: EmbeddingResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.object, "list");
    assert_eq!(deserialized.data[0].embedding, vec![1.0, 2.0]);
    // ◄── Serialize → deserialize → same values: no data loss
}
```

This pattern ensures that every field survives the JSON round-trip — if serde skips a field or changes a type, this test catches it.

**Pattern 2: Route Existence Verification**

Integration tests verify the endpoint exists and responds with appropriate status codes, without needing a fully functional backend:

```rust
#[tokio::test]
async fn embeddings_route_exists() {
    let mock_server = wiremock::MockServer::start().await;
    let (mut app, _registry) = make_app_with_mock(&mock_server).await;

    let request = Request::builder()
        .method("POST")
        .uri("/v1/embeddings")
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = app.call(request).await.unwrap();

    // Route exists — should not be 404 or 405
    assert_ne!(response.status(), StatusCode::NOT_FOUND);
    assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    // ◄── Verifies the route is wired up, regardless of handler outcome
}
```

**Pattern 3: Negative Testing**

Each error path gets its own test, verifying the handler returns the right HTTP status:

```rust
#[tokio::test]
async fn embeddings_model_not_found_returns_error() {
    let mock_server = wiremock::MockServer::start().await;
    let (mut app, _registry) = make_app_with_mock(&mock_server).await;

    let body = serde_json::json!({
        "model": "nonexistent-model",
        "input": "hello"
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/embeddings")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.call(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    // ◄── Unknown model → 404, not 500 or 502
}
```

---

## Key Rust Concepts

### 1. `#[serde(untagged)]` for Flexible Input

```rust
#[derive(Deserialize, Serialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    Single(String),
    Batch(Vec<String>),
}
```

`#[serde(untagged)]` tells serde to try deserializing each variant in order without expecting a type tag. For `"input": "hello"` it tries `Single(String)` first — success. For `"input": ["hello", "world"]` it tries `Single` (fails), then `Batch(Vec<String>)` — success. This mirrors how OpenAI's API accepts both formats.

### 2. `#[serde(skip_serializing_if)]` for Optional Fields

```rust
pub struct EmbeddingRequest {
    pub model: String,
    pub input: EmbeddingInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding_format: Option<String>,
}
```

When `encoding_format` is `None`, serde omits the field entirely from the JSON output instead of writing `"encoding_format": null`. This keeps the serialized JSON clean and matches what OpenAI clients expect.

### 3. `Arc<AppState>` for Shared State

```rust
pub async fn handle(
    State(state): State<Arc<AppState>>,
    // ...
```

Axum extracts the shared application state via the `State` extractor. `Arc` (Atomic Reference Counting) lets multiple handler tasks share the same state without copying it. The registry, router, and HTTP client inside `AppState` are all thread-safe, so concurrent embedding requests don't conflict.

### 4. `filter_map()` for Lossy Conversions

```rust
let vector: Vec<f32> = embedding.iter()
    .filter_map(|v| v.as_f64().map(|f| f as f32))
    .collect();
```

JSON numbers are parsed as `f64`. We need `f32` vectors (standard for embeddings). `filter_map()` combines filtering and mapping: `as_f64()` returns `None` for non-numeric values, which `filter_map()` silently skips. The `f64 as f32` cast loses precision but is acceptable — embedding vectors don't need 64-bit precision.

### 5. Error Mapping with `map_err()`

```rust
let vectors = agent.embeddings(&request.model, input_texts).await
    .map_err(|e| {
        let _ = state.registry.decrement_pending(&backend.id);
        ApiError::from_agent_error(e)
    })?;
```

`map_err()` transforms the error type while also performing cleanup. Here, if the embedding call fails, we decrement the pending counter *before* converting the `AgentError` into an `ApiError`. The `?` operator then propagates the mapped error. This pattern ensures cleanup runs on both success and failure paths.

---

## Common Patterns in This Codebase

### 1. The NII Agent Pattern

Every backend interaction goes through the `InferenceAgent` trait. The handler never needs to know which backend type it's talking to:

```rust
// Handler doesn't care if this is OpenAI or Ollama
let agent = state.registry.get_agent(&backend.id)?;
let vectors = agent.embeddings(&request.model, input_texts).await?;
```

Adding a new backend that supports embeddings requires only two changes:
1. Set `capabilities.embeddings: true` in `profile()`
2. Override `embeddings()` with backend-specific HTTP logic

The handler, router, and all other infrastructure work automatically.

### 2. The Validate → Route → Gate → Delegate Pattern

The embeddings handler follows the same pattern as chat completions:

```
Validate input format
  │
  ▼
Route via Router::select_backend()
  │
  ▼
Gate via capabilities check
  │
  ▼
Delegate via agent.embeddings()
  │
  ▼
Assemble response + headers
```

This separation means validation logic doesn't know about routing, routing doesn't know about agents, and agents don't know about HTTP response formatting. Each stage is independently testable and replaceable.

### 3. The Transparent Headers Pattern

Every response — chat completions, embeddings, models list — gets X-Nexus-* headers injected. The pattern is always the same:

```rust
let nexus_headers = NexusTransparentHeaders::new(
    backend.id.clone(),
    backend.backend_type,
    RouteReason::CapabilityMatch,
    privacy_zone,
    routing_result.cost_estimated,
);
nexus_headers.inject_into_response(&mut resp);
```

This single injection point ensures consistency across all endpoints. The JSON body is never modified — only headers are added.

### 4. The Pending Counter Pattern

Load-aware routing needs to know how many requests each backend is currently processing:

```rust
let _ = state.registry.increment_pending(&backend.id);   // before call

let result = agent.embeddings(&request.model, input_texts).await
    .map_err(|e| {
        let _ = state.registry.decrement_pending(&backend.id);  // on error
        ApiError::from_agent_error(e)
    })?;

let _ = state.registry.decrement_pending(&backend.id);   // on success
```

The counter is decremented on both success and error paths. The `let _ =` pattern acknowledges that the `Result` is intentionally ignored — if pending tracking fails, the request should still proceed.

---

## Next Steps

After understanding F17, here's what to explore next:

1. **F15: Speculative Router** — The `RequestRequirements` struct and capability filtering that F17 reuses for routing (see `specs/018-speculative-router/walkthrough.md`)
2. **F12: Transparent Protocol** — The full `NexusTransparentHeaders` system that F17 uses to inject routing metadata (see `src/api/headers.rs`)
3. **Adding a new agent**: Try adding embeddings support to `GenericOpenAIAgent` — set the capability flag and override `embeddings()` (see pattern in File 3)

### Questions to Investigate

- What happens if a client sends `"encoding_format": "base64"` instead of `"float"`? (Hint: the `encoding_format` field is accepted but not used by the handler — it's forwarded to the agent)
- Why does the handler decrement pending in *both* the error closure and after success? (Hint: the `?` operator would skip the post-success decrement on error)
- How would you add dimension reduction (e.g., OpenAI's `dimensions` parameter)? (Hint: add it to `EmbeddingRequest`, forward it in the agent's request body, no handler logic changes needed)
