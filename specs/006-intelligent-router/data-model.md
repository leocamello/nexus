# Data Model: Intelligent Router (F06)

**Date**: 2025-01-10  
**Phase**: Phase 1 - Design & Contracts

This document defines the data entities and their relationships for the Intelligent Router feature.

## Core Entities

### 1. Router

**Purpose**: Central routing coordinator that selects the best backend for each incoming request based on model availability, backend capabilities, health status, and configurable scoring.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `registry` | `Arc<Registry>` | Shared reference to backend registry |
| `strategy` | `RoutingStrategy` | Immutable after construction; default: `Smart` |
| `weights` | `ScoringWeights` | Immutable after construction; must sum to 100 |
| `aliases` | `HashMap<String, String>` | Alias → target model mapping; validated for circular refs at startup |
| `fallbacks` | `HashMap<String, Vec<String>>` | Model → ordered fallback list; single-level only |
| `round_robin_counter` | `AtomicU64` | Monotonically increasing; wraps on overflow |

**Responsibilities**:
- Resolve model aliases (with chaining up to 3 levels)
- Find candidate backends for a model from the registry
- Filter candidates by health status (`Healthy` only) and capabilities
- Apply routing strategy to select the best candidate
- Traverse fallback chains when primary model unavailable
- Return `RoutingResult` with selected backend, actual model, and decision metadata

**Lifecycle**: Created once at gateway startup, shared via `Arc<Router>` (or owned by `AppState`). Immutable configuration after construction; only `round_robin_counter` mutates.

**Thread Safety**: All fields are immutable except `round_robin_counter` which uses `AtomicU64` for lock-free increment. Registry access is thread-safe via `Arc<Registry>`. Multiple concurrent `select_backend()` calls are safe.

---

### 2. RequestRequirements

**Purpose**: Requirements extracted from an incoming `ChatCompletionRequest` used to filter backend candidates by capability.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `model` | `String` | Raw model name from request; may be an alias |
| `estimated_tokens` | `u32` | `sum(content.len()) / 4` across all messages |
| `needs_vision` | `bool` | `true` if any message part has `type == "image_url"` |
| `needs_tools` | `bool` | `true` if `extra["tools"]` key exists in request |
| `needs_json_mode` | `bool` | `true` if `extra["response_format"]["type"] == "json_object"` |

**Responsibilities**:
- Parse `ChatCompletionRequest` to detect capability requirements
- Estimate token count from message content lengths
- Detect multimodal content (vision) from message parts
- Detect tool use from extra fields
- Detect JSON mode from response format

**Lifecycle**: Created per-request via `RequestRequirements::from_request()`. Short-lived; consumed by `Router::select_backend()`.

**Thread Safety**: `Clone + PartialEq + Debug`; value type with no shared state.

---

### 3. RoutingResult

**Purpose**: The outcome of a successful routing decision, carrying the selected backend and metadata about how it was chosen.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `backend` | `Arc<Backend>` | The selected backend (snapshot copy) |
| `actual_model` | `String` | Model name used (may differ from requested if alias/fallback) |
| `fallback_used` | `bool` | `true` if a fallback model was substituted |
| `route_reason` | `String` | Human-readable selection explanation |

**Route Reason Formats**:

| Strategy | Format | Example |
|----------|--------|---------|
| Smart (single) | `"only_healthy_backend"` | When only one candidate |
| Smart (multi) | `"highest_score:{name}:{score}"` | `"highest_score:ollama-local:95.00"` |
| RoundRobin | `"round_robin:index_{n}"` | `"round_robin:index_3"` |
| PriorityOnly | `"priority:{name}:{priority}"` | `"priority:fast-gpu:1"` |
| Random | `"random:{name}"` | `"random:ollama-2"` |
| Fallback | `"fallback:{model}:{strategy_reason}"` | `"fallback:llama3:70b:highest_score:92.00"` |

**Lifecycle**: Created by `select_backend()`, consumed by API handler for proxying and header construction.

**Thread Safety**: `Debug`; backend is `Arc`-wrapped for cheap cloning.

---

### 4. RoutingStrategy

**Purpose**: Enum defining how the router selects among qualified candidate backends.

**Attributes**:

| Variant | Selection Logic | Use Case |
|---------|-----------------|----------|
| `Smart` (default) | Score by priority + load + latency; select highest score | Balanced default |
| `RoundRobin` | Rotate through candidates via atomic counter | Even distribution |
| `PriorityOnly` | Select candidate with lowest priority number | Dedicated primary |
| `Random` | Hash-based random selection from candidates | Testing, chaos |

**Responsibilities**:
- Define backend selection algorithm
- Parse from string (`FromStr`) and serialize (`Display`)

**Lifecycle**: Set at startup from config; immutable.

