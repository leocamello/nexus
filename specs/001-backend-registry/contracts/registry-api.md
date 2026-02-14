# Backend Registry API Contract

This document defines the internal Registry API — the source of truth for all backend and model state in Nexus.

**Source**: `src/registry/mod.rs`, `src/registry/backend.rs`

---

## Overview

The Registry is a thread-safe, in-memory store backed by `DashMap` (lock-free concurrent hash maps). It stores backends keyed by ID and maintains a secondary model-to-backend index for fast model lookups.

**Thread Safety**: All operations are safe to call from multiple async tasks concurrently. Atomic counters (`AtomicU32`, `AtomicU64`) are used for load tracking without locks.

---

## Data Types

### `BackendType`

Enum indicating API compatibility of a backend.

```rust
pub enum BackendType {
    Ollama,    // Ollama API (/api/tags)
    VLLM,      // vLLM OpenAI-compatible API
    LlamaCpp,  // llama.cpp server (/health)
    Exo,       // Exo distributed inference
    OpenAI,    // OpenAI API
    LMStudio,  // LM Studio
    Generic,   // Generic/unknown backend
}
```

**Serialization**: `#[serde(rename_all = "lowercase")]` — serializes as `"ollama"`, `"vllm"`, `"llamacpp"`, etc.

### `BackendStatus`

```rust
pub enum BackendStatus {
    Healthy,    // Accepting requests
    Unhealthy,  // Failed health check
    Unknown,    // Not yet checked (initial state)
    Draining,   // Healthy but not accepting new requests
}
```

**Serialization**: `#[serde(rename_all = "lowercase")]`

### `DiscoverySource`

```rust
pub enum DiscoverySource {
    Static,  // Configured in TOML config file
    MDNS,    // Auto-discovered via mDNS
    Manual,  // Added via CLI at runtime
}
```

**Serialization**: `#[serde(rename_all = "lowercase")]`

### `Model`

Represents an LLM model available on a backend.

```rust
pub struct Model {
    pub id: String,                    // e.g., "llama3:70b"
    pub name: String,                  // Human-readable name
    pub context_length: u32,           // Max context window (tokens)
    pub supports_vision: bool,         // Image input support
    pub supports_tools: bool,          // Function/tool calling support
    pub supports_json_mode: bool,      // JSON mode support
    pub max_output_tokens: Option<u32>, // Output limit (if any)
}
```

**JSON Serialization**:
```json
{
  "id": "llama3:70b",
  "name": "llama3:70b",
  "context_length": 4096,
  "supports_vision": false,
  "supports_tools": false,
  "supports_json_mode": false,
  "max_output_tokens": null
}
```

### `Backend`

Internal type with atomic counters for thread-safe runtime state. **Not directly serializable** — convert to `BackendView` for JSON output.

```rust
pub struct Backend {
    pub id: String,
    pub name: String,
    pub url: String,
    pub backend_type: BackendType,
    pub status: BackendStatus,
    pub last_health_check: DateTime<Utc>,
    pub last_error: Option<String>,
    pub models: Vec<Model>,
    pub priority: i32,                      // Lower = preferred
    pub pending_requests: AtomicU32,        // In-flight request count
    pub total_requests: AtomicU64,          // Lifetime request count
    pub avg_latency_ms: AtomicU32,          // EMA latency (α=0.2)
    pub discovery_source: DiscoverySource,
    pub metadata: HashMap<String, String>,
}
```

**Constructor**:
```rust
Backend::new(
    id: String,
    name: String,
    url: String,
    backend_type: BackendType,
    models: Vec<Model>,
    discovery_source: DiscoverySource,
    metadata: HashMap<String, String>,
) -> Self
```

Initial state: `status = Unknown`, all atomic counters = 0, `priority = 0`, `last_error = None`.

### `BackendView`

Serializable view of `Backend`. Atomic fields are read and stored as plain integers.

```rust
pub struct BackendView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub backend_type: BackendType,
    pub status: BackendStatus,
    pub last_health_check: DateTime<Utc>,
    pub last_error: Option<String>,
    pub models: Vec<Model>,
    pub priority: i32,
    pub pending_requests: u32,
    pub total_requests: u64,
    pub avg_latency_ms: u32,
    pub discovery_source: DiscoverySource,
    pub metadata: HashMap<String, String>,
}
```

**Conversion**: `impl From<&Backend> for BackendView` — reads all atomics with `SeqCst` ordering.

**JSON Example**:
```json
{
  "id": "ollama-local",
  "name": "Local Ollama",
  "url": "http://localhost:11434",
  "backend_type": "ollama",
  "status": "healthy",
  "last_health_check": "2024-02-14T10:30:45.123Z",
  "last_error": null,
  "models": [
    {
      "id": "llama3:70b",
      "name": "llama3:70b",
      "context_length": 4096,
      "supports_vision": false,
      "supports_tools": false,
      "supports_json_mode": false,
      "max_output_tokens": null
    }
  ],
  "priority": 1,
  "pending_requests": 2,
  "total_requests": 1500,
  "avg_latency_ms": 850,
  "discovery_source": "static",
  "metadata": {}
}
```

---

## Registry Operations

### `Registry::new() -> Self`

