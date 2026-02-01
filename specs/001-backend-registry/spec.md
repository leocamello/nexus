# Feature Specification: Backend Registry

**Feature Branch**: `001-backend-registry`  
**Created**: 2026-02-01  
**Status**: ✅ Implemented  
**Priority**: P0 (MVP)  
**PR**: [#12](https://github.com/leocamello/nexus/pull/12)

## Overview

In-memory data store tracking all known backends and their models. This is the **source of truth** for all backend state in Nexus. Every other component (Health Checker, Router, API Gateway, CLI) depends on the Registry.

## User Scenarios & Testing

### User Story 1 - Register Static Backends (Priority: P1)

As a user, I want to configure backends in a TOML file so that Nexus knows where to route requests when it starts.

**Why this priority**: Without at least one backend registered, Nexus cannot serve any requests. This is the foundation.

**Independent Test**: Can be fully tested by creating a Registry, adding a backend, and verifying it can be retrieved.

**Acceptance Scenarios**:

1. **Given** an empty registry, **When** I add a backend with valid URL and type, **Then** the backend is stored and can be retrieved by ID
2. **Given** a registry with one backend, **When** I add another backend with the same ID, **Then** the operation returns an error (duplicate ID)
3. **Given** a registry with backends, **When** I serialize to JSON, **Then** all backend data is correctly represented

---

### User Story 2 - Query Backends by Model (Priority: P1)

As the Router, I need to find all backends that serve a specific model so I can select the best one for a request.

**Why this priority**: This is the primary query path for every incoming request. Performance is critical.

**Independent Test**: Can be tested by registering multiple backends with overlapping models and querying by model name.

**Acceptance Scenarios**:

1. **Given** backends A (models: llama3, mistral) and B (models: llama3, qwen), **When** I query for "llama3", **Then** both A and B are returned
2. **Given** backends with various models, **When** I query for a non-existent model, **Then** an empty list is returned
3. **Given** 100 backends with 10 models each, **When** I query by model, **Then** response time is < 1ms

---

### User Story 3 - Track Backend Health Status (Priority: P1)

As the Health Checker, I need to update backend status so unhealthy backends are excluded from routing.

**Why this priority**: Health status directly affects routing decisions and system reliability.

**Independent Test**: Can be tested by updating status and verifying get_healthy_backends() filters correctly.

**Acceptance Scenarios**:

1. **Given** a healthy backend, **When** I update status to Unhealthy, **Then** get_healthy_backends() excludes it
2. **Given** an unhealthy backend, **When** I update status to Healthy, **Then** get_healthy_backends() includes it
3. **Given** a status update, **When** I check last_health_check, **Then** it reflects the update timestamp

---

### User Story 4 - Track In-Flight Requests (Priority: P2)

As the Router, I need to track pending requests per backend for load-aware routing decisions.

**Why this priority**: Load balancing improves with this, but basic routing works without it.

**Independent Test**: Can be tested by incrementing/decrementing counters and verifying atomic behavior.

**Acceptance Scenarios**:

1. **Given** a backend with 0 pending requests, **When** I call increment_pending(), **Then** pending_requests becomes 1
2. **Given** a backend with 5 pending requests, **When** I call decrement_pending(), **Then** pending_requests becomes 4
3. **Given** 100 concurrent increment/decrement calls, **When** all complete, **Then** the final count is correct (no race conditions)

---

### User Story 5 - Update Model List (Priority: P2)

As the Health Checker, I need to update the model list when backends report new or removed models.

**Why this priority**: Model discovery is important but can be added after basic health checking works.

**Independent Test**: Can be tested by updating models and verifying the model-to-backend index updates.

**Acceptance Scenarios**:

1. **Given** a backend with models [A, B], **When** I update to [B, C], **Then** model A is removed and C is added
2. **Given** model index, **When** I update backend models, **Then** get_backends_for_model() reflects changes immediately
3. **Given** a model update, **When** another thread queries, **Then** it sees a consistent state (no partial updates)

---

### User Story 6 - Remove Backends (Priority: P2)

As the Discovery service, I need to remove backends that are no longer available.

**Why this priority**: Removal is less frequent than queries; backends typically stay registered.

**Independent Test**: Can be tested by adding then removing a backend and verifying cleanup.

**Acceptance Scenarios**:

1. **Given** a registered backend, **When** I remove it by ID, **Then** it is no longer retrievable
2. **Given** a removed backend, **When** I query by its models, **Then** it does not appear in results
3. **Given** a non-existent ID, **When** I try to remove it, **Then** the operation returns an error

---

### Edge Cases

- What happens when pending_requests is decremented below 0? → Clamp to 0 (saturating_sub), log warning via tracing
- What happens when update_latency receives 0ms? → Accept it (valid for cached responses)
- What happens with concurrent add and remove of same ID? → Last operation wins, no panic
- What happens when model index has stale entries? → Rebuild index on inconsistency detection
- What happens with extremely long model names (>1000 chars)? → Accept but log warning

## Requirements

### Functional Requirements

- **FR-001**: Registry MUST store Backend structs with all specified fields
- **FR-002**: Registry MUST store Model structs with all specified fields
- **FR-003**: Registry MUST support adding backends with unique IDs
- **FR-004**: Registry MUST support removing backends by ID
- **FR-005**: Registry MUST support querying all backends
- **FR-006**: Registry MUST support querying healthy backends only
- **FR-007**: Registry MUST support querying backends by model name
- **FR-008**: Registry MUST maintain a model-to-backend index for O(1) lookup
- **FR-009**: Registry MUST support updating backend status
- **FR-010**: Registry MUST support updating backend model list
- **FR-011**: Registry MUST support atomic increment/decrement of pending_requests
- **FR-012**: Registry MUST support updating rolling average latency
- **FR-013**: Registry MUST be serializable to JSON for debugging

### Non-Functional Requirements

- **NFR-001**: All operations MUST be thread-safe
- **NFR-002**: Read operations MUST not block other reads
- **NFR-003**: Query by model MUST complete in < 1ms for up to 1000 backends
- **NFR-004**: Memory usage MUST be < 10KB per backend (Backend struct + Models + metadata HashMap)
- **NFR-005**: Registry MUST handle 10,000+ concurrent read operations

### Key Entities

- **Backend**: Represents an LLM inference server (Ollama, vLLM, etc.)
  - Has unique ID, connection URL, health status, and capability metadata
  - Contains zero or more Models
  - Tracks runtime statistics (pending requests, latency)

- **Model**: Represents an available LLM model on a backend
  - Has unique identifier within a backend
  - Defines capabilities (context length, vision, tools, JSON mode)
  - Can exist on multiple backends simultaneously

- **BackendType**: Enum categorizing the backend's API compatibility
  - Ollama, VLLM, LlamaCpp, Exo, OpenAI, Generic

- **BackendStatus**: Enum representing health state
  - Healthy, Unhealthy, Unknown, Draining

- **DiscoverySource**: Enum indicating how backend was discovered
  - Static (config file), MDNS (auto-discovered), Manual (CLI)

## Data Structures

### Backend

```rust
pub struct Backend {
    pub id: String,                           // UUID
    pub name: String,                         // Human-readable name
    pub url: String,                          // Base URL (e.g., "http://localhost:11434")
    pub backend_type: BackendType,
    pub status: BackendStatus,
    pub last_health_check: DateTime<Utc>,
    pub last_error: Option<String>,
    pub models: Vec<Model>,
    pub priority: i32,                        // Lower = prefer
    pub pending_requests: AtomicU32,          // Current in-flight
    pub total_requests: AtomicU64,            // Lifetime total
    pub avg_latency_ms: AtomicU32,            // Exponential moving average (α=0.2)
    pub discovery_source: DiscoverySource,
    pub metadata: HashMap<String, String>,
}
```

### Model

```rust
pub struct Model {
    pub id: String,                 // Model identifier (e.g., "llama3:70b")
    pub name: String,               // Display name
    pub context_length: u32,        // Max context window
    pub supports_vision: bool,
    pub supports_tools: bool,
    pub supports_json_mode: bool,
    pub max_output_tokens: Option<u32>,
}
```

### Enums

```rust
pub enum BackendType {
    Ollama,
    VLLM,
    LlamaCpp,
    Exo,
    OpenAI,
    Generic,
}

pub enum BackendStatus {
    Healthy,
    Unhealthy,
    Unknown,
    Draining,  // Healthy but not accepting new requests
}

pub enum DiscoverySource {
    Static,    // From config file
    MDNS,      // Auto-discovered via mDNS
    Manual,    // Added via CLI at runtime
}
```

## Operations

| Operation | Signature | Description |
|-----------|-----------|-------------|
| add_backend | `(backend: Backend) -> Result<(), RegistryError>` | Add new backend; error if ID exists |
| remove_backend | `(id: &str) -> Result<Backend, RegistryError>` | Remove and return; error if not found |
| get_backend | `(id: &str) -> Option<Backend>` | Get by ID; None if not found |
| get_all_backends | `() -> Vec<Backend>` | List all backends |
| get_healthy_backends | `() -> Vec<Backend>` | Filter to Healthy status only |
| get_backends_for_model | `(model: &str) -> Vec<Backend>` | Find backends serving model |
| update_status | `(id: &str, status: BackendStatus, error: Option<String>) -> Result<(), RegistryError>` | Update health status |
| update_models | `(id: &str, models: Vec<Model>) -> Result<(), RegistryError>` | Replace model list |
| increment_pending | `(id: &str) -> Result<u32, RegistryError>` | Atomic increment; returns new value |
| decrement_pending | `(id: &str) -> Result<u32, RegistryError>` | Atomic decrement; uses saturating_sub, logs warning if was 0 |
| update_latency | `(id: &str, latency_ms: u32) -> Result<(), RegistryError>` | Update EMA: new = α*sample + (1-α)*old, α=0.2 |
| backend_count | `() -> usize` | Total registered backends |
| model_count | `() -> usize` | Total unique models across all backends |

## Success Criteria

### Measurable Outcomes

- **SC-001**: All Registry operations complete without panics under concurrent access
- **SC-002**: get_backends_for_model() completes in < 1ms with 1000 backends
- **SC-003**: 10,000 concurrent reads complete without deadlock
- **SC-004**: Memory usage is < 10KB per backend (measured with 100 backends)
- **SC-005**: Serialization to JSON produces valid, parseable output
- **SC-006**: All atomic counter operations maintain consistency under stress test

### Definition of Done

- [x] All data structures implemented with proper derives
- [x] All operations implemented and documented
- [x] Unit tests for every operation
- [x] Concurrent access stress tests pass
- [x] Property-based tests for counter operations
- [x] JSON serialization/deserialization works
- [x] Code passes clippy and fmt checks
- [x] Module has `#[cfg(test)] mod tests` block
