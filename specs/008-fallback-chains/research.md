# Research: Fallback Chains (F08)

**Date**: 2026-02-08
**Status**: Implemented (PR #99)

This document captures the technical decisions made during F08 implementation, alternatives considered, and rationale for each choice. F08 builds on F06's partial fallback implementation, adding the `RoutingResult` struct and `X-Nexus-Fallback-Model` response header.

## Research Questions & Findings

### 1. Fallback Chain Storage Data Structure

**Question**: How should fallback chains be stored and accessed?

**Decision**: Use `HashMap<String, Vec<String>>` in the `Router` struct, populated from TOML config at startup. The key is the primary model name, the value is an ordered list of fallback model names.

**Rationale**:
- `HashMap` provides O(1) lookup by model name — called on every request where the primary model is unavailable
- `Vec<String>` preserves insertion order, which defines fallback priority (first entry is tried first)
- Immutable after construction — no need for concurrent write access
- Small dataset (typically < 10 entries with 2-3 fallbacks each)
- TOML array syntax maps naturally to `Vec<String>`

**Implementation**:
```rust
pub struct Router {
    // ...
    fallbacks: HashMap<String, Vec<String>>,
    // ...
}

// Config (nexus.toml):
// [routing.fallbacks]
// "llama3:70b" = ["qwen2:72b", "mixtral:8x7b"]
```

**Alternatives Considered**:
- **`HashMap<String, LinkedList<String>>`**: Rejected — linked list provides no benefit for ordered iteration over small lists; Vec is cache-friendlier
- **Priority queue per model**: Rejected — fallback order is static (config-defined), not dynamic; queue overhead is unnecessary
- **Flat list of `(primary, fallback, priority)` tuples**: Rejected — requires grouping by primary model at lookup time; HashMap pre-groups by key
- **Graph-based fallback structure**: Rejected — fallbacks are simple ordered lists, not graphs; no transitive fallback behavior is needed

---

### 2. Ordered Iteration Through Fallback Chain

**Question**: How should the router iterate through fallback models?

**Decision**: Linear iteration through the `Vec<String>` in order. For each fallback model, run the full `filter_candidates()` pipeline (health check, capability matching). Use the first fallback that has available candidates.

**Rationale**:
- Order matters: the first fallback is the operator's preferred alternative
- Each fallback gets the same capability filtering as the primary model — no shortcuts
- The chosen routing strategy (Smart, RoundRobin, etc.) applies within each fallback's candidate set
- Linear iteration is O(k) where k is the chain length (typically 2-3) — negligible overhead

**Implementation**:
```rust
// In select_backend(), after primary model fails:
let fallbacks = self.get_fallbacks(&model);
for fallback_model in &fallbacks {
    let candidates = self.filter_candidates(fallback_model, requirements);
    if !candidates.is_empty() {
        let (selected, mut route_reason) = match self.strategy {
            RoutingStrategy::Smart => { /* same scoring as primary */ }
            // ... other strategies
        };
        route_reason = format!("fallback:{}:{}", model, route_reason);
        tracing::warn!(requested_model = %model, fallback_model = %fallback_model,
            backend = %selected.name, "Using fallback model");
        return Ok(RoutingResult {
            backend: Arc::new(selected),
            actual_model: fallback_model.clone(),
            fallback_used: true,
            route_reason,
        });
    }
}
```

**Alternatives Considered**:
- **Parallel fallback evaluation**: Rejected — adds complexity for negligible benefit; fallback chains are short (2-3 entries) and filter_candidates is sub-millisecond
- **Score across all fallbacks simultaneously**: Rejected — violates the ordered priority semantics; a lower-priority fallback should never beat a higher-priority one
- **Skip capability filtering for fallbacks**: Rejected — would route vision requests to non-vision backends just because they're fallbacks; capabilities are non-negotiable

---

### 3. Fallback Chain Exhaustion Error Handling

**Question**: What happens when all models in a fallback chain are unavailable?

**Decision**: Return `RoutingError::FallbackChainExhausted { chain: Vec<String> }` which maps to HTTP 503 with an actionable error message listing the entire chain that was tried.

**Rationale**:
- 503 (Service Unavailable) is semantically correct — the service exists but can't serve the request right now
- Including the full chain in the error (`["llama3:70b", "qwen2:72b", "mixtral:8x7b"]`) tells the operator exactly what was tried
- Follows the constitutional principle: "503 with actionable context over silent quality downgrades"
- Distinct from `ModelNotFound` (model never existed) and `NoHealthyBackend` (model exists but backends are down)

**Implementation**:
```rust
// After all fallbacks exhausted:
if !fallbacks.is_empty() {
    let mut chain = vec![model.clone()];
    chain.extend(fallbacks);
    Err(RoutingError::FallbackChainExhausted { chain })
} else if model_exists {
    Err(RoutingError::NoHealthyBackend { model })
} else {
    Err(RoutingError::ModelNotFound { model: requirements.model.clone() })
}
```

**Alternatives Considered**:
- **Return last error only**: Rejected — loses context about what was tried; operators need to see the full chain to diagnose
- **HTTP 404**: Rejected — the model is configured, just unavailable; 404 suggests it was never set up
- **Silent fallback to any available model**: Rejected — violates explicit contracts principle; serving a random model when the chain is exhausted is a silent quality downgrade
- **Queue the request for retry**: Rejected — Nexus is stateless; request queuing is planned for v0.4 (F22)

---

### 4. X-Nexus-Fallback-Model Response Header

**Question**: How should the API communicate that a fallback model was used?

**Decision**: Add `X-Nexus-Fallback-Model: <actual_model>` response header only when `RoutingResult.fallback_used == true`. The response body's `model` field continues to show the requested model (OpenAI compatibility).

**Rationale**:
- Response body must not be modified — OpenAI API compatibility is a constitutional principle
- HTTP headers are the correct channel for proxy metadata (`X-` prefix for custom headers)
- Conditional injection keeps responses clean for the common case (primary model available)
- Header value is the actual model that served the request, enabling client-side awareness
- Lowercase header name (`x-nexus-fallback-model`) for HTTP/2 compliance

**Implementation**:
```rust
// In src/api/completions.rs, after routing:
if routing_result.fallback_used {
    response.headers_mut().insert(
        "x-nexus-fallback-model",
        HeaderValue::from_str(&routing_result.actual_model)?,
    );
}
// Response body model field = requested model (transparent to client)
```

**Alternatives Considered**:
- **Modify response body model field**: Rejected — breaks OpenAI API compatibility; clients parsing `response.model` would see unexpected values
- **Always include header (with primary model name when no fallback)**: Rejected — adds noise to every response; header absence is a clean signal that no fallback was needed
- **Use `X-Nexus-Route-Info` with JSON value**: Rejected — simpler single-value header is easier for clients to parse; route_reason is internal, not client-facing
- **Separate `/v1/route` endpoint for metadata**: Rejected — requires two requests; header piggybacks on the existing response

---

### 5. RoutingResult Struct Design

**Question**: How should `select_backend()` communicate fallback metadata to the API layer?

**Decision**: Return a `RoutingResult` struct instead of a bare `Arc<Backend>`. The struct carries the selected backend, actual model name, fallback flag, and route reason.

**Rationale**:
- The API layer needs to know whether a fallback was used (for header injection) and what model was actually used
- `route_reason` provides structured debugging information: `"fallback:llama3:70b:highest_score:98"`
- Adding fields to a struct is backward-compatible; adding return values to a function is not
- Clean separation: Router decides routing, API handler decides response formatting

**Implementation**:
```rust
pub struct RoutingResult {
    pub backend: Arc<Backend>,
    pub actual_model: String,
    pub fallback_used: bool,
    pub route_reason: String,
}
```

**Alternatives Considered**:
- **Return `(Arc<Backend>, Option<String>)` tuple**: Rejected — tuples don't self-document; `result.1` is less readable than `result.actual_model`
- **Return `Arc<Backend>` and set header in Router**: Rejected — Router shouldn't know about HTTP headers; violates separation of concerns
- **Use a trait with metadata methods**: Rejected — over-engineering for a data-carrying return type; struct is simpler

---

### 6. Integration with Routing Strategies

**Question**: How do fallback chains interact with the configured routing strategy?

**Decision**: The same routing strategy applies to both primary and fallback model selection. When a fallback is tried, `filter_candidates()` + strategy selection runs identically to the primary model path.

**Rationale**:
- Consistency: if the operator chose `round_robin`, they expect round-robin for all model selections, including fallbacks
- Code reuse: the same strategy dispatch logic handles both paths, reducing duplication
- The `route_reason` includes strategy context prefixed with `fallback:`: e.g., `"fallback:llama3:70b:round_robin:index_2"`

**Implementation**:
```rust
// Fallback path reuses the same strategy match:
for fallback_model in &fallbacks {
    let candidates = self.filter_candidates(fallback_model, requirements);
    if !candidates.is_empty() {
        let (selected, mut route_reason) = match self.strategy {
            RoutingStrategy::Smart => { self.select_smart(&candidates) /* ... */ }
            RoutingStrategy::RoundRobin => { /* same AtomicU64 counter */ }
            RoutingStrategy::PriorityOnly => { self.select_priority_only(&candidates) }
            RoutingStrategy::Random => { self.select_random(&candidates) }
        };
        route_reason = format!("fallback:{}:{}", model, route_reason);
        // ...
    }
}
```

**Alternatives Considered**:
- **Always use Smart for fallbacks**: Rejected — overrides operator's strategy choice; inconsistent behavior between primary and fallback
- **Separate fallback strategy config**: Rejected — adds configuration complexity with unclear value; most operators want consistent behavior
- **Reset round-robin counter for fallbacks**: Rejected — shared counter ensures global distribution; resetting would bias fallback selection

---

### 7. Fallback WARN Logging

**Question**: At what level should fallback usage be logged?

**Decision**: Log at `WARN` level when a fallback model is used, including the requested model, fallback model, and selected backend.

**Rationale**:
- Fallback usage indicates a capacity or availability issue — operators should be aware
- `WARN` is appropriate: the request succeeded (not ERROR) but something was suboptimal (not INFO)
- Structured fields (`requested_model`, `fallback_model`, `backend`) enable log filtering and alerting
- Matches the log level progression established in F11: INFO for success, WARN for degraded, ERROR for failure

**Implementation**:
```rust
tracing::warn!(
    requested_model = %model,
    fallback_model = %fallback_model,
    backend = %selected.name,
    "Using fallback model"
);
```

**Alternatives Considered**:
- **INFO level**: Rejected — fallback usage is a signal that something is wrong; INFO would bury it in routine logs
- **ERROR level**: Rejected — the request succeeded; ERROR should be reserved for actual failures
- **DEBUG level**: Rejected — too low; operators need fallback visibility in production without enabling debug logging

---

### 8. Alias-Then-Fallback Interaction

**Question**: How do aliases and fallbacks interact? Can an alias target have fallbacks?

**Decision**: Aliases resolve first, then fallback chains are looked up using the resolved model name. This means fallbacks should be defined for actual model names, not alias names.

**Rationale**:
- Clean pipeline: `alias_resolution → primary_lookup → fallback_chain`
- Avoids ambiguity: is the fallback for the alias or the target?
- Enables: `"gpt-4"` → (alias) → `"llama3:70b"` → (fallback) → `["qwen2:72b", "mixtral:8x7b"]`
- Fallback config uses model names that exist in backends, not user-facing alias names

**Implementation**:
```rust
// In select_backend():
let model = self.resolve_alias(&requirements.model);  // Step 1: Resolve alias
let candidates = self.filter_candidates(&model, ...);  // Step 2: Try resolved model
// Step 3: If no candidates, try fallbacks for the RESOLVED model name
let fallbacks = self.get_fallbacks(&model);
```

**Alternatives Considered**:
- **Look up fallbacks for both alias and resolved name**: Rejected — creates ambiguity about which fallback chain wins; single lookup point is deterministic
- **Resolve aliases within fallback chains**: Rejected — would allow `fallbacks: "a" → ["b"]` where `b` is an alias; mixing indirection layers adds complexity without clear benefit
- **Separate alias and fallback resolution**: Rejected — they're naturally sequential; alias is naming, fallback is availability

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Entire fallback chain unavailable | Medium | `FallbackChainExhausted` error with full chain for diagnostics |
| Fallback model has different capabilities | Medium | Same `filter_candidates()` pipeline applies; capability-incompatible fallbacks are skipped |
| Header not visible to clients behind proxies | Low | Standard HTTP header; proxies typically forward custom headers |
| Large fallback chains add latency | Low | Each fallback iteration is sub-ms; chains > 5 are unusual |
| Alias + fallback misconfiguration | Medium | Define fallbacks for resolved model names; documented in config examples |

---

## References

- [HTTP custom headers convention](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers)
- [OpenAI API error handling](https://platform.openai.com/docs/guides/error-codes)
- [Nexus LEARNINGS.md - F08 section](../../docs/LEARNINGS.md)
- [F06 Intelligent Router research](../006-intelligent-router/research.md)
