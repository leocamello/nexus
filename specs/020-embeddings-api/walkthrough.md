# F17: Embeddings API — Code Walkthrough

> A junior-developer-friendly guide to how the `/v1/embeddings` endpoint works in Nexus.

## 1. Architecture Overview

Nexus exposes an OpenAI-compatible embeddings endpoint at `POST /v1/embeddings`. It follows the same patterns as chat completions:

```
Client  ──POST /v1/embeddings──▶  Axum Handler  ──Router──▶  Agent  ──▶  Backend
                                  (embeddings.rs)             (NII trait)   (Ollama / OpenAI)
```

**Key architectural concepts:**

- **NII (Nexus Inference Interface):** Every backend implements the `InferenceAgent` trait. The `embeddings()` method is an *optional capability* — it returns `AgentError::Unsupported` by default. Only agents that actually support embeddings (OpenAI, Ollama) override it.
- **Unified Router:** The same `Router::select_backend()` used for chat completions also handles embedding requests. The handler builds `RequestRequirements` and lets the router pick the best backend.
- **Capability gating:** After routing, the handler checks `agent.profile().capabilities.embeddings` before calling `agent.embeddings()`. This prevents routing to a backend that advertises a model but can't embed it.

## 2. Request Flow

Here's what happens step-by-step when a client sends `POST /v1/embeddings`:

```
┌─────────────────────────────────────────────────────────────────┐
│  1. Axum deserializes JSON into EmbeddingRequest                │
│  2. Validate input is non-empty                                 │
│  3. Estimate tokens: sum(input.len() / 4) — chars/4 heuristic  │
│  4. Build RequestRequirements (no vision/tools/json needed)     │
│  5. Router::select_backend() → picks backend by model + health │
│  6. Registry::get_agent() → get the NII agent for that backend │
│  7. Check agent.profile().capabilities.embeddings == true       │
│  8. agent.embeddings(input_texts) → backend-specific HTTP call  │
│  9. Build OpenAI-compatible EmbeddingResponse                   │
│ 10. Inject X-Nexus-* transparent headers                        │
└─────────────────────────────────────────────────────────────────┘
```

### Error cases at each step

| Step | Failure | HTTP Status |
|------|---------|-------------|
| 1 | Invalid JSON | 400 Bad Request |
| 2 | Empty input array | 400 Bad Request |
| 5 | Model not found | 404 Not Found |
| 5 | No healthy backend | 503 Service Unavailable |
| 6 | No agent registered | 502 Bad Gateway |
| 7 | Agent doesn't support embeddings | 503 Service Unavailable |
| 8 | Backend returns error | Mapped from `AgentError` |

## 3. Key Files

### `src/api/embeddings.rs` — Handler + Types

This is the entry point. It defines the request/response types and the handler function.

**Types:**

```rust
// Input accepts both single string and array (OpenAI-compatible)
#[derive(Deserialize, Serialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    Single(String),
    Batch(Vec<String>),
}

// The request — mirrors OpenAI's format
pub struct EmbeddingRequest {
    pub model: String,
    pub input: EmbeddingInput,
    pub encoding_format: Option<String>,  // optional, e.g. "float"
}

// The response — also mirrors OpenAI's format
pub struct EmbeddingResponse {
    pub object: String,            // always "list"
    pub data: Vec<EmbeddingObject>,
    pub model: String,
    pub usage: EmbeddingUsage,
}
```

`EmbeddingInput::into_vec()` normalizes both variants into `Vec<String>` so the rest of the handler doesn't care which format the client used.

**Handler highlights:**

```rust
// Token estimation — simple chars/4 heuristic
let estimated_tokens: u32 = input_texts.iter().map(|s| s.len() as u32 / 4).sum();

// Capability gate — checked AFTER routing
if !agent.profile().capabilities.embeddings {
    return Err(ApiError::service_unavailable(...));
}

// Delegate to the agent — backend-specific logic lives there
let vectors = agent.embeddings(input_texts.clone()).await?;
```

### `src/agent/mod.rs` — InferenceAgent Trait

The trait defines `embeddings()` as an optional method with a default that returns `Unsupported`:

```rust
#[async_trait]
pub trait InferenceAgent: Send + Sync + 'static {
    // ... required methods (health_check, list_models, chat_completion, etc.)

    /// Default: returns Unsupported. Override in agents that support embeddings.
    async fn embeddings(&self, _input: Vec<String>) -> Result<Vec<Vec<f32>>, AgentError> {
        Err(AgentError::Unsupported("embeddings"))
    }
}
```