**Thread Safety**: `Copy + Clone + PartialEq + Eq + Default`; enum value type.

---

### 5. ScoringWeights

**Purpose**: Configurable weights for the Smart routing strategy's scoring function.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `priority` | `u32` | Default: `50`; weight for backend priority score |
| `load` | `u32` | Default: `30`; weight for pending request count |
| `latency` | `u32` | Default: `20`; weight for average latency |

**Validation**: `priority + load + latency` must equal `100`. Enforced by `validate()`.

**Responsibilities**:
- Control relative importance of scoring components
- Validate that weights sum to 100

**Lifecycle**: Set at startup from config; immutable after construction.

**Thread Safety**: `Copy + Clone + PartialEq + Eq + Debug`; value type.

---

### 6. RoutingError

**Purpose**: Error types for routing failures, mapped to HTTP error responses.

**Attributes**:

| Variant | Fields | HTTP Status |
|---------|--------|-------------|
| `ModelNotFound` | `model: String` | 404 |
| `NoHealthyBackend` | `model: String` | 503 |
| `CapabilityMismatch` | `model: String, missing: Vec<String>` | 400 |
| `FallbackChainExhausted` | `chain: Vec<String>` | 503 |

**Responsibilities**:
- Provide descriptive error messages with actionable context
- Map to appropriate HTTP status codes
- Include model names and attempted chains for debugging

**Lifecycle**: Created on routing failure, propagated to API handler.

**Thread Safety**: `Debug + Error`; value type.

---

## Entity Relationships

```
┌─────────────────────────┐
│  ChatCompletionRequest  │
│  (from API handler)     │
└─────────────────────────┘
            │
            │ from_request()
            ▼
┌─────────────────────────┐
│  RequestRequirements    │
│                         │
│  - model                │
│  - estimated_tokens     │
│  - needs_vision         │
│  - needs_tools          │
│  - needs_json_mode      │
└─────────────────────────┘
            │
            │ select_backend()
            ▼
┌──────────────────────────────────────────────┐
│                   Router                      │
│                                              │
│  1. resolve_alias(model)      ◄── aliases    │
│  2. filter_candidates(model)  ◄── registry   │
│     - health: Healthy only                   │
│     - vision, tools, json_mode               │
│     - context_length >= tokens               │
│  3. apply strategy            ◄── strategy   │
│     - smart: score_backend()  ◄── weights    │
│     - round_robin: counter++                 │
│     - priority_only: min(priority)           │
│     - random: hash-based                     │
│  4. fallback chain            ◄── fallbacks  │
└──────────────────────────────────────────────┘
            │
            │ Ok / Err
            ▼
┌────────────────────┐    ┌─────────────────┐
│   RoutingResult    │    │  RoutingError    │
│                    │    │                  │
│  - backend (Arc)   │    │  ModelNotFound   │
│  - actual_model    │    │  NoHealthyBknd  │
│  - fallback_used   │    │  CapabilityMis. │
│  - route_reason    │    │  FallbackExh.   │
└────────────────────┘    └─────────────────┘
```

---

## State Transitions

### Routing Decision Flow

```
Request arrives
    ↓
Extract RequestRequirements
    ↓
Resolve alias chain (max 3 levels)
    ↓
Get backends for resolved model
    ↓
Filter: Healthy status only
    ↓
Filter: Capability match
    (vision, tools, json_mode, context_length)
    ↓
┌─────────────────────┐
│ Candidates found?   │
│                     │
│ Yes → Apply strategy│──────▶ Return RoutingResult
│                     │        (fallback_used=false)
│ No  → Try fallback  │
│       chain         │
└─────────────────────┘
    ↓
For each fallback model (in order):
    Filter candidates (same health + capability checks)
    ↓
    Found? → Log WARN, Return RoutingResult (fallback_used=true)
    ↓
All exhausted? → Return FallbackChainExhausted
No fallbacks configured?
    ↓
    Model exists but unhealthy → NoHealthyBackend
    Model never existed → ModelNotFound
```

### Scoring Function (Smart Strategy)

```
Input: priority (u32), pending_requests (u32), avg_latency_ms (u32)

priority_score  = 100 - min(priority, 100)
load_score      = 100 - min(pending_requests, 100)
latency_score   = 100 - min(avg_latency_ms / 10, 100)

final_score = (priority_score × weight.priority
             + load_score × weight.load
             + latency_score × weight.latency) / 100

Output: 0-100 (higher is better)
```

| Input | Score Range | Scaling |
|-------|-------------|---------|
| Priority 0 | 100 | Lower priority number = better |
| Priority 100+ | 0 | Clamped at 100 |
| 0 pending requests | 100 | Fewer = better |
| 100+ pending | 0 | Clamped at 100 |
| 0ms latency | 100 | Lower = better |
| 1000ms+ latency | 0 | Scaled by /10, clamped at 100 |

