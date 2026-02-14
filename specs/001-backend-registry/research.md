# Research: Backend Registry (F01)

**Date**: 2026-02-03
**Phase**: Implemented (v0.1)

This document captures the technical decisions made during implementation of the Backend Registry — the central source of truth for all LLM backend state in Nexus.

## Research Questions & Findings

### 1. Concurrent Data Structure for Backend Storage

**Question**: How should we store backends for concurrent read/write access across async tasks?

**Decision**: Use `DashMap<String, Backend>` from the `dashmap` crate (v6).

**Rationale**:
- The registry is read on every request (routing) and written by health checker and mDNS discovery
- `DashMap` provides lock-free reads and shard-based writes — reads never block each other
- No `async` required — all operations are synchronous, which simplifies the API
- Shard count scales automatically with the number of entries

**Alternatives Considered**:
- `RwLock<HashMap<String, Backend>>`: Rejected because a single write blocks all reads. Under load, health checker writes would stall routing decisions. Acceptable for small deployments but doesn't match Nexus's zero-config scaling goal.
- `Arc<Mutex<HashMap>>`: Rejected because even reads require exclusive access. Worst concurrency profile of all options.
- `tokio::sync::RwLock<HashMap>`: Rejected because it's async-aware but still a single lock. Adds `.await` to every registry access, complicating the routing hot path.
- `evmap` (eventually consistent map): Rejected because it trades consistency for read performance. Stale backend status during routing could send requests to unhealthy backends.

**Implementation**:
```rust
pub struct Registry {
    backends: DashMap<String, Backend>,
    model_index: DashMap<String, Vec<String>>,
}
```

**References**:
- https://docs.rs/dashmap/6/dashmap/
- https://github.com/xacrimon/dashmap

---

### 2. Atomic Counters vs Mutex for Runtime Metrics

**Question**: How should we track per-backend runtime metrics (pending requests, total requests, latency)?

**Decision**: Use `AtomicU32` and `AtomicU64` directly on the `Backend` struct with `SeqCst` ordering.

**Rationale**:
- Pending requests are incremented/decremented on every proxied request — this is the hottest path
- Atomic operations are lock-free and complete in a single CPU instruction
- `SeqCst` ordering ensures all threads see consistent values for routing decisions
- No mutex contention means the routing budget (< 1ms) is easily met

**Alternatives Considered**:
- `Mutex<u32>` per counter: Rejected because lock contention scales linearly with request volume. A busy backend with 100 concurrent requests would have 100 threads contending on the same mutex.
- `RwLock<BackendMetrics>` struct: Rejected because writes (every request) would block reads (every routing decision). Worse than individual atomics.
- `Ordering::Relaxed`: Considered for `pending_requests` since occasional stale values wouldn't cause correctness issues. Rejected in favor of `SeqCst` for consistency across all counters — the performance difference is negligible on x86.

**Implementation**:
```rust
pub struct Backend {
    pub pending_requests: AtomicU32,
    pub total_requests: AtomicU64,
    pub avg_latency_ms: AtomicU32,
    // ...
}
```

---

### 3. Latency Tracking Algorithm

**Question**: How should we compute rolling average latency for routing decisions?

**Decision**: Exponential Moving Average (EMA) with α=0.2, implemented as integer math: `new = (sample + 4×old) / 5`.