This means you can add a new agent type without worrying about embeddings — it just won't support them until you explicitly override the method.

### `src/agent/openai.rs` — OpenAI Agent

OpenAI supports native batch embedding. The agent sends all inputs in a single request:

```rust
async fn embeddings(&self, input: Vec<String>) -> Result<Vec<Vec<f32>>, AgentError> {
    let url = format!("{}/v1/embeddings", self.base_url);

    let body = serde_json::json!({
        "model": "text-embedding-ada-002",
        "input": input,   // ← entire batch in one request
    });

    let response = self.client
        .post(&url)
        .header("authorization", format!("Bearer {}", self.api_key))
        .json(&body)
        .timeout(Duration::from_secs(60))
        .send()
        .await?;

    // Parse response.data[].embedding arrays into Vec<Vec<f32>>
    // ...
}
```

**Key details:**
- Uses Bearer token authentication from config
- 60-second timeout
- Parses `response.data[i].embedding` arrays into `Vec<f32>` vectors
- The profile advertises `capabilities.embeddings: true`

### `src/agent/ollama.rs` — Ollama Agent

Ollama's `/api/embed` endpoint doesn't support batch input the same way, so the agent iterates per input:

```rust
async fn embeddings(&self, input: Vec<String>) -> Result<Vec<Vec<f32>>, AgentError> {
    let mut results = Vec::with_capacity(input.len());

    for text in &input {
        let url = format!("{}/api/embed", self.base_url);
        let body = serde_json::json!({
            "model": "all-minilm",
            "input": text,   // ← one input at a time
        });

        let response = self.client.post(&url).json(&body).send().await?;

        // Ollama returns { "embeddings": [[...]] }
        let vector = parse_ollama_embeddings(response)?;
        results.push(vector);
    }

    Ok(results)
}
```

**Key differences from OpenAI:**
- Uses Ollama's native `/api/embed` endpoint (not `/v1/embeddings`)
- Iterates per input text (N HTTP requests for N inputs)
- Response format is `{ "embeddings": [[...]] }` — different from OpenAI's `{ "data": [{"embedding": [...]}] }`
- No auth header needed (local backend)

### `src/api/headers.rs` — X-Nexus-* Transparent Headers

After building the response, the handler injects routing metadata headers:

```rust
let nexus_headers = NexusTransparentHeaders::new(
    backend.id.clone(),         // X-Nexus-Backend: "ollama-local"
    backend.backend_type,       // X-Nexus-Backend-Type: "local" or "cloud"
    RouteReason::CapabilityMatch,  // X-Nexus-Route-Reason
    privacy_zone,               // X-Nexus-Privacy-Zone: "restricted" or "open"
    routing_result.cost_estimated, // X-Nexus-Cost-Estimated (cloud only)
);
nexus_headers.inject_into_response(&mut resp);
```

These headers let clients see *which* backend served the request without Nexus modifying the JSON response body (Constitution Principle III).

## 4. Design Decisions

### Why OpenAI format as standard?

Nexus uses OpenAI's embedding request/response format as the canonical wire format. This means any OpenAI-compatible client works out of the box. Backend-specific formats (like Ollama's `/api/embed`) are translated by the agent layer.

### Why capability-based routing?

Not every backend supports embeddings. The `AgentCapabilities.embeddings` flag lets the system know which backends can handle embedding requests. This is checked *after* routing (not during) because the router selects based on model availability, and the capability check is a second-stage filter.

```rust
// In AgentProfile — set per agent type
pub struct AgentCapabilities {
    pub embeddings: bool,       // OpenAI: true, Ollama: true, Generic: false
    pub model_lifecycle: bool,
    pub token_counting: bool,
    pub resource_monitoring: bool,
}
```

### Why does Ollama iterate per-input?

Ollama's `/api/embed` endpoint accepts a single input string and returns one embedding. To support batch requests (OpenAI format allows `"input": ["a", "b", "c"]`), the Ollama agent sends N sequential requests. This is a trade-off:

- **Pro:** Works with Ollama's native API without changes
- **Con:** N requests for N inputs (higher latency for large batches)
- **Future:** If Ollama adds batch support, the agent can be updated without changing the handler

### Why chars/4 for token estimation?

The handler estimates tokens as `text.len() / 4` (the standard heuristic used throughout Nexus). This is used for routing decisions, not billing. OpenAI's agent has exact tiktoken counting, but the embedding handler uses the heuristic for simplicity since exact token counts aren't critical for embedding routing.

### Why X-Nexus-* headers?

