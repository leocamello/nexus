# F06: Intelligent Router

**Status**: Draft  
**Priority**: P1  
**Branch**: `feature/f06-intelligent-router`  
**Dependencies**: F02 (Backend Registry), F03 (Health Checker)

---

## Overview

### What It Is
An intelligent request routing system that selects the best backend for each request based on model requirements, backend capabilities, and current system state.

### Goals
1. Route requests to backends that can fulfill model and capability requirements
2. Balance load across backends using configurable scoring
3. Support model aliases for transparent model substitution
4. Provide fallback chains for resilience
5. Make routing decisions in < 1ms with no external calls

### Non-Goals
1. GPU/resource scheduling (backends manage their own resources)
2. Request queuing (requests are routed immediately or rejected)
3. Model downloading or management
4. Load prediction or auto-scaling

---

## User Stories

### US-01: Basic Model Routing
**As a** developer using an OpenAI client  
**I want** requests to be routed to a backend that has my requested model  
**So that** I can use any model available in my cluster without knowing which backend hosts it

**Priority**: P0 (Core functionality)

**Acceptance Scenarios**:
- **Given** backends A (llama3:8b) and B (mistral:7b) are healthy
- **When** I request model "llama3:8b"
- **Then** the request is routed to backend A

- **Given** no backend has model "gpt-5"
- **When** I request model "gpt-5"
- **Then** I receive a 404 error with message "Model 'gpt-5' not found"

---

### US-02: Capability-Based Routing
**As a** developer sending multimodal requests  
**I want** requests to be routed only to backends that support the required capabilities  
**So that** my vision/tool requests don't fail due to capability mismatch

**Priority**: P0 (Core functionality)

**Acceptance Scenarios**:
- **Given** backend A has llama3 (no vision) and backend B has llava (vision)
- **When** I send a request with image_url in messages
- **Then** the request is routed to backend B

- **Given** backend A has llama3 (no tools) and backend B has llama3 (tools)
- **When** I send a request with tools array
- **Then** the request is routed to backend B

- **Given** no backend supports vision for model "llama3:8b"
- **When** I send a vision request for "llama3:8b"
- **Then** I receive a 400 error explaining capability mismatch

---

### US-03: Load-Aware Routing
**As a** system administrator  
**I want** requests distributed based on backend load and latency  
**So that** no single backend becomes overwhelmed

**Priority**: P0 (Core functionality)

**Acceptance Scenarios**:
- **Given** backends A (10 pending requests) and B (2 pending requests) both have llama3
- **When** I request model "llama3:8b"
- **Then** the request is more likely to route to backend B

- **Given** backends A (50ms avg latency) and B (200ms avg latency)
- **When** I request a model both support
- **Then** backend A receives higher score

---

### US-04: Model Aliases
**As a** developer migrating from OpenAI  
**I want** to use familiar model names like "gpt-4" that map to local models  
**So that** I don't need to change my client code

**Priority**: P1 (Enhanced functionality)

**Acceptance Scenarios**:
- **Given** alias "gpt-4" → "llama3:70b" is configured
- **When** I request model "gpt-4"
- **Then** the request is routed to a backend with "llama3:70b"

- **Given** alias "gpt-4" → "llama3:70b" but no backend has llama3:70b
- **When** I request model "gpt-4"
- **Then** I receive a 404 error mentioning both the alias and target model

---

### US-05: Fallback Chains
**As a** system administrator  
**I want** to configure fallback models when primary models are unavailable  
**So that** requests succeed even when preferred backends are down

**Priority**: P1 (Enhanced functionality)

**Acceptance Scenarios**:
- **Given** fallback chain "claude-3-opus" → ["llama3:70b", "mistral:7b"]
- **And** no backend has claude-3-opus or llama3:70b
- **When** I request model "claude-3-opus"
- **Then** the request is routed to a backend with "mistral:7b"

- **Given** all models in fallback chain are unavailable
- **When** I request the primary model
- **Then** I receive a 503 error listing the attempted models

---

### US-06: Routing Strategies
**As a** system administrator  
**I want** to choose different routing strategies for different use cases  
**So that** I can optimize for my specific workload

