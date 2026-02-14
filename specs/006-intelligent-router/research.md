# Research: Intelligent Router (F06)

**Date**: 2026-02-08
**Status**: Implemented (PR #87)

This document captures the technical decisions made during F06 implementation, alternatives considered, and rationale for each choice.

## Research Questions & Findings

### 1. Capability-First Filtering Architecture

**Question**: In what order should the router evaluate candidates — filter by capabilities first, or score all backends and filter later?

**Decision**: Filter by capabilities (health, vision, tools, context length, JSON mode) *before* scoring by load/latency. Two-phase pipeline: `filter_candidates()` → strategy selection.

**Rationale**:
- Prevents a fast-but-incapable backend from winning selection (e.g., a low-latency backend that doesn't support vision)
- Reduces the candidate set before expensive scoring computations
- Makes the routing decision predictable: "can this backend serve the request?" is answered before "which capable backend is best?"
- Follows the principle of explicit failures — if no backend supports the capability, return `CapabilityMismatch` error immediately

**Implementation**:
```rust
fn filter_candidates(&self, model: &str, requirements: &RequestRequirements) -> Vec<Backend> {
    let mut candidates = self.registry.get_backends_for_model(model);
    candidates.retain(|b| b.status == BackendStatus::Healthy);
    candidates.retain(|backend| {
        if let Some(model_info) = backend.models.iter().find(|m| m.id == model) {
            if requirements.needs_vision && !model_info.supports_vision { return false; }
            if requirements.needs_tools && !model_info.supports_tools { return false; }
            if requirements.needs_json_mode && !model_info.supports_json_mode { return false; }
            if requirements.estimated_tokens > model_info.context_length { return false; }
            true
        } else { false }
    });
    candidates
}
```

**Alternatives Considered**:
- **Score-then-filter**: Rejected — a high-scoring backend without required capabilities wastes computation and creates confusing "why was this backend skipped?" debugging scenarios
- **Capability penalty in scoring**: Rejected — missing capabilities should be a hard filter (binary yes/no), not a soft score reduction that might be outweighed by other factors
- **Separate capability index**: Rejected — adds data structure complexity; the Model struct already carries capability flags that are cheap to check

---

### 2. Scoring Weights Design (Priority 50 / Load 30 / Latency 20)

**Question**: How should the scoring function weight priority, load, and latency?

**Decision**: Default weights of `priority=50, load=30, latency=20`, must sum to 100, configurable via TOML.

**Rationale**:
- Priority gets 50% because operators explicitly assign it — human intent should dominate automatic signals
- Load gets 30% because pending request count directly reflects current capacity pressure
- Latency gets 20% because it's an EMA (exponential moving average) that smooths out spikes — useful but less immediately actionable than load
- Sum-to-100 constraint ensures weights are interpretable as percentages
- Validation at config load prevents misconfiguration (e.g., weights summing to 150)

**Implementation**:
```rust
pub fn score_backend(
    priority: u32, pending_requests: u32, avg_latency_ms: u32, weights: &ScoringWeights,
) -> u32 {
    let priority_score = 100 - priority.min(100);          // Lower priority number = higher score
    let load_score = 100 - pending_requests.min(100);      // Fewer pending = higher score
    let latency_score = 100 - (avg_latency_ms / 10).min(100); // 0ms=100, 1000ms=0

    (priority_score * weights.priority + load_score * weights.load
        + latency_score * weights.latency) / 100
}
```

**Alternatives Considered**:
- **Equal weights (33/33/34)**: Rejected — doesn't respect operator intent; a priority-1 backend should almost always win over priority-10 regardless of load
- **Latency-dominant weighting**: Rejected — latency varies by model size and request complexity; a 70B model will always be slower than a 7B model, but that doesn't make it worse
- **Floating-point weights**: Rejected — integer arithmetic is simpler, faster, and avoids floating-point precision issues in the hot path
- **Machine-learned weights**: Rejected — premature optimization; static configurable weights are transparent and debuggable

---

### 3. Four Routing Strategy Patterns

**Question**: What routing strategies should Nexus support, and how should they be dispatched?

**Decision**: Four strategies via enum dispatch: `Smart` (default), `RoundRobin`, `PriorityOnly`, `Random`.

**Rationale**:
- **Smart**: Weighted multi-factor scoring — best for heterogeneous fleets with varying priorities and loads
- **RoundRobin**: Uniform distribution — best for homogeneous backends where fairness matters
- **PriorityOnly**: Deterministic selection by priority number — best for active/standby configurations
- **Random**: Statistical distribution — useful for testing and when no preference exists
- Enum dispatch (match statement) instead of trait objects keeps the routing path allocation-free and under 1ms

**Implementation**:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoutingStrategy {
    #[default]
    Smart,
    RoundRobin,
    PriorityOnly,
    Random,
}

// In select_backend():
let (selected, route_reason) = match self.strategy {
    RoutingStrategy::Smart => { /* max_by_key on score */ }
    RoutingStrategy::RoundRobin => { /* AtomicU64 counter % len */ }
    RoutingStrategy::PriorityOnly => { /* min_by_key on priority */ }
    RoutingStrategy::Random => { /* RandomState hash of SystemTime */ }
};
```

**Alternatives Considered**:
- **Trait object (`Box<dyn Strategy>`)**: Rejected — adds heap allocation per routing decision; enum dispatch is zero-cost and the strategy set is closed (no plugins planned)
- **Weighted random**: Rejected — harder to reason about than deterministic smart scoring; would conflate randomness with intentional weighting
- **Least-connections only**: Rejected — too narrow; covered by Smart strategy with load weight set to 100

---

### 4. Lock-Free Round-Robin with AtomicU64

**Question**: How do we implement thread-safe round-robin without mutexes?

**Decision**: Use `AtomicU64` counter with `Ordering::Relaxed`, modulo candidate count.

**Rationale**:
- `fetch_add(1, Relaxed)` is a single CPU instruction on modern architectures
- `Ordering::Relaxed` is sufficient because we don't need happens-before guarantees — occasional duplicate selection under extreme concurrency is acceptable for round-robin
- No lock contention means routing stays under 1ms even with hundreds of concurrent requests
- Counter wraps at `u64::MAX` (~18.4 quintillion) — effectively infinite

**Implementation**:
```rust
round_robin_counter: AtomicU64::new(0),

// In select_backend():
let counter = self.round_robin_counter.fetch_add(1, Ordering::Relaxed);
let index = (counter as usize) % candidates.len();
let backend = &candidates[index];
```

**Alternatives Considered**:
- **`Mutex<usize>` counter**: Rejected — mutex contention under load; unnecessarily heavy for a simple counter
- **`AtomicU64` with `Ordering::SeqCst`**: Rejected — stronger ordering provides no benefit for round-robin fairness; adds unnecessary memory fence overhead
- **Thread-local counters**: Rejected — would create per-thread round-robin cycles that don't distribute globally

---

### 5. Request Inspection for Vision/Tools Detection

**Question**: How do we determine if a request needs vision or tool-use capabilities?

**Decision**: Inspect the `ChatCompletionRequest` payload at routing time. Vision is detected by `image_url` content parts; tools by `tools` key in extra fields; JSON mode by `response_format.type == "json_object"`.

**Rationale**:
- Request payload is the authoritative source for what capabilities are needed
- Sub-millisecond payload inspection (no I/O) fits within the routing latency budget
- `MessageContent` enum already distinguishes `Text` from `Parts` (multimodal content)
- `extra` HashMap captures OpenAI-compatible fields like `tools` and `response_format` without needing typed fields for every possible parameter

**Implementation**:
```rust
pub fn from_request(request: &ChatCompletionRequest) -> RequestRequirements {
    let mut needs_vision = false;
    for message in &request.messages {
        match &message.content {
            MessageContent::Parts { content } => {
                for part in content {
                    if part.part_type == "image_url" { needs_vision = true; }
                }
            }
            _ => {}
        }
    }
    let needs_tools = request.extra.contains_key("tools");
    let needs_json_mode = request.extra
        .get("response_format")
        .and_then(|v| v.as_object())
        .and_then(|obj| obj.get("type"))
        .and_then(|v| v.as_str())
        .map(|t| t == "json_object")
        .unwrap_or(false);
    // ...
}
```

**Alternatives Considered**:
- **Model name heuristics**: Rejected — model names don't reliably indicate capabilities (e.g., `llama3` may or may not support tools depending on the backend)
- **Backend capability probing at request time**: Rejected — adds network I/O to the hot path; capabilities are already known from health checks
- **Client-provided capability headers**: Rejected — shifts burden to clients and violates OpenAI API compatibility

---

### 6. Token Estimation Approach

**Question**: How do we estimate token count for context length checking?

**Decision**: Simple `chars / 4` heuristic applied to all text content in the request messages.

**Rationale**:
- OpenAI's rule of thumb: ~4 characters per token for English text
- Provides a reasonable approximation without requiring a tokenizer dependency
- Context length check is a safety gate, not a precise budget — overestimating slightly is acceptable
- Avoids pulling in `tiktoken` or model-specific tokenizers which would add 10+ MB to the binary

**Implementation**:
```rust
estimated_tokens += content.len() as u32 / 4;
```

**Alternatives Considered**:
- **Exact tokenizer (tiktoken)**: Rejected — adds significant binary size, model-specific complexity, and latency; precision isn't needed for a safety gate
- **Word count / 0.75**: Rejected — word splitting is locale-dependent and slower than character division
- **Skip estimation entirely**: Rejected — would allow requests to be routed to backends with insufficient context windows, causing downstream errors

---

### 7. No I/O in the Routing Hot Path

**Question**: How do we ensure routing decisions stay under 1ms?

**Decision**: The router reads directly from the Registry's `DashMap` — no network calls, no disk I/O, no async operations in the scoring path.

**Rationale**:
- `DashMap` provides lock-free concurrent reads via shard-based locking
- Backend state (priority, pending requests, latency EMA) is maintained by background tasks (health checker)
- The router consumes pre-computed state, never generates it
- Measured routing decision time: sub-100µs for typical fleet sizes (< 50 backends)

**Alternatives Considered**:
- **Real-time backend health check during routing**: Rejected — adds 50-500ms per routing decision; unacceptable for the latency budget
- **Redis/external state store**: Rejected — introduces network dependency and failure mode; in-memory DashMap is faster and simpler
- **Channel-based state updates**: Rejected — adds indirection; direct DashMap reads are simpler and faster

---

### 8. RoutingResult Return Type

**Question**: What should `select_backend()` return?

**Decision**: Return a `RoutingResult` struct containing the selected backend, actual model name, fallback flag, and route reason.

**Rationale**:
- Carries metadata needed by the API layer (was a fallback used? which model?)
- `route_reason` enables debugging: `"highest_score:Backend A:98"`, `"round_robin:index_3"`, `"only_healthy_backend"`
- Separates routing logic from header injection — the API handler decides what to do with fallback metadata
- Extensible for future metadata (e.g., privacy zone, cost estimate)

**Implementation**:
```rust
pub struct RoutingResult {
    pub backend: Arc<Backend>,
    pub actual_model: String,
    pub fallback_used: bool,
    pub route_reason: String,
}
```

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Scoring weights misconfiguration | Medium | Validate sum == 100 at config load; reject invalid weights |
| Capability data stale (model unloaded) | Medium | Health checker refreshes model lists periodically |
| Token estimation inaccuracy | Low | Heuristic overestimates slightly; safety gate, not budget |
| Round-robin unfairness under candidate churn | Low | Acceptable for round-robin; Smart strategy handles this better |
| DashMap contention under extreme load | Low | Shard-based locking minimizes contention; benchmarked < 100µs |

---

## References

- [DashMap documentation](https://docs.rs/dashmap/latest/dashmap/)
- [Atomic ordering in Rust](https://doc.rust-lang.org/std/sync/atomic/enum.Ordering.html)
- [OpenAI Chat Completions API](https://platform.openai.com/docs/api-reference/chat)
- [Nexus LEARNINGS.md - F06 section](../../docs/LEARNINGS.md)