Nexus never modifies the JSON response body (OpenAI compatibility). All routing metadata goes into HTTP headers. The embeddings endpoint uses `RouteReason::CapabilityMatch` since the backend was selected for having the requested model and embedding capability.

## 5. Testing

### Unit Tests (8 tests in `src/api/embeddings.rs`)

These test serialization/deserialization of the embedding types:

| Test | What it verifies |
|------|-----------------|
| `embedding_request_deserialize_single_input` | `"input": "hello"` parses as `Single` variant |
| `embedding_request_deserialize_batch_input` | `"input": ["a","b"]` parses as `Batch` variant |
| `embedding_request_with_encoding_format` | Optional `encoding_format` field |
| `embedding_input_into_vec_single` | `Single("x").into_vec()` → `vec!["x"]` |
| `embedding_input_into_vec_batch` | `Batch(["a","b"]).into_vec()` → `vec!["a","b"]` |
| `embedding_response_serialization_matches_openai` | Response JSON matches OpenAI format |
| `embedding_response_roundtrip` | Serialize → deserialize → same values |
| `embedding_object_serialization` | 1536-dim vector serializes correctly |

### Integration Tests (5 tests in `tests/embeddings_test.rs`)

These test the full HTTP stack with mock backends:

| Test | What it verifies |
|------|-----------------|
| `embeddings_route_exists` | `POST /v1/embeddings` returns non-404/405 |
| `embeddings_returns_valid_response` | With mock backend, returns 200/503/502 |
| `embeddings_model_not_found_returns_error` | Unknown model → 404 |
| `embeddings_batch_input_accepted` | Array input format accepted (not 400) |
| `embeddings_invalid_json_returns_422` | Malformed JSON → 400 |

### Running the tests

```bash
# All embedding tests (unit + integration)
cargo test embeddings

# Unit tests only
cargo test api::embeddings::tests

# Integration tests only
cargo test --test embeddings_test

# Single test
cargo test embedding_request_deserialize_single_input
```

## 6. Common Modifications

### Adding embeddings support to a new agent

Say you're adding a new agent type (e.g., `VLLMAgent`) and want it to support embeddings:

**Step 1:** Set the capability flag in `profile()`:

```rust
// src/agent/vllm.rs
fn profile(&self) -> AgentProfile {
    AgentProfile {
        backend_type: "vllm".to_string(),
        version: None,
        privacy_zone: self.privacy_zone,
        capabilities: AgentCapabilities {
            embeddings: true,  // ← Enable embeddings
            model_lifecycle: false,
            token_counting: false,
            resource_monitoring: false,
        },
        capability_tier: self.capability_tier,
    }
}
```

**Step 2:** Override `embeddings()` in your `InferenceAgent` implementation:

```rust
async fn embeddings(&self, input: Vec<String>) -> Result<Vec<Vec<f32>>, AgentError> {
    // 1. Build the HTTP request for your backend's embedding endpoint
    let url = format!("{}/v1/embeddings", self.base_url);
    let body = serde_json::json!({
        "model": "your-embedding-model",
        "input": input,
    });

    // 2. Send the request
    let response = self.client
        .post(&url)
        .json(&body)
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| AgentError::Network(e.to_string()))?;

    // 3. Check status
    if !response.status().is_success() {
        return Err(AgentError::Upstream {
            status: response.status().as_u16(),
            message: response.text().await.unwrap_or_default(),
        });
    }

    // 4. Parse the response into Vec<Vec<f32>>
    //    Adapt this to your backend's response format
    let body: serde_json::Value = response.json().await
        .map_err(|e| AgentError::InvalidResponse(e.to_string()))?;

    let data = body["data"].as_array()
        .ok_or_else(|| AgentError::InvalidResponse("Missing data".into()))?;

    let results = data.iter().map(|item| {
        item["embedding"].as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect()
    }).collect();

    Ok(results)
}
```

**Step 3:** Add tests. At minimum:

```rust
#[tokio::test]
async fn test_embeddings_success() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/v1/embeddings")
        .with_status(200)
        .with_body(r#"{"data":[{"embedding":[0.1,0.2]}]}"#)
        .create_async()
        .await;

    let agent = test_agent(server.url());
    let result = agent.embeddings(vec!["hello".into()]).await.unwrap();

    mock.assert_async().await;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], vec![0.1, 0.2]);
}
```

**That's it.** The handler in `src/api/embeddings.rs` doesn't need any changes — it calls `agent.embeddings()` through the trait, so your new agent is automatically supported.
