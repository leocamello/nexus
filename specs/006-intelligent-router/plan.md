# F06: Intelligent Router - Technical Plan

**Feature**: Intelligent Router  
**Spec**: [spec.md](./spec.md)  
**Created**: 2026-02-08

---

## Constitution Check

### Simplicity Gate ✅
- [x] Using ≤3 main modules? **Yes**: routing (main), scoring, strategies
- [x] No speculative features? **Yes**: Only implementing documented requirements
- [x] No premature optimization? **Yes**: Simple scoring, no caching until needed
- [x] Simplest approach? **Yes**: Direct candidate filtering and scoring

### Anti-Abstraction Gate ✅
- [x] No wrapper layers? **Yes**: Direct use of Registry and types
- [x] Single representation? **Yes**: One Router struct, one scoring function
- [x] No framework-on-framework? **Yes**: Plain Rust with existing deps
- [x] Abstractions justified? **Yes**: Strategy trait justified by 4 distinct behaviors

### Integration-First Gate ✅
- [x] API contracts defined? **Yes**: RoutingError, RequestRequirements defined
- [x] Integration tests planned? **Yes**: End-to-end routing tests
- [x] End-to-end testable? **Yes**: Can test full routing through mock registry

### Performance Gate ✅
- [x] Routing decision < 1ms? **Yes**: No I/O, simple scoring math
- [x] Total overhead < 5ms? **Yes**: Routing is tiny part of request lifecycle
- [x] Memory baseline < 50MB? **Yes**: Small in-memory structures only

---

## Technical Approach

### Phase 1: Core Routing Engine

**Goal**: Basic model routing with capability filtering

1. Create `RequestRequirements` struct and extraction logic
2. Implement candidate filtering (model match, health, capabilities)
3. Create `RoutingError` enum with descriptive errors
4. Implement basic `Router::select_backend()` returning first match

**Key Decisions**:
- Requirements extracted once per request, passed through routing pipeline
- Capabilities checked as boolean flags (simpler than capability objects)
- Context length estimated as characters/4 (conservative estimate)

### Phase 2: Scoring and Smart Strategy

**Goal**: Intelligent backend selection based on multiple factors

1. Implement `ScoringWeights` and `score()` function
2. Add scoring to `Router` for smart strategy
3. Track pending requests and latency in Backend (if not already)
4. Select highest-scoring backend from candidates

**Key Decisions**:
- Scoring uses integer math (0-100 scale) for speed
- Weights configurable but must sum to 100
- Latency data from health checker (avg of last N checks)

### Phase 3: Additional Strategies

**Goal**: Support all four routing strategies

1. Implement `RoutingStrategy` enum
2. Add atomic counter for round-robin
3. Implement each strategy's selection logic
4. Strategy selection based on config

**Key Decisions**:
- Round-robin uses AtomicU64 for thread-safe counter
- Random uses `fastrand` (already a dev dependency)
- Priority-only simply sorts by priority and takes first

### Phase 4: Aliases and Fallbacks

**Goal**: Model substitution for compatibility and resilience

1. Add alias map to Router
2. Implement alias resolution with cycle detection
3. Add fallback chain map to Router
4. Implement fallback traversal when model unavailable

**Key Decisions**:
- Aliases are single-level (no chaining aliases)
- Fallbacks are single-level (don't follow fallback's fallbacks)
- Max 10 alias hops to prevent infinite loops
- Aliases applied before fallbacks

### Phase 5: Configuration Integration

**Goal**: Load routing config from TOML and environment

1. Add `RoutingConfig` to config.rs
2. Parse aliases and fallbacks from config
3. Add environment variable overrides
4. Wire config into Router construction

### Phase 6: API Integration

**Goal**: Connect router to HTTP handlers

1. Add Router to AppState
2. Use router in chat completions handler
3. Convert RoutingError to appropriate HTTP responses
4. Add routing metrics logging

---

## Data Structures

### New Types

```rust
// src/routing/requirements.rs
pub struct RequestRequirements {
    pub model: String,
    pub estimated_tokens: u32,
    pub needs_vision: bool,
    pub needs_tools: bool,
    pub needs_json_mode: bool,
}

// src/routing/scoring.rs
pub struct ScoringWeights {
    pub priority: u32,
    pub load: u32,
    pub latency: u32,
}

// src/routing/strategies.rs
pub enum RoutingStrategy {
    Smart,
    RoundRobin,
    PriorityOnly,
    Random,
}

// src/routing/error.rs
pub enum RoutingError {
    ModelNotFound { model: String },
    NoHealthyBackend { model: String },
    CapabilityMismatch { model: String, missing: Vec<String> },
    FallbackChainExhausted { chain: Vec<String> },
}

// src/routing/mod.rs
pub struct Router {
    registry: Arc<Registry>,
    strategy: RoutingStrategy,
    weights: ScoringWeights,
    aliases: HashMap<String, String>,
    fallbacks: HashMap<String, Vec<String>>,
    round_robin_counter: AtomicU64,
}
```