**Rationale**:
- EMA gives recent samples more weight, which is essential for detecting backend degradation quickly
- α=0.2 means the last 5 samples contribute ~67% of the value — responsive but not jittery
- Integer math avoids floating-point atomics (which don't exist in std) and rounding complexity
- Compare-exchange loop ensures correctness under concurrent updates without locks

**Alternatives Considered**:
- Simple arithmetic mean: Rejected because it weights all historical samples equally. A backend that was slow 1000 requests ago would still drag down the average, masking current performance.
- Sliding window (last N samples): Rejected because it requires a buffer (Vec or ring buffer), which means a lock to protect the buffer. EMA achieves the same recency bias with a single atomic.
- Histogram-based percentiles: Rejected as premature complexity. P50/P99 tracking is valuable for observability (added in F09) but overkill for routing decisions where relative comparison is sufficient.

**Implementation**:
```rust
pub fn update_latency(&self, id: &str, latency_ms: u32) -> Result<(), RegistryError> {
    loop {
        let current = backend.avg_latency_ms.load(Ordering::SeqCst);
        let new_val = if current == 0 {
            latency_ms  // First sample sets initial value
        } else {
            (latency_ms + 4 * current) / 5  // EMA: α=0.2
        };
        match backend.avg_latency_ms.compare_exchange(
            current, new_val, Ordering::SeqCst, Ordering::SeqCst
        ) {
            Ok(_) => return Ok(()),
            Err(_) => continue,  // CAS retry on concurrent modification
        }
    }
}
```

---

### 4. BackendView Separation Pattern

**Question**: How should we serialize Backend state for API responses and CLI output?

**Decision**: Create a separate `BackendView` struct with plain values and implement `From<&Backend>`.

**Rationale**:
- `AtomicU32`/`AtomicU64` cannot derive `Serialize` — serde doesn't support atomic types
- Mixing serialization concerns into the `Backend` struct would require custom serde implementations on every atomic field
- The view model pattern cleanly separates internal representation from external contracts
- `From<&Backend>` provides zero-ceremony conversion at the call site

**Alternatives Considered**:
- Custom `Serialize` implementation on `Backend`: Rejected because it couples the internal struct to a specific serialization format. If we later need different views (summary vs detailed), we'd need to add serialization flags.
- `#[serde(with = "atomic_serde")]` attribute: Rejected because no well-maintained crate exists for this, and writing custom serializers for each atomic field is error-prone.
- Wrapping atomics in `Arc<AtomicU32>` with a newtype: Rejected as unnecessary indirection. The view model is simpler and more idiomatic.

**Implementation**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendView {
    pub pending_requests: u32,  // Plain u32, not AtomicU32
    pub total_requests: u64,
    pub avg_latency_ms: u32,
    // ... other fields
}

impl From<&Backend> for BackendView {
    fn from(backend: &Backend) -> Self {
        Self {
            pending_requests: backend.pending_requests.load(Ordering::SeqCst),
            // ...
        }
    }
}
```

**References**:
- LEARNINGS.md: "View Models for CLI Output — Separating internal types from display types prevents coupling"

---

### 5. In-Memory vs Persistent Storage

**Question**: Should the registry persist backend state across restarts?

**Decision**: In-memory only. No persistence layer.

**Rationale**:
- Nexus is a stateless control plane — backends are rediscovered on startup via config + mDNS
- Persistent state would require a storage backend (SQLite, file, etc.), violating the single-binary principle
- Runtime metrics (pending requests, latency EMA) are only meaningful for the current process lifetime
- Static backends come from the config file; mDNS backends are rediscovered within seconds
- Persisting state adds complexity for disaster recovery, schema migration, and corruption handling

**Alternatives Considered**:
- SQLite via `rusqlite`: Rejected because it adds a native dependency (libsqlite3), complicates cross-compilation, and breaks the zero-dependency deployment model.
- File-based JSON snapshot: Rejected because it introduces race conditions between the snapshot writer and crash recovery. The snapshot could be stale or corrupt.
- `sled` embedded database: Rejected because it adds ~2MB to binary size and introduces a background compaction thread. Overkill for a registry that typically holds 2-10 backends.

---

### 6. Model Index Design

**Question**: How should we support fast model-to-backend lookups?

**Decision**: Maintain a secondary `DashMap<String, Vec<String>>` mapping model IDs to backend IDs.

**Rationale**:
- The primary query pattern is "which backends serve model X?" (every chat completion request)
- Without an index, every lookup would scan all backends and their model lists — O(backends × models)
- The index provides O(1) lookup by model ID, returning the backend IDs to fetch
- Index updates happen only during health checks (model discovery) and backend add/remove — infrequent compared to reads

**Alternatives Considered**:
- Linear scan on every request: Rejected because it scales poorly. With 10 backends × 50 models each, that's 500 comparisons per request.
- Inverted index with `HashMap<String, HashSet<String>>`: Rejected because `HashSet` has higher memory overhead per entry and `Vec` is sufficient since backend counts are small (typically < 20).
- No separate index, rely on `DashMap::iter()`: Rejected because iteration holds read references to multiple shards, increasing contention.

**Implementation**:
```rust
pub struct Registry {
    backends: DashMap<String, Backend>,
    model_index: DashMap<String, Vec<String>>,
}

pub fn get_backends_for_model(&self, model_id: &str) -> Vec<Backend> {
    if let Some(backend_ids) = self.model_index.get(model_id) {
        backend_ids.iter()
            .filter_map(|id| self.get_backend(id))
            .collect()
    } else {
        Vec::new()
    }
}
```

---

### 7. Saturating Decrement for Pending Requests

**Question**: How should we handle the edge case where `decrement_pending` is called when the counter is already at 0?

**Decision**: Use a compare-exchange loop with saturating subtraction and log a warning.

**Rationale**:
- Underflow to `u32::MAX` would make the backend appear to have 4 billion pending requests, breaking routing
- The compare-exchange loop handles concurrent modifications without locks
- Warning log helps diagnose bugs (double-decrement) without crashing the server
- Saturating at 0 is the safe default — worst case, routing slightly overestimates backend capacity

**Implementation**:
```rust
pub fn decrement_pending(&self, id: &str) -> Result<u32, RegistryError> {
    loop {
        let current = backend.pending_requests.load(Ordering::SeqCst);
        if current == 0 {
            tracing::warn!(backend_id = %id,
                "Attempted to decrement pending_requests when already at 0");
            return Ok(0);
        }
        match backend.pending_requests.compare_exchange(
            current, current - 1, Ordering::SeqCst, Ordering::SeqCst
        ) {
            Ok(_) => return Ok(current - 1),
            Err(_) => continue,
        }
    }
}
```

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| DashMap shard contention under extreme write load | Medium | Health checker writes are periodic (30s), not per-request. Contention only possible with many concurrent backend additions. |
| EMA integer math loses precision for small latencies | Low | Minimum granularity is 1ms. For backends with 0-1ms latency, the EMA would show 0-1ms — still directionally correct for routing. |
| Model index inconsistency if update_models crashes mid-operation | Medium | Old models removed before new ones added. Crash between steps leaves the index incomplete but self-heals on next health check. |
| Clone overhead for `get_all_backends()` | Low | Each clone copies atomics via `load()` + struct fields. Acceptable for O(10) backends. Would need refactoring for O(1000). |

---

## References

- [DashMap documentation](https://docs.rs/dashmap/6/dashmap/)
- [Rust atomic types](https://doc.rust-lang.org/std/sync/atomic/)
- [Exponential Moving Average](https://en.wikipedia.org/wiki/Moving_average#Exponential_moving_average)
- LEARNINGS.md: "View Models for CLI Output"
