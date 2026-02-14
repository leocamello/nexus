# Data Model: Backend Registry (F01)

**Date**: 2025-01-10  
**Phase**: Phase 1 - Foundation

This document defines the data entities and their relationships for the Backend Registry feature.

## Core Entities

### 1. Backend

**Purpose**: Represents an LLM inference backend with configuration, capabilities, and runtime metrics.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `id` | `String` | Unique identifier, typically UUID |
| `name` | `String` | Human-readable, non-empty |
| `url` | `String` | Valid HTTP/HTTPS URL |
| `backend_type` | `BackendType` | Enum: Ollama, VLLM, LlamaCpp, Exo, OpenAI, LMStudio, Generic |
| `status` | `BackendStatus` | Enum: Healthy, Unhealthy, Unknown, Draining |
| `last_health_check` | `DateTime<Utc>` | Initialized to `Utc::now()` at creation |
| `last_error` | `Option<String>` | Most recent error message, `None` when healthy |
| `models` | `Vec<Model>` | Available models, may be empty initially |
| `priority` | `i32` | Lower = preferred; default 0 |
| `pending_requests` | `AtomicU32` | In-flight request count; initialized to 0 |
| `total_requests` | `AtomicU64` | Lifetime request count; initialized to 0 |
| `avg_latency_ms` | `AtomicU32` | Rolling EMA latency (α=0.2); initialized to 0 |
| `discovery_source` | `DiscoverySource` | Enum: Static, MDNS, Manual |
| `metadata` | `HashMap<String, String>` | Extensible key-value pairs (e.g., `mdns_instance`) |

**Responsibilities**:
- Hold backend identity, configuration, and runtime state
- Provide atomic counters for lock-free concurrent metric updates
- Store model capabilities for routing decisions

**Lifecycle**: Created via `Backend::new()` with status `Unknown` and all counters at 0. Status transitions managed by HealthChecker. Removed via `Registry::remove_backend()`.

**Thread Safety**: Atomic types (`AtomicU32`, `AtomicU64`) for counters enable lock-free reads and writes. Non-atomic fields (`status`, `models`, `last_error`) are mutated only through `Registry` methods that acquire DashMap write guards.

---

### 2. BackendView

**Purpose**: Serializable snapshot of a Backend for JSON output and API responses.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `id` | `String` | Copied from Backend |
| `name` | `String` | Copied from Backend |
| `url` | `String` | Copied from Backend |
| `backend_type` | `BackendType` | Copied from Backend |
| `status` | `BackendStatus` | Copied from Backend |
| `last_health_check` | `DateTime<Utc>` | Copied from Backend |
| `last_error` | `Option<String>` | Copied from Backend |
| `models` | `Vec<Model>` | Cloned from Backend |
| `priority` | `i32` | Copied from Backend |
| `pending_requests` | `u32` | Loaded from AtomicU32 (SeqCst) |
| `total_requests` | `u64` | Loaded from AtomicU64 (SeqCst) |
| `avg_latency_ms` | `u32` | Loaded from AtomicU32 (SeqCst) |
| `discovery_source` | `DiscoverySource` | Copied from Backend |
| `metadata` | `HashMap<String, String>` | Cloned from Backend |

**Responsibilities**:
- Provide `Serialize`/`Deserialize` support (atomics cannot derive Serialize)
- Capture point-in-time snapshot of backend state

**Lifecycle**: Created on-demand via `From<&Backend>` conversion. Short-lived — used for serialization then dropped.

**Thread Safety**: Plain data type with no interior mutability. Safe to send across threads.

---

### 3. Model

**Purpose**: Represents an LLM model's identity and capability flags for routing decisions.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `id` | `String` | Unique model identifier (e.g., `"llama3:70b"`) |
| `name` | `String` | Human-readable name |
| `context_length` | `u32` | Maximum context window in tokens; default 4096 |
| `supports_vision` | `bool` | Whether model accepts image inputs |
| `supports_tools` | `bool` | Whether model supports function/tool calling |
| `supports_json_mode` | `bool` | Whether model supports structured JSON output |
| `max_output_tokens` | `Option<u32>` | Maximum output tokens if limited |