**Priority**: P1 (Enhanced functionality)

**Acceptance Scenarios**:
- **Given** strategy is "round_robin" with 3 healthy backends
- **When** I send 6 requests
- **Then** each backend receives exactly 2 requests

- **Given** strategy is "priority_only" with backends at priority 1 and 2
- **When** I send requests
- **Then** all requests go to the priority 1 backend

- **Given** strategy is "random"
- **When** I send 100 requests to 3 backends
- **Then** distribution is approximately even (each gets 25-45 requests)

---

## Technical Design

### Request Requirements Extraction

Requirements are extracted from the incoming `ChatCompletionRequest`:

```rust
pub struct RequestRequirements {
    /// Model name from request
    pub model: String,
    
    /// Estimated token count (characters / 4)
    pub estimated_tokens: u32,
    
    /// Request contains image_url in messages
    pub needs_vision: bool,
    
    /// Request has tools array
    pub needs_tools: bool,
    
    /// Request needs JSON mode (response_format.type == "json_object")
    pub needs_json_mode: bool,
}

impl RequestRequirements {
    pub fn from_request(request: &ChatCompletionRequest) -> Self;
}
```

**Detection Logic**:
| Requirement | Detection Method |
|-------------|------------------|
| Vision | Any `messages[*].content[*].type == "image_url"` |
| Tools | `tools` array present and non-empty |
| JSON Mode | `response_format.type == "json_object"` |
| Token Estimate | `sum(len(m.content) for m in messages) / 4` where content is the text string (for multipart content, only text parts are counted) |

### Routing Decision Flow

```
┌──────────────────────────────────────────────────────────────┐
│                    select_backend(request)                    │
└──────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────┐
│ 1. Extract requirements from request                          │
│    - model_name, estimated_tokens                             │
│    - needs_vision, needs_tools, needs_json_mode               │
└──────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────┐
│ 2. Get candidate backends for model                           │
│    registry.get_backends_for_model(model_name)                │
└──────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────┐
│ 3. Filter by health status (Healthy only)                     │
└──────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────┐
│ 4. Filter by capabilities                                     │
│    - context_length >= estimated_tokens                       │
│    - supports_vision if needs_vision                          │
│    - supports_tools if needs_tools                            │
└──────────────────────────────────────────────────────────────┘
                              │
                              ▼
                    ┌─────────────────┐
                    │ Any candidates? │
                    └─────────────────┘
                     │ No          │ Yes
                     ▼             │
┌────────────────────────────┐    │
│ 5a. Try alias resolution   │    │
│     If alias exists, retry │    │
│     with aliased model     │    │
└────────────────────────────┘    │
          │ No alias              │
          ▼                       │
┌────────────────────────────┐    │
│ 5b. Try fallback chain     │    │
│     For each fallback:     │    │
│     retry with that model  │    │
└────────────────────────────┘    │
          │ No fallback           │
          ▼                       │
┌────────────────────────────┐    │
│ Return NoBackendAvailable  │    │
│ error with details         │    │
└────────────────────────────┘    │
                                  │
                                  ▼
┌──────────────────────────────────────────────────────────────┐
│ 6. Apply routing strategy                                     │
│    - smart: score and select best                             │
│    - round_robin: next in rotation                            │
│    - priority_only: lowest priority number                    │
│    - random: random selection                                 │
└──────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────┐
│ Return selected backend                                       │
└──────────────────────────────────────────────────────────────┘
```

### Scoring Function (Smart Strategy)

```rust
pub struct ScoringWeights {
    pub priority: u32,  // Default: 50
    pub load: u32,      // Default: 30
    pub latency: u32,   // Default: 20
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self { priority: 50, load: 30, latency: 20 }
    }
}

pub fn score(backend: &Backend, weights: &ScoringWeights) -> u32 {
    let priority_score = 100 - backend.priority.min(100);
    let load_score = 100 - backend.pending_requests().min(100);
    let latency_score = 100 - (backend.avg_latency_ms() / 10).min(100);
    
    (priority_score * weights.priority
     + load_score * weights.load
     + latency_score * weights.latency) / 100
}
```