Creates an empty registry with no backends.

### `add_backend(backend: Backend) -> Result<(), RegistryError>`

Add a backend to the registry.

- **Precondition**: No backend with the same `id` exists.
- **Side effect**: Updates model index for all models in the backend.
- **Error**: `RegistryError::DuplicateBackend(id)` if ID already exists.

### `remove_backend(id: &str) -> Result<Backend, RegistryError>`

Remove a backend by ID and return it.

- **Side effect**: Cleans up model index entries. Removes model index entries entirely if no other backends serve that model.
- **Error**: `RegistryError::BackendNotFound(id)` if ID not found.

### `get_backend(id: &str) -> Option<Backend>`

Get a cloned copy of a backend by ID. Atomic counter values are snapshot at call time.

### `get_all_backends() -> Vec<Backend>`

Get cloned copies of all registered backends.

### `get_backends_for_model(model_id: &str) -> Vec<Backend>`

Get all backends that serve a specific model (via model index lookup).

### `get_healthy_backends() -> Vec<Backend>`

Get all backends with `status == BackendStatus::Healthy`.

### `backend_count() -> usize`

Number of registered backends.

### `model_count() -> usize`

Number of unique models across all backends (from model index).

### `update_status(id: &str, status: BackendStatus, error: Option<String>) -> Result<(), RegistryError>`

Update a backend's health status. Also sets `last_health_check` to `Utc::now()` and updates `last_error`.

- **Error**: `RegistryError::BackendNotFound(id)`.

### `update_models(id: &str, new_models: Vec<Model>) -> Result<(), RegistryError>`

Replace a backend's entire model list. Removes old model index entries and adds new ones.

- **Error**: `RegistryError::BackendNotFound(id)`.

### `increment_pending(id: &str) -> Result<u32, RegistryError>`

Atomically increment pending request counter. Returns new value.

- **Ordering**: `SeqCst` (fetch_add).
- **Error**: `RegistryError::BackendNotFound(id)`.

### `decrement_pending(id: &str) -> Result<u32, RegistryError>`

Atomically decrement pending request counter (saturating at 0). Uses compare-exchange loop.

- If already at 0, logs a warning via `tracing::warn!` and returns 0.
- **Error**: `RegistryError::BackendNotFound(id)`.

### `update_latency(id: &str, latency_ms: u32) -> Result<(), RegistryError>`

Update rolling average latency using EMA with α=0.2.

- **Formula**: `new = (sample + 4 × old) / 5`
- **First sample**: Sets value directly (when current == 0).
- Uses compare-exchange loop for lock-free updates.
- **Error**: `RegistryError::BackendNotFound(id)`.

---

## mDNS Support Methods

### `has_backend_url(url: &str) -> bool`

Check if any backend has the given URL. Comparison is normalized (trailing slashes stripped).

### `set_mdns_instance(id: &str, instance: &str) -> Result<(), RegistryError>`

Store an mDNS instance name in the backend's `metadata` under key `"mdns_instance"`.

### `find_by_mdns_instance(instance: &str) -> Option<String>`

Find a backend ID by its mDNS instance name (searches metadata).

---

## Error Types

```rust
pub enum RegistryError {
    DuplicateBackend(String),  // Backend ID already exists
    BackendNotFound(String),   // No backend with given ID
}
```

---

## Thread-Safety Guarantees

| Component | Mechanism | Details |
|-----------|-----------|---------|
| Backend storage | `DashMap<String, Backend>` | Lock-free concurrent reads/writes |
| Model index | `DashMap<String, Vec<String>>` | Secondary index, updated on add/remove/update |
| Pending requests | `AtomicU32` | `fetch_add` / compare-exchange loop |
| Total requests | `AtomicU64` | `fetch_add` |
| Average latency | `AtomicU32` | Compare-exchange loop with EMA |
| All atomic ops | `SeqCst` ordering | Strongest consistency guarantee |

---

## Implementation Notes

### Model Index

The model index (`DashMap<String, Vec<String>>`) maps model IDs to lists of backend IDs that serve them. This enables O(1) lookup for "which backends serve model X?" without iterating all backends.

The index is maintained automatically:
- **add_backend**: Adds entries for all models in the backend
- **remove_backend**: Removes entries and cleans up empty model entries
- **update_models**: Removes old entries, adds new entries

### Backend Cloning

`get_backend()` and `get_all_backends()` return deep clones with atomic values snapshot at call time. This is necessary because `Backend` contains `AtomicU32`/`AtomicU64` which are not `Clone` — values are read with `SeqCst` and used to construct new atomics.

### URL Normalization

The `normalize_url()` function strips trailing slashes for comparison, so `http://localhost:11434` and `http://localhost:11434/` are treated as the same URL.

---

## Testing Strategy

### Unit Tests
1. Add/remove backends and verify counts
2. Duplicate backend detection
3. Backend not found errors
4. Model index consistency after add/remove/update
5. Atomic counter operations (increment, decrement, saturation)
6. EMA latency computation (first sample, steady state)
7. URL normalization edge cases
8. mDNS instance lookup

### Integration Tests
1. Concurrent add/remove from multiple tasks
2. Model index consistency under concurrent updates
3. Atomic counter accuracy under contention