**Responsibilities**:
- Carry capability metadata for capability-aware routing
- Enable Router to filter backends by required features (vision, tools, context window)

**Lifecycle**: Created during health check response parsing. Updated when HealthChecker refreshes model lists. Stored inside `Backend.models`.

**Thread Safety**: Immutable value type implementing `Clone`, `PartialEq`, `Eq`. Safe to share.

---

### 4. Registry

**Purpose**: Thread-safe in-memory store and query engine for all known backends and models.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `backends` | `DashMap<String, Backend>` | Keyed by backend ID |
| `model_index` | `DashMap<String, Vec<String>>` | Model ID → list of backend IDs |

**Responsibilities**:
- CRUD operations: `add_backend`, `remove_backend`, `get_backend`, `get_all_backends`
- Query operations: `get_backends_for_model`, `get_healthy_backends`
- Status management: `update_status`, `update_models`
- Atomic metric operations: `increment_pending`, `decrement_pending`, `update_latency`
- mDNS support: `has_backend_url`, `set_mdns_instance`, `find_by_mdns_instance`
- Maintain `model_index` as secondary index for model→backend lookups

**Lifecycle**: Created once at startup. Shared via `Arc<Registry>` across HealthChecker, Router, API handlers, and mDNS discovery.

**Thread Safety**: `DashMap` provides lock-free concurrent reads and fine-grained write locks per key. No global locks.

---

### 5. BackendType (Enum)

**Purpose**: Identifies the API compatibility of a backend for health check endpoint selection and response parsing.

**Variants**:

| Variant | Health Endpoint | Response Format |
|---------|----------------|-----------------|
| `Ollama` | `/api/tags` | Ollama tags JSON |
| `VLLM` | `/v1/models` | OpenAI format |
| `LlamaCpp` | `/health` | `{"status": "ok"}` |
| `Exo` | `/v1/models` | OpenAI format |
| `OpenAI` | `/v1/models` | OpenAI format |
| `LMStudio` | `/v1/models` | OpenAI format |
| `Generic` | `/v1/models` | OpenAI format |

**Serde**: `rename_all = "lowercase"` for TOML/JSON compatibility.

---

### 6. BackendStatus (Enum)

**Purpose**: Tracks backend availability for routing decisions.

**Variants**:

| Variant | Receives Requests | Description |
|---------|-------------------|-------------|
| `Healthy` | Yes | Passed health check |
| `Unhealthy` | No | Failed health check threshold |
| `Unknown` | No | Not yet checked (initial state) |
| `Draining` | No | Healthy but not accepting new requests |

---

### 7. DiscoverySource (Enum)

**Purpose**: Records how a backend was added to the registry.

**Variants**: `Static` (config file), `MDNS` (auto-discovered), `Manual` (CLI runtime).

---

### 8. RegistryError (Enum)

**Purpose**: Error types for registry operations.

**Variants**:

| Variant | Trigger |
|---------|---------|
| `DuplicateBackend(String)` | `add_backend` with existing ID |
| `BackendNotFound(String)` | Operation on non-existent backend ID |

---

## Entity Relationships

```
┌────────────────────────┐
│       Registry         │
│                        │
│  backends: DashMap     │──────┐
│    <String, Backend>   │      │
│                        │      │
│  model_index: DashMap  │──┐   │
│    <String, Vec<ID>>   │  │   │
└────────────────────────┘  │   │
                            │   │
         model ID ──────────┘   │
                                │
                    ┌───────────┘
                    ▼
          ┌──────────────────┐
          │     Backend      │
          │                  │
          │  id, name, url   │
          │  backend_type    │
          │  status          │
          │  models: Vec ────┼──────┐
          │  priority        │      │
          │  pending (atomic)│      │
          │  total (atomic)  │      │
          │  latency (atomic)│      │
          │  metadata        │      │
          └──────────────────┘      │
                    │               │
                    │ From<&Backend>│
                    ▼               ▼
          ┌────────────────┐  ┌──────────┐
          │  BackendView   │  │  Model   │
          │                │  │          │
          │  (serializable │  │  id      │
          │   snapshot)    │  │  name    │
          └────────────────┘  │  context │
                              │  vision  │
                              │  tools   │
                              │  json    │
                              └──────────┘
```