**Score Components**:
| Component | Calculation | Range | Weight |
|-----------|-------------|-------|--------|
| Priority | `100 - min(priority, 100)` | 0-100 | 50% |
| Load | `100 - min(pending_requests, 100)` | 0-100 | 30% |
| Latency | `100 - min(avg_latency_ms / 10, 100)` | 0-100 | 20% |

### Routing Strategies

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoutingStrategy {
    #[default]
    Smart,
    RoundRobin,
    PriorityOnly,
    Random,
}
```

| Strategy | Selection Logic | Use Case |
|----------|-----------------|----------|
| `Smart` | Score by priority + load + latency, select highest | Default, balanced |
| `RoundRobin` | Rotate through candidates in order | Even distribution |
| `PriorityOnly` | Always select lowest priority number | Dedicated primary |
| `Random` | Random selection from candidates | Testing, chaos |

### Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum RoutingError {
    #[error("Model '{model}' not found")]
    ModelNotFound { model: String },
    
    #[error("No healthy backend available for model '{model}'")]
    NoHealthyBackend { model: String },
    
    #[error("No backend supports required capabilities for model '{model}': {missing:?}")]
    CapabilityMismatch { model: String, missing: Vec<String> },
    
    #[error("All backends in fallback chain unavailable: {chain:?}")]
    FallbackChainExhausted { chain: Vec<String> },
}
```

### Router Struct

```rust
pub struct Router {
    /// Reference to backend registry
    registry: Arc<Registry>,
    
    /// Routing strategy
    strategy: RoutingStrategy,
    
    /// Scoring weights for smart strategy
    weights: ScoringWeights,
    
    /// Model aliases (alias → target)
    aliases: HashMap<String, String>,
    
    /// Fallback chains (model → [fallback1, fallback2, ...])
    fallbacks: HashMap<String, Vec<String>>,
    
    /// Round-robin counter (atomic for thread safety)
    round_robin_counter: AtomicU64,
}
```

---

## Configuration

```toml
[routing]
# Routing strategy: smart, round_robin, priority_only, random
strategy = "smart"

# Maximum retry attempts on backend failure
max_retries = 2

[routing.weights]
# Scoring weights for smart strategy (must sum to 100)
priority = 50
load = 30
latency = 20

[routing.aliases]
# Model aliases for OpenAI compatibility
"gpt-4" = "llama3:70b"
"gpt-4-turbo" = "llama3:70b"
"gpt-3.5-turbo" = "llama3:8b"
"claude-3-opus" = "llama3:70b"
"claude-3-sonnet" = "mistral:7b"

[routing.fallbacks]
# Fallback chains when primary model unavailable
"llama3:70b" = ["llama3:8b", "mistral:7b"]
"claude-3-opus" = ["llama3:70b", "mistral:7b"]
```

**Environment Variable Overrides**:
| Config | Environment Variable | Example |
|--------|---------------------|---------|
| `routing.strategy` | `NEXUS_ROUTING_STRATEGY` | `round_robin` |
| `routing.max_retries` | `NEXUS_ROUTING_MAX_RETRIES` | `3` |

---

## API Integration

The router integrates with the existing API layer:

```rust
// In POST /v1/chat/completions handler
async fn chat_completions(
    State(state): State<AppState>,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Response, ApiError> {
    // Extract requirements
    let requirements = RequestRequirements::from_request(&request);
    
    // Select backend
    let backend = state.router.select_backend(&requirements)?;
    
    // Proxy request to backend
    proxy_request(&backend, request).await
}
```

---

## Non-Functional Requirements

### Performance
| Metric | Target | Maximum |
|--------|--------|---------|
| Routing decision time | < 1ms | 2ms |
| Memory per alias | 100 bytes | 500 bytes |
| Memory per fallback chain | 200 bytes | 1KB |

### Concurrency
- Routing decisions must be thread-safe
- Multiple concurrent routing decisions allowed
- Round-robin counter uses atomic operations
- No locks during candidate scoring