---

## Validation & Constraints

### Weights Validation

**Rule**: Scoring weights must sum to exactly 100.

**Implementation**:
```rust
impl ScoringWeights {
    pub fn validate(&self) -> Result<(), String> {
        let sum = self.priority + self.load + self.latency;
        if sum != 100 {
            Err(format!("Scoring weights must sum to 100, got {}", sum))
        } else {
            Ok(())
        }
    }
}
```

### Candidate Filtering

**Rule**: Candidates must pass all filters to be eligible:
1. Backend status must be `BackendStatus::Healthy`
2. If `needs_vision`, model's `supports_vision` must be `true`
3. If `needs_tools`, model's `supports_tools` must be `true`
4. If `needs_json_mode`, model's `supports_json_mode` must be `true`
5. `estimated_tokens` must be ≤ model's `context_length`

### Token Estimation

**Rule**: Token count is estimated as `content_length / 4` (rough chars-to-tokens ratio). For multipart messages, only `"text"` parts contribute to the estimate; `"image_url"` parts contribute zero tokens but set `needs_vision`.

### Strategy Parsing

**Rule**: Strategy parsed case-insensitively from string. Valid values: `"smart"`, `"round_robin"`, `"priority_only"`, `"random"`. Unknown values return error.

---

## Thread Safety

**Requirement**: Multiple concurrent routing decisions must execute without locks in the hot path.

**Implementation**:
- `Router` fields (`strategy`, `weights`, `aliases`, `fallbacks`) are immutable after construction
- `round_robin_counter` uses `AtomicU64::fetch_add(1, Relaxed)` for lock-free increment
- `Registry` access uses `DashMap` (concurrent HashMap) — no locks during candidate lookup
- Backend atomic counters (`pending_requests`, `avg_latency_ms`) read with `Ordering::Relaxed`
- Each `select_backend()` call creates a `Backend` snapshot (copies atomic values) to avoid TOCTOU issues
- No `Mutex` or `RwLock` in the routing hot path

---

## Performance Characteristics

| Operation | Target Latency | Implementation |
|-----------|----------------|----------------|
| Alias resolution (per level) | < 50ns | `HashMap::get()` lookup |
| Alias resolution (full chain) | < 200ns | Max 3 levels × HashMap lookup |
| Get candidates from registry | < 10µs | DashMap iteration over model index |
| Filter by health + capabilities | < 5µs | Linear scan of candidates (typically < 10) |
| Score single backend | < 100ns | 3 arithmetic operations + weighted sum |
| Score all candidates | < 1µs | Linear scan with scoring (typically < 10 backends) |
| Round-robin selection | < 50ns | Atomic fetch_add + modulo |
| Random selection | < 100ns | Hash-based random + index |
| Full routing decision | < 1ms | All steps combined |
| Backend snapshot creation | < 500ns | Clone strings + copy atomics |

**Total Request Overhead**: < 1ms for routing decision (constitution latency budget).

**Memory**:
- Router struct: ~500 bytes base + aliases/fallbacks
- Per alias entry: ~100 bytes (two String allocations)
- Per fallback entry: ~200 bytes (String + Vec<String>)
- Per routing decision: ~1KB (Backend snapshot, temporary)

---

## Testing Strategy

### Unit Tests

1. **Requirements extraction**: Text content token estimation, vision detection from `image_url` parts, tools detection from `extra["tools"]`, JSON mode detection from `response_format`
2. **Scoring function**: Default weights, component isolation (priority/load/latency), clamping at boundaries (0 and 100+), perfect score (0,0,0) = 100, worst score = 0
3. **Strategy selection**: Smart selects highest score, round-robin distributes evenly, priority-only selects lowest number, random provides approximate distribution
4. **Candidate filtering**: Health status filtering, vision/tools/json_mode capability filtering, context length filtering
5. **Error conditions**: ModelNotFound for unknown models, NoHealthyBackend when all unhealthy, FallbackChainExhausted when all fallbacks fail

### Property-Based Tests

1. Score function always returns 0-100
2. Round-robin distributes evenly over N iterations
3. Smart strategy always selects highest-scoring backend
4. Priority-only always selects lowest priority number

### Integration Tests

1. End-to-end routing through API with mock backends
2. Routing with live registry updates (backend goes unhealthy mid-request)
3. Concurrent routing decisions (thread safety)

---

## Future Extensions

### Not in Scope

1. **Request queuing**: Requests are routed immediately or rejected
2. **Load prediction**: No forecasting or auto-scaling
3. **GPU scheduling**: Backends manage their own resources
4. **Sticky sessions**: Stateless routing by design
5. **Per-request strategy override**: Strategy is global configuration