---

## State Transitions

### BackendStatus Lifecycle

```
                   ┌───────────┐
          ┌───────►│  Unknown  │◄─── (initial state)
          │        └─────┬─────┘
          │              │
          │    first success / first failure
          │              │
          │        ┌─────┴──────┐
          │        ▼            ▼
     ┌─────────┐         ┌───────────┐
     │ Healthy │         │ Unhealthy │
     └────┬────┘         └─────┬─────┘
          │                    │
          │ failures ≥         │ successes ≥
          │ failure_threshold  │ recovery_threshold
          │                    │
          └────────►───────────┘
                    (bidirectional)

     ┌──────────┐
     │ Draining │  ← set manually, not by health checker
     └──────────┘
```

### Model Index Maintenance

On `add_backend`: Each model ID is added to `model_index[model.id]` → `[backend.id]`.

On `remove_backend`: Each model ID entry has the backend ID removed. Empty entries are deleted.

On `update_models`: Old model entries removed from index, new model entries added.

---

## Validation & Constraints

### Backend ID Uniqueness

**Rule**: `add_backend` rejects duplicate IDs with `RegistryError::DuplicateBackend`.

**Implementation**: `DashMap::contains_key` check before insertion.

---

### URL Normalization

**Rule**: URL comparisons strip trailing slashes for deduplication.

**Implementation**:
```rust
fn normalize_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}
```

Used by `has_backend_url` for mDNS deduplication.

---

### Pending Request Saturation

**Rule**: `decrement_pending` saturates at 0 (never underflows).

**Implementation**: Compare-exchange loop with `AtomicU32`. Logs warning if already at 0.

---

### Latency EMA Calculation

**Rule**: Rolling average uses EMA with α=0.2: `new = (sample + 4×old) / 5`.

**First Sample**: Sets value directly (avoids averaging with 0).

**Implementation**: Compare-exchange loop for atomic thread-safe update.

---

### Atomic Ordering

**Consistency**: All atomic operations use `Ordering::SeqCst` for maximum correctness. This is slightly slower than `Relaxed` but prevents subtle ordering bugs across threads.

---

## Performance Characteristics

| Operation | Complexity | Latency | Implementation |
|-----------|-----------|---------|----------------|
| `add_backend` | O(M) | < 1µs | DashMap insert + model index updates |
| `remove_backend` | O(M) | < 1µs | DashMap remove + model index cleanup |
| `get_backend` | O(1) | < 100ns | DashMap get + deep clone |
| `get_all_backends` | O(N) | < 10µs | Iterate + clone all |
| `get_backends_for_model` | O(K) | < 1µs | Index lookup + K backend gets |
| `get_healthy_backends` | O(N) | < 10µs | Iterate + filter + clone |
| `update_status` | O(1) | < 100ns | DashMap get_mut + field write |
| `update_models` | O(M) | < 1µs | Index cleanup + replacement |
| `increment_pending` | O(1) | < 50ns | `fetch_add(1, SeqCst)` |
| `decrement_pending` | O(1) | < 50ns | Compare-exchange loop |
| `update_latency` | O(1) | < 50ns | Compare-exchange loop |
| `has_backend_url` | O(N) | < 10µs | Linear scan with URL normalization |

Where N = number of backends, M = models per backend, K = backends per model.

**Memory**: Each `Backend` ~500 bytes + ~100 bytes per model. For 100 backends with 10 models each: ~150 KB total.

---

## Future Extensions

### Not in Current Scope

1. **Persistent storage**: Registry is in-memory only; restarts lose state
2. **Backend groups/tags**: No grouping beyond `BackendType`
3. **VRAM tracking**: No GPU memory awareness
4. **Backend versioning**: No model version pinning
5. **TTL/expiry**: No automatic removal of stale backends

These are mentioned for awareness but are NOT part of F01 implementation.