### Reliability
- No external calls during routing (use cached registry data)
- Graceful degradation when all backends unhealthy
- Clear error messages for debugging

---

## Edge Cases

### Empty or Invalid States
| Condition | Behavior |
|-----------|----------|
| No backends registered | Return `ModelNotFound` error |
| All backends unhealthy | Return `NoHealthyBackend` error |
| Empty model name in request | Return 400 Bad Request |
| Unknown routing strategy | Use `Smart` as default |

### Alias and Fallback Edge Cases
| Condition | Behavior |
|-----------|----------|
| Circular alias (a→b→a) | Detect and return error (aliases are single-level) |
| Alias points to unavailable model | Try fallback chain for aliased model |
| Empty fallback chain | Treat as no fallback configured |
| Fallback model also has fallbacks | Do not chain fallbacks (single level only) |

### Capability Mismatches
| Condition | Behavior |
|-----------|----------|
| Vision request, no vision backends | Return `CapabilityMismatch` with "vision" |
| Tools request, no tools backends | Return `CapabilityMismatch` with "tools" |
| Context too long for all backends | Return `CapabilityMismatch` with "context_length" |
| Multiple missing capabilities | List all in error response |

### Scoring Edge Cases
| Condition | Behavior |
|-----------|----------|
| All backends same score | Return first candidate |
| Backend with priority > 100 | Clamp to 100 in score calculation |
| No latency data yet | Use 0ms (best possible score) |
| Pending requests > 100 | Clamp to 100 in score calculation |

---

## Testing Strategy

### Unit Tests
1. Requirements extraction from various request types
2. Scoring function with different weights
3. Each routing strategy in isolation
4. Alias resolution (including circular detection)
5. Fallback chain traversal
6. Capability matching logic

### Property-Based Tests
1. Score function always returns value in valid range
2. Round-robin distributes evenly over N iterations
3. Smart strategy always selects highest-scoring backend
4. Alias resolution terminates (no infinite loops)

### Integration Tests
1. End-to-end routing through API
2. Routing with live registry updates
3. Fallback behavior when backends go down
4. Concurrent routing decisions

### Performance Tests
1. Routing decision < 1ms with 100 backends
2. Routing decision < 1ms with 1000 models
3. No degradation under concurrent load

---

## Dependencies

### Internal
- `src/registry/mod.rs` - Backend and model data
- `src/api/types.rs` - ChatCompletionRequest type
- `src/config.rs` - RoutingConfig

### External Crates
- None new (uses existing: `thiserror`, `tracing`)

---

## File Structure

```
src/
├── routing/
│   ├── mod.rs           # Router struct and main logic
│   ├── requirements.rs  # RequestRequirements extraction
│   ├── scoring.rs       # Scoring function and weights
│   ├── strategies.rs    # RoutingStrategy implementations
│   └── error.rs         # RoutingError types
└── config.rs            # Add RoutingConfig
```

---

## Acceptance Criteria Summary

- [x] AC-01: Routes to backend with exact model match
- [x] AC-02: Filters candidates by health status (Healthy only)
- [x] AC-03: Filters by vision capability when request has images
- [x] AC-04: Filters by tools capability when request has tools
- [x] AC-05: Filters by context length (estimated tokens vs model limit)
- [x] AC-06: Scores backends using priority, load, latency
- [x] AC-07: Resolves model aliases transparently
- [x] AC-08: Traverses fallback chain when model unavailable
- [x] AC-09: Detects and prevents circular aliases
- [x] AC-10: Returns descriptive errors for all failure cases
- [x] AC-11: Smart strategy selects highest-scoring backend
- [x] AC-12: Round-robin distributes evenly
- [x] AC-13: Priority-only always selects lowest priority number
- [x] AC-14: Random strategy provides approximate even distribution
- [x] AC-15: Routing decision completes in < 1ms
- [x] AC-16: Thread-safe concurrent routing decisions

---

## References

- [OpenAI Chat Completions API](https://platform.openai.com/docs/api-reference/chat)
- [Nexus Architecture](../../docs/ARCHITECTURE.md)
- [Constitution - Intelligent Routing](../../.specify/memory/constitution.md)