### Config Extension

```rust
// src/config.rs
pub struct RoutingConfig {
    pub strategy: RoutingStrategy,
    pub max_retries: u32,
    pub weights: ScoringWeights,
    pub aliases: HashMap<String, String>,
    pub fallbacks: HashMap<String, Vec<String>>,
}
```

---

## Test Strategy

### Unit Tests (per module)

**requirements.rs**:
- Extract from simple text request
- Extract from multimodal request (images)
- Extract from request with tools
- Extract from request with response_format
- Token estimation accuracy

**scoring.rs**:
- Score with default weights
- Score with custom weights
- Score at boundary values (0, 100, >100)
- Score with no latency data

**strategies.rs**:
- Smart selects highest score
- Round-robin cycles through backends
- Priority-only selects lowest number
- Random produces varied results

**Router (mod.rs)**:
- Basic model match routing
- Capability filtering
- Alias resolution
- Alias cycle detection
- Fallback chain traversal
- Error generation for each failure case

### Property-Based Tests

```rust
#[proptest]
fn score_always_in_range(
    priority: u32,
    pending: u32,
    latency: u32,
    weights: ScoringWeights,
) {
    let score = calculate_score(priority, pending, latency, &weights);
    prop_assert!(score <= 100);
}

#[proptest]
fn round_robin_distributes_evenly(backends: Vec<Backend>) {
    // After N * len(backends) iterations, each backend selected N times
}
```

### Integration Tests

```rust
// tests/routing_integration.rs
#[tokio::test]
async fn test_end_to_end_routing() {
    // Setup registry with multiple backends
    // Create router
    // Send request through full stack
    // Verify correct backend selected
}
```

---

## File Changes

### New Files
| File | Purpose |
|------|---------|
| `src/routing/mod.rs` | Router struct, select_backend logic |
| `src/routing/requirements.rs` | RequestRequirements extraction |
| `src/routing/scoring.rs` | ScoringWeights, score function |
| `src/routing/strategies.rs` | RoutingStrategy enum and impls |
| `src/routing/error.rs` | RoutingError enum |

### Modified Files
| File | Changes |
|------|---------|
| `src/lib.rs` | Add `pub mod routing;` |
| `src/config.rs` | Add RoutingConfig, parse routing section |
| `src/api/handlers.rs` | Use router for backend selection |
| `src/api/state.rs` | Add Router to AppState |

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Scoring formula produces unexpected results | Medium | Medium | Property-based tests, manual verification |
| Circular alias causes infinite loop | Low | High | Max hop limit, cycle detection |
| Performance regression with many backends | Low | Medium | Benchmark with 100+ backends |
| Thread contention on round-robin counter | Low | Low | AtomicU64 is fast, benchmark if needed |

---

## Complexity Tracking

| Component | Lines (Est.) | Complexity | Notes |
|-----------|--------------|------------|-------|
| requirements.rs | 100 | Low | Simple struct + extraction |
| scoring.rs | 80 | Low | Math operations only |
| strategies.rs | 120 | Medium | 4 distinct strategies |
| error.rs | 50 | Low | Error enum definitions |
| mod.rs (Router) | 300 | Medium | Main logic, filtering, aliases |
| Config changes | 100 | Low | Struct definitions, parsing |
| API integration | 50 | Low | Wiring only |
| **Total** | **~800** | **Medium** | |

---

## Implementation Order

1. **T01**: Create routing module structure
2. **T02**: Implement RequestRequirements extraction
3. **T03**: Implement basic candidate filtering
4. **T04**: Implement RoutingError types
5. **T05**: Implement scoring function
6. **T06**: Implement smart strategy
7. **T07**: Implement round-robin strategy
8. **T08**: Implement priority-only strategy
9. **T09**: Implement random strategy
10. **T10**: Implement alias resolution
11. **T11**: Implement fallback chains
12. **T12**: Add RoutingConfig to config.rs
13. **T13**: Integrate router with API handlers
14. **T14**: Add integration tests
15. **T15**: Performance validation

---

## Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Routing latency | < 1ms p99 | Benchmark test |
| Test coverage | > 90% | cargo-tarpaulin |
| All strategies work | Pass | Unit + integration tests |
| No regressions | Pass | Existing tests still pass |
