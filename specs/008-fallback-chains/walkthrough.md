# Fallback Chains - Code Walkthrough

**Feature**: F08 - Fallback Chains  
**Audience**: Junior developers joining the project  
**Last Updated**: 2026-02-08

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: routing.rs - Configuration](#file-1-routingrs---configuration)
4. [File 2: error.rs - What Can Go Wrong](#file-2-errorrs---what-can-go-wrong)
5. [File 3: mod.rs - The Routing Logic](#file-3-modrs---the-routing-logic)
6. [File 4: completions.rs - API Layer](#file-4-completionsrs---api-layer)
7. [Understanding the Tests](#understanding-the-tests)
8. [Key Rust Concepts](#key-rust-concepts)
9. [Common Patterns in This Codebase](#common-patterns-in-this-codebase)
10. [Next Steps](#next-steps)

---

## The Big Picture

Think of Fallback Chains as a **backup plan for your favorite restaurant**. When you really want pizza but your usual place is closed, you have a list of alternatives: first try the Italian place downtown, then the diner on the corner.

Fallback chains work the same way for AI models. When the requested model isn't available (maybe all its backends are unhealthy), Nexus automatically tries alternatives in order.

### Fallback vs Retry

Before we dive in, let's clarify two related but different concepts:

```
┌─────────────────────────────────────────────────────────────────────┐
│                        RETRY vs FALLBACK                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  RETRY (same model, different backend)                              │
│  ═══════════════════════════════════════                            │
│    Request: "llama3:70b"                                            │
│    ┌─────────────┐     FAIL      ┌─────────────┐                    │
│    │  Backend A  │ ───────────▶  │  Backend B  │   (same model)     │
│    │  llama3:70b │               │  llama3:70b │                    │
│    └─────────────┘               └─────────────┘                    │
│                                                                     │
│  FALLBACK (different model)                                         │
│  ═══════════════════════════                                        │
│    Request: "llama3:70b"                                            │
│    Model unavailable!                                               │
│    ┌─────────────┐               ┌─────────────┐                    │
│    │  Backend C  │               │  Backend D  │                    │
│    │  qwen2:72b  │   (try this)  │  mistral:7b │   (then this)     │
│    └─────────────┘               └─────────────┘                    │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

| Concept | Trigger | Scope |
|---------|---------|-------|
| **Retry** | Request fails (timeout, error) | Same model, different backend |
| **Fallback** | Model completely unavailable | Different model entirely |

### How It Fits in Nexus

```
┌─────────────────────────────────────────────────────────────────────┐
│                              Nexus                                   │
│                                                                      │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │                      API Layer                                │   │
│  │  ┌────────────────────────────────────────────────────────┐  │   │
│  │  │ completions.rs                                          │  │   │
│  │  │ • Calls Router.select_backend()                         │  │   │
│  │  │ • Adds X-Nexus-Fallback-Model header if needed          │  │   │
│  │  └────────────────────────────────────────────────────────┘  │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                              │                                       │
│                              ▼                                       │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │                      Router (YOU ARE HERE!)                   │   │
│  │  ┌────────────────────────────────────────────────────────┐  │   │
│  │  │ select_backend()                                        │  │   │
│  │  │ 1. Resolve aliases                                      │  │   │
│  │  │ 2. Try primary model                                    │  │   │
│  │  │ 3. If unavailable → try fallback chain                  │  │   │
│  │  │ 4. Return RoutingResult with fallback_used flag         │  │   │
│  │  └────────────────────────────────────────────────────────┘  │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                              │                                       │
│                              ▼                                       │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │                      Registry                                 │   │
│  │  • Tracks backend health                                      │   │
│  │  • Router queries: "Which backends have model X?"             │   │
│  └──────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

---

## File Structure

```
src/
├── config/
│   └── routing.rs          # Fallback configuration parsing
├── routing/
│   ├── mod.rs              # Router struct, select_backend(), RoutingResult
│   └── error.rs            # FallbackChainExhausted error
└── api/
    └── completions.rs      # X-Nexus-Fallback-Model header injection

tests/
├── routing_integration.rs          # Fallback routing tests
└── fallback_header_integration.rs  # HTTP header tests
```

---

## File 1: routing.rs - Configuration

```rust
// src/config/routing.rs

/// Routing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RoutingConfig {
    pub strategy: RoutingStrategy,
    pub max_retries: u32,
    pub weights: RoutingWeights,
    #[serde(default)]
    pub aliases: HashMap<String, String>,
    #[serde(default)]
    pub fallbacks: HashMap<String, Vec<String>>,  // ◄── Fallback chains!
}
```

**What's happening here:**

| Field | Type | Purpose |
|-------|------|---------|
| `fallbacks` | `HashMap<String, Vec<String>>` | Maps model → list of fallbacks |
| `#[serde(default)]` | Attribute | If missing in config, use empty HashMap |

**TOML Configuration Example:**

```toml
[routing.fallbacks]
"llama3:70b" = ["qwen2:72b", "mistral:7b"]
"gpt-4" = ["llama3:70b", "mixtral:8x7b"]
```

This means:
- If `llama3:70b` is unavailable → try `qwen2:72b` first, then `mistral:7b`
- If `gpt-4` is unavailable → try `llama3:70b` first, then `mixtral:8x7b`

---

## File 2: error.rs - What Can Go Wrong

```rust
// src/routing/error.rs

/// Errors that can occur during backend selection
#[derive(Debug, Error)]
pub enum RoutingError {
    /// The requested model was not found in any backend
    #[error("Model '{model}' not found")]
    ModelNotFound { model: String },

    /// No healthy backend is available for the requested model
    #[error("No healthy backend available for model '{model}'")]
    NoHealthyBackend { model: String },

    /// No backend supports the required capabilities
    #[error("No backend supports required capabilities for model '{model}': {missing:?}")]
    CapabilityMismatch { model: String, missing: Vec<String> },

    /// All models in the fallback chain were exhausted
    #[error("All backends in fallback chain unavailable: {chain:?}")]
    FallbackChainExhausted { chain: Vec<String> },  // ◄── New for F08!
}
```

**Breaking it down:**

| Error Variant | When It Happens | HTTP Status |
|---------------|-----------------|-------------|
| `ModelNotFound` | Model doesn't exist in any backend | 404 |
| `NoHealthyBackend` | Model exists but all backends are down | 503 |
| `FallbackChainExhausted` | Primary AND all fallbacks unavailable | 503 |

**Why FallbackChainExhausted is Different:**

When this error occurs, Nexus includes the entire chain in the error message:
```json
{
    "error": {
        "message": "All backends in fallback chain unavailable: [\"llama3:70b\", \"qwen2:72b\", \"mistral:7b\"]",
        "type": "service_unavailable",
        "code": 503
    }
}
```

This helps operators understand that fallbacks were attempted.

---

## File 3: mod.rs - The Routing Logic

### Part 1: RoutingResult Struct

```rust
// src/routing/mod.rs

/// Result of a successful routing decision
#[derive(Debug)]
pub struct RoutingResult {
    /// The selected backend
    pub backend: Arc<Backend>,
    /// The actual model name used (may differ if fallback)
    pub actual_model: String,
    /// True if a fallback model was used
    pub fallback_used: bool,
}
```

**Why do we need this?**

Before F08, `select_backend()` just returned a backend. But now we need to know:
- Did we use a fallback? (`fallback_used`)
- What model did we actually use? (`actual_model`)

This information flows to the API layer, which adds the `X-Nexus-Fallback-Model` header.

### Part 2: Router Struct

```rust
pub struct Router {
    /// Reference to backend registry
    registry: Arc<Registry>,
    
    /// Routing strategy to use
    strategy: RoutingStrategy,
    
    /// Model aliases (alias → target)
    aliases: HashMap<String, String>,
    
    /// Fallback chains (model → [fallback1, fallback2, ...])
    fallbacks: HashMap<String, Vec<String>>,  // ◄── Fallback chains!
    
    // ... other fields
}
```

### Part 3: get_fallbacks() - Retrieve the Chain

```rust
/// Get fallback chain for a model
fn get_fallbacks(&self, model: &str) -> Vec<String> {
    self.fallbacks.get(model).cloned().unwrap_or_default()
}
```

**What's happening:**
- `self.fallbacks.get(model)` - Look up fallbacks for this model
- `.cloned()` - Make a copy (fallbacks is `&Vec<String>`, we need `Vec<String>`)
- `.unwrap_or_default()` - If no fallbacks configured, return empty Vec

### Part 4: select_backend() - The Main Logic

This is where the magic happens. Let's walk through it step by step:

```rust
pub fn select_backend(
    &self,
    requirements: &RequestRequirements,
) -> Result<RoutingResult, RoutingError> {
    // Step 1: Resolve any aliases
    let model = self.resolve_alias(&requirements.model);

    // Step 2: Check if model exists anywhere (even in unhealthy backends)
    let all_backends = self.registry.get_backends_for_model(&model);
    let model_exists = !all_backends.is_empty();

    // Step 3: Try to find HEALTHY backends for the primary model
    let candidates = self.filter_candidates(&model, requirements);

    if !candidates.is_empty() {
        // Success! Use primary model
        let selected = match self.strategy {
            RoutingStrategy::Smart => self.select_smart(&candidates),
            // ... other strategies
        };
        return Ok(RoutingResult {
            backend: Arc::new(selected),
            actual_model: model.clone(),
            fallback_used: false,  // ◄── No fallback needed!
        });
    }

    // Step 4: Primary unavailable → try fallback chain
    let fallbacks = self.get_fallbacks(&model);
    for fallback_model in &fallbacks {
        let candidates = self.filter_candidates(fallback_model, requirements);
        if !candidates.is_empty() {
            let selected = /* select using strategy */;
            tracing::warn!(
                requested_model = %model,
                fallback_model = %fallback_model,
                backend = %selected.name,
                "Using fallback model"
            );
            return Ok(RoutingResult {
                backend: Arc::new(selected),
                actual_model: fallback_model.clone(),  // ◄── Fallback model!
                fallback_used: true,                    // ◄── Flag is set!
            });
        }
    }

    // Step 5: All attempts failed
    if !fallbacks.is_empty() {
        // We had fallbacks configured, but all exhausted
        let mut chain = vec![model.clone()];
        chain.extend(fallbacks);
        Err(RoutingError::FallbackChainExhausted { chain })
    } else if model_exists {
        // Model exists but no healthy backends (no fallbacks configured)
        Err(RoutingError::NoHealthyBackend { model: model.clone() })
    } else {
        // Model doesn't exist at all
        Err(RoutingError::ModelNotFound {
            model: requirements.model.clone(),
        })
    }
}
```

**The Fallback Flow (ASCII Diagram):**

```
┌─────────────────────────────────────────────────────────────────────┐
│  Request: model = "llama3:70b"                                      │
│  Config:  fallbacks["llama3:70b"] = ["qwen2:72b", "mistral:7b"]     │
└─────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
                    ┌───────────────────┐
                    │ Resolve aliases   │
                    │ "llama3:70b"      │
                    └───────────────────┘
                              │
                              ▼
                    ┌───────────────────┐
                    │ Try primary model │
                    │ "llama3:70b"      │
                    └───────────────────┘
                              │
              ┌───────────────┴───────────────┐
              │                               │
              ▼                               ▼
      ┌───────────────┐               ┌───────────────┐
      │ Healthy       │               │ No healthy    │
      │ backend found │               │ backends      │
      └───────────────┘               └───────────────┘
              │                               │
              ▼                               ▼
      ┌───────────────┐               ┌───────────────────┐
      │ Return        │               │ Try fallback[0]   │
      │ fallback_used │               │ "qwen2:72b"       │
      │ = false       │               └───────────────────┘
      └───────────────┘                       │
                              ┌───────────────┴───────────────┐
                              │                               │
                              ▼                               ▼
                      ┌───────────────┐               ┌───────────────┐
                      │ Found!        │               │ Not found     │
                      │ Log WARN      │               └───────────────┘
                      │ Return        │                       │
                      │ fallback_used │                       ▼
                      │ = true        │               ┌───────────────────┐
                      └───────────────┘               │ Try fallback[1]   │
                                                      │ "mistral:7b"      │
                                                      └───────────────────┘
                                                              │
                                              ┌───────────────┴───────────────┐
                                              │                               │
                                              ▼                               ▼
                                      ┌───────────────┐               ┌───────────────┐
                                      │ Found!        │               │ Not found     │
                                      │ Return        │               │ No more       │
                                      │ fallback_used │               │ fallbacks     │
                                      │ = true        │               └───────────────┘
                                      └───────────────┘                       │
                                                                              ▼
                                                              ┌───────────────────────────┐
                                                              │ Err(FallbackChainExhausted)│
                                                              │ chain: ["llama3:70b",     │
                                                              │         "qwen2:72b",      │
                                                              │         "mistral:7b"]     │
                                                              └───────────────────────────┘
```

### Key Design Decision: Single-Level Fallbacks

**Fallbacks do NOT chain to other fallbacks.**

```toml
[routing.fallbacks]
"primary" = ["fallback1", "fallback2"]
"fallback1" = ["alternate"]  # This is NOT followed from "primary"!
```

When requesting "primary":
1. Try "primary" ❌
2. Try "fallback1" ❌
3. Try "fallback2" ❌
4. Error! (does NOT try "alternate")

**Why?**
- Predictability: You know exactly which models might be used
- Performance: O(n) where n = fallback chain length
- Control: User explicitly defines all acceptable alternatives

---

## File 4: completions.rs - API Layer

### Part 1: The Header Constant

```rust
// src/api/completions.rs

/// Header name for fallback model notification (lowercase for HTTP/2 compatibility)
pub const FALLBACK_HEADER: &str = "x-nexus-fallback-model";
```

**Why lowercase?**

HTTP/2 requires header names to be lowercase. Using a constant ensures consistency.

### Part 2: Adding the Header (Non-Streaming)

```rust
pub async fn handle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Response, ApiError> {
    // ... routing and request handling ...

    let routing_result = state.router.select_backend(&requirements)?;
    let fallback_used = routing_result.fallback_used;
    let actual_model = routing_result.actual_model.clone();

    // ... proxy request to backend ...

    match proxy_request(&state, backend, &headers, &request).await {
        Ok(response) => {
            // Create response
            let mut resp = Json(response).into_response();
            
            // Add fallback header if applicable
            if fallback_used {
                if let Ok(header_value) = HeaderValue::from_str(&actual_model) {
                    resp.headers_mut()
                        .insert(HeaderName::from_static(FALLBACK_HEADER), header_value);
                }
            }
            return Ok(resp);
        }
        // ... error handling ...
    }
}
```

**What's happening:**

1. `routing_result.fallback_used` - Did the router use a fallback?
2. `routing_result.actual_model` - What model was actually used?
3. Only if `fallback_used` is true, we add the header
4. `HeaderValue::from_str()` can fail if model name has invalid characters

### Part 3: Adding the Header (Streaming)

```rust
async fn handle_streaming(
    state: Arc<AppState>,
    headers: HeaderMap,
    request: ChatCompletionRequest,
) -> Result<Response, ApiError> {
    // ... routing ...
    
    let fallback_used = routing_result.fallback_used;
    let actual_model = routing_result.actual_model.clone();

    // Create SSE stream
    let stream = create_sse_stream(state, Arc::clone(&backend), headers, request);

    // Create SSE response and add fallback header if needed
    let mut resp = Sse::new(stream).into_response();
    if fallback_used {
        if let Ok(header_value) = HeaderValue::from_str(&actual_model) {
            resp.headers_mut()
                .insert(HeaderName::from_static(FALLBACK_HEADER), header_value);
        }
    }

    Ok(resp)
}
```

**Key Point:** The header is added to the initial HTTP response, not to each SSE event. This means clients can detect fallback usage before any streaming data arrives.

### Part 4: Error Mapping

```rust
let routing_result = state.router.select_backend(&requirements).map_err(|e| {
    match e {
        // ... other errors ...
        
        crate::routing::RoutingError::FallbackChainExhausted { chain } => {
            ApiError::model_not_found(&chain[0], &available)
        }
        crate::routing::RoutingError::NoHealthyBackend { model } => {
            ApiError::service_unavailable(&format!(
                "No healthy backend available for model '{}'",
                model
            ))
        }
    }
})?;
```

| Routing Error | HTTP Response |
|---------------|---------------|
| `FallbackChainExhausted` | 404 (model not found) |
| `NoHealthyBackend` | 503 (service unavailable) |

---

## Understanding the Tests

### Test Categories

| Category | File | Purpose |
|----------|------|---------|
| Unit Tests | `src/routing/mod.rs` | Test fallback logic in isolation |
| Integration Tests | `tests/routing_integration.rs` | Test Router with Registry |
| API Tests | `tests/fallback_header_integration.rs` | Test HTTP header injection |

### Example: Basic Fallback Test

```rust
// tests/routing_integration.rs

#[test]
fn test_routing_with_fallbacks() {
    // ARRANGE: Setup registry with only fallback model
    let registry = Arc::new(Registry::new());
    registry
        .add_backend(create_test_backend(
            "backend1",
            "Backend 1",
            "mistral:7b",  // Only this model available
            1,
        ))
        .unwrap();

    // Configure fallback: llama3:70b → [llama3:8b, mistral:7b]
    let mut config = NexusConfig::default();
    config.routing.fallbacks.insert(
        "llama3:70b".to_string(),
        vec!["llama3:8b".to_string(), "mistral:7b".to_string()],
    );
    
    let state = AppState::new(registry, Arc::new(config));

    // ACT: Request the unavailable primary model
    let request = ChatCompletionRequest {
        model: "llama3:70b".to_string(),  // Not available!
        // ...
    };
    let requirements = RequestRequirements::from_request(&request);
    let result = state.router.select_backend(&requirements).unwrap();

    // ASSERT: Should fallback to mistral
    assert_eq!(result.backend.models[0].id, "mistral:7b");
    assert!(result.fallback_used);
    assert_eq!(result.actual_model, "mistral:7b");
}
```

**The Pattern:** Arrange → Act → Assert

### Example: Fallback Header Test

```rust
// tests/fallback_header_integration.rs

#[tokio::test]
async fn api_response_includes_fallback_header() {
    // ARRANGE: Setup mock servers
    let primary_mock = MockServer::start().await;
    let fallback_mock = MockServer::start().await;

    // Mock fallback backend response
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(/* ... */))
        .mount(&fallback_mock)
        .await;

    let (mut app, registry) = create_test_app_with_fallback(&primary_mock, &fallback_mock).await;

    // Make primary backend unhealthy (force fallback)
    let _ = registry.update_status(
        "primary-backend",
        BackendStatus::Unhealthy,
        Some("down".to_string()),
    );

    // ACT: Send request for primary model
    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(/* model: "primary-model" */)
        .unwrap();

    let response = app.call(request).await.unwrap();

    // ASSERT: Header is present with fallback model name
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key("x-nexus-fallback-model"));
    assert_eq!(
        response.headers().get("x-nexus-fallback-model").unwrap().to_str().unwrap(),
        "fallback-model"
    );
}
```

### Example: Combined Alias + Fallback Test

```rust
#[test]
fn test_routing_result_with_alias_and_fallback() {
    // Given:
    // - alias "alias" → "primary"
    // - fallback "primary" → ["fallback"]
    // - only "fallback" is available

    let registry = Arc::new(Registry::new());
    registry.add_backend(create_test_backend(
        "backend_fallback",
        "Backend Fallback",
        "fallback",  // Only model available
        1,
    )).unwrap();

    let mut config = NexusConfig::default();
    config.routing.aliases
        .insert("alias".to_string(), "primary".to_string());
    config.routing.fallbacks
        .insert("primary".to_string(), vec!["fallback".to_string()]);
    
    let state = AppState::new(registry, Arc::new(config));

    // Request "alias" → resolves to "primary" → fallback to "fallback"
    let request = ChatCompletionRequest {
        model: "alias".to_string(),
        // ...
    };
    let requirements = RequestRequirements::from_request(&request);
    let result = state.router.select_backend(&requirements).unwrap();

    // Then: fallback was used, actual model is "fallback"
    assert!(result.fallback_used);
    assert_eq!(result.actual_model, "fallback");
}
```

**Key Insight:** Aliases are resolved FIRST, then fallbacks apply to the resolved model.

---

## Key Rust Concepts

| Concept | What It Means |
|---------|---------------|
| `HashMap<String, Vec<String>>` | Map of model name to list of fallback names |
| `Arc<Backend>` | Shared ownership of backend across threads |
| `HeaderName::from_static()` | Create header name from compile-time string |
| `HeaderValue::from_str()` | Create header value (can fail if invalid chars) |
| `tracing::warn!()` | Log at WARN level with structured fields |
| `#[serde(default)]` | Use default value if field missing in config |
| `Result<RoutingResult, RoutingError>` | Either success with result, or specific error |
| `.cloned()` | Convert `Option<&T>` to `Option<T>` by cloning |
| `.unwrap_or_default()` | Get value or default (empty Vec for Vec<String>) |

---

## Common Patterns in This Codebase

### Pattern 1: Iterating Through Fallbacks

```rust
// Linear search through fallback chain
let fallbacks = self.get_fallbacks(&model);
for fallback_model in &fallbacks {
    let candidates = self.filter_candidates(fallback_model, requirements);
    if !candidates.is_empty() {
        // Found one!
        return Ok(/* success with this fallback */);
    }
}
// None found - error
```

**Why linear?** Fallback chains are small (typically 2-3 models), and order matters.

### Pattern 2: Conditional Header Injection

```rust
if fallback_used {
    if let Ok(header_value) = HeaderValue::from_str(&actual_model) {
        resp.headers_mut()
            .insert(HeaderName::from_static(FALLBACK_HEADER), header_value);
    }
}
```

**Key Points:**
- Header only added when fallback was used
- `from_str()` can fail - we silently skip if model name has invalid characters
- Mutation happens on a mutable reference to headers

### Pattern 3: Building Error Context

```rust
// Build descriptive error with full chain
let mut chain = vec![model.clone()];
chain.extend(fallbacks);
Err(RoutingError::FallbackChainExhausted { chain })
```

This creates: `["llama3:70b", "qwen2:72b", "mistral:7b"]`

Operators can see exactly what was tried.

### Pattern 4: Struct for Return Metadata

```rust
// Instead of:
fn select_backend(&self) -> Result<Arc<Backend>, Error>

// We use:
fn select_backend(&self) -> Result<RoutingResult, Error>

// Where RoutingResult carries extra information:
pub struct RoutingResult {
    pub backend: Arc<Backend>,
    pub actual_model: String,
    pub fallback_used: bool,
}
```

**Why?** The caller (API layer) needs to know if fallback was used to add the header.

---

## Next Steps

Now that you understand Fallback Chains, explore:

1. **Model Aliases (F07)** - How `gpt-4` can map to `llama3:70b`
2. **Intelligent Router (F06)** - The scoring logic that picks the best backend
3. **Health Checker** - How backends become unhealthy (triggering fallbacks)
4. **API Layer** - How responses flow back to clients

### Quick Reference

```bash
# Run fallback tests
cargo test fallback

# Run header tests  
cargo test fallback_header

# Run all routing tests
cargo test routing

# See WARN logs when fallback happens
RUST_LOG=warn cargo run -- serve
```
