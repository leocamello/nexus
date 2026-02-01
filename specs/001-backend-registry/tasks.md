# Implementation Tasks: Backend Registry

**Spec**: [spec.md](./spec.md)  
**Plan**: [plan.md](./plan.md)  
**Status**: Ready for Implementation

## Task Overview

| Task | Description | Est. Time | Dependencies |
|------|-------------|-----------|--------------|
| T01 | Project setup & module scaffolding | 1h | None |
| T02 | Implement enums & Model struct | 1.5h | T01 |
| T03 | Implement Backend struct & BackendView | 2h | T02 |
| T04 | Implement RegistryError | 1h | T01 |
| T05 | Implement Registry core (add/remove/get) | 2.5h | T03, T04 |
| T06 | Implement model index & queries | 2h | T05 |
| T07 | Implement status & model updates | 2h | T06 |
| T08 | Implement atomic counters | 2.5h | T05 |
| T09 | Add property-based tests | 1.5h | T08 |
| T10 | Concurrency stress tests | 2h | T06, T08 |
| T11 | Integration, docs & benchmarks | 2h | All |

**Total Estimated Time**: ~20 hours

---

## T01: Project Setup & Module Scaffolding

**Goal**: Create module structure and add dev dependency.

**Files to create/modify**:
- `src/lib.rs` (create)
- `src/registry/mod.rs` (create)
- `src/registry/backend.rs` (create, empty)
- `src/registry/error.rs` (create, empty)
- `src/registry/tests.rs` (create, empty)
- `Cargo.toml` (add proptest)

**Implementation Steps**:
1. Add `proptest = "1"` to `[dev-dependencies]` in Cargo.toml
2. Create `src/lib.rs`:
   ```rust
   pub mod registry;
   ```
3. Create `src/registry/mod.rs`:
   ```rust
   mod backend;
   mod error;
   #[cfg(test)]
   mod tests;
   
   pub use backend::*;
   pub use error::*;
   ```
4. Create empty placeholder files for backend.rs, error.rs, tests.rs
5. Run `cargo check` to verify structure compiles

**Acceptance Criteria**:
- [ ] `cargo check` passes with no errors
- [ ] Module structure matches plan's file layout
- [ ] proptest is available in dev-dependencies

**Test Command**: `cargo check`

---

## T02: Implement Enums & Model Struct

**Goal**: Define BackendType, BackendStatus, DiscoverySource, and Model with serialization.

**Files to modify**:
- `src/registry/backend.rs`
- `src/registry/tests.rs`

**Tests to Write First** (in tests.rs):
```rust
#[test]
fn test_backend_type_serialization() {
    // BackendType::Ollama serializes to "ollama"
}

#[test]
fn test_backend_status_serialization() {
    // BackendStatus::Healthy serializes to "healthy"
}

#[test]
fn test_discovery_source_serialization() {
    // DiscoverySource::MDNS serializes to "mdns"
}

#[test]
fn test_model_creation() {
    // Model can be created with all fields
}

#[test]
fn test_model_json_roundtrip() {
    // Model serializes to JSON and deserializes back
}
```

**Implementation Steps**:
1. Write the 5 tests (they will fail - no impl yet)
2. Implement enums with `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]`
3. Use `#[serde(rename_all = "lowercase")]` for enum variants
4. Implement Model struct with all fields from spec
5. Run tests until all pass

**Acceptance Criteria**:
- [ ] All 5 tests pass
- [ ] Enums serialize to lowercase strings
- [ ] Model has all fields: id, name, context_length, supports_vision, supports_tools, supports_json_mode, max_output_tokens
- [ ] `cargo clippy` passes

**Test Command**: `cargo test registry::tests::test_model -- --nocapture`

---

## T03: Implement Backend Struct & BackendView

**Goal**: Define Backend with atomic fields and BackendView for serialization.

**Files to modify**:
- `src/registry/backend.rs`
- `src/registry/tests.rs`

**Tests to Write First**:
```rust
#[test]
fn test_backend_creation() {
    // Backend can be created with all fields
}

#[test]
fn test_backend_view_from_backend() {
    // BackendView can be created from Backend
}

#[test]
fn test_backend_view_json_roundtrip() {
    // BackendView serializes to JSON correctly
}

#[test]
fn test_backend_default_values() {
    // New backend has sensible defaults (pending=0, total=0, etc.)
}
```

**Implementation Steps**:
1. Write the 4 tests (they will fail)
2. Implement Backend struct:
   - Regular fields: id, name, url, backend_type, status, last_health_check, last_error, models, priority, discovery_source, metadata
   - Atomic fields: pending_requests (AtomicU32), total_requests (AtomicU64), avg_latency_ms (AtomicU32)
3. Implement `Backend::new()` constructor with sensible defaults
4. Implement `BackendView` struct (all non-atomic, for serialization)
5. Implement `From<&Backend> for BackendView`
6. Run tests until all pass

**Acceptance Criteria**:
- [ ] All 4 tests pass
- [ ] Backend has all fields from spec
- [ ] Atomic fields use std::sync::atomic types
- [ ] BackendView serializes to valid JSON
- [ ] `cargo clippy` passes

**Test Command**: `cargo test registry::tests::test_backend -- --nocapture`

---

## T04: Implement RegistryError

**Goal**: Define error types for registry operations.

**Files to modify**:
- `src/registry/error.rs`
- `src/registry/tests.rs`

**Tests to Write First**:
```rust
#[test]
fn test_error_duplicate_backend() {
    // DuplicateBackend error contains the ID
}

#[test]
fn test_error_backend_not_found() {
    // BackendNotFound error contains the ID
}

#[test]
fn test_error_display() {
    // Errors implement Display with useful messages
}
```

**Implementation Steps**:
1. Write the 3 tests
2. Implement RegistryError enum with thiserror:
   ```rust
   #[derive(Debug, thiserror::Error)]
   pub enum RegistryError {
       #[error("backend already exists: {0}")]
       DuplicateBackend(String),
       
       #[error("backend not found: {0}")]
       BackendNotFound(String),
   }
   ```
3. Run tests until all pass

**Acceptance Criteria**:
- [ ] All 3 tests pass
- [ ] Errors implement std::error::Error via thiserror
- [ ] Error messages include relevant IDs
- [ ] `cargo clippy` passes

**Test Command**: `cargo test registry::tests::test_error -- --nocapture`

---

## T05: Implement Registry Core (add/remove/get)

**Goal**: Implement Registry struct with basic CRUD operations.

**Files to modify**:
- `src/registry/mod.rs`
- `src/registry/tests.rs`

**Tests to Write First**:
```rust
#[test]
fn test_registry_new_empty() {
    // New registry has 0 backends
}

#[test]
fn test_add_backend_success() {
    // Adding backend stores it and can be retrieved
}

#[test]
fn test_add_backend_duplicate_error() {
    // Adding duplicate ID returns DuplicateBackend error
}

#[test]
fn test_remove_backend_success() {
    // Removing backend returns it and removes from registry
}

#[test]
fn test_remove_backend_not_found() {
    // Removing non-existent ID returns BackendNotFound error
}

#[test]
fn test_get_backend_found() {
    // Getting existing backend returns Some(backend)
}

#[test]
fn test_get_backend_not_found() {
    // Getting non-existent ID returns None
}

#[test]
fn test_get_all_backends() {
    // Returns all registered backends
}

#[test]
fn test_backend_count() {
    // Returns correct count after add/remove
}
```

**Implementation Steps**:
1. Write all 9 tests
2. Implement Registry struct:
   ```rust
   pub struct Registry {
       backends: DashMap<String, Backend>,
       model_index: DashMap<String, Vec<String>>,
   }
   ```
3. Implement `Registry::new()`
4. Implement `add_backend()` - check for duplicate, insert
5. Implement `remove_backend()` - remove, return, or error
6. Implement `get_backend()` - clone and return
7. Implement `get_all_backends()` - iterate and collect
8. Implement `backend_count()` - return len
9. Run tests until all pass

**Acceptance Criteria**:
- [ ] All 9 tests pass
- [ ] Registry uses DashMap for storage
- [ ] add_backend returns error on duplicate
- [ ] remove_backend returns error if not found
- [ ] `cargo clippy` passes

**Test Command**: `cargo test registry::tests::test_registry -- --nocapture`

---

## T06: Implement Model Index & Queries

**Goal**: Implement model-to-backend index and query operations.

**Files to modify**:
- `src/registry/mod.rs`
- `src/registry/tests.rs`

**Tests to Write First**:
```rust
#[test]
fn test_model_index_updated_on_add() {
    // Adding backend updates model index
}

#[test]
fn test_model_index_updated_on_remove() {
    // Removing backend cleans up model index
}

#[test]
fn test_get_backends_for_model_single() {
    // Returns single backend with model
}

#[test]
fn test_get_backends_for_model_multiple() {
    // Returns multiple backends with same model
}

#[test]
fn test_get_backends_for_model_none() {
    // Returns empty vec for unknown model
}

#[test]
fn test_get_healthy_backends_includes_healthy() {
    // Includes backends with Healthy status
}

#[test]
fn test_get_healthy_backends_excludes_unhealthy() {
    // Excludes backends with Unhealthy status
}

#[test]
fn test_get_healthy_backends_excludes_draining() {
    // Excludes backends with Draining status
}

#[test]
fn test_model_count() {
    // Counts unique models across all backends
}
```

**Implementation Steps**:
1. Write all 9 tests
2. Update `add_backend()` to populate model_index:
   - For each model in backend.models, add backend.id to index
3. Update `remove_backend()` to cleanup model_index:
   - For each model, remove backend.id from index
   - Remove empty model entries
4. Implement `get_backends_for_model()`:
   - Lookup model in index, fetch each backend
5. Implement `get_healthy_backends()`:
   - Filter all backends by status == Healthy
6. Implement `model_count()`:
   - Return model_index.len()
7. Run tests until all pass

**Acceptance Criteria**:
- [ ] All 9 tests pass
- [ ] Model index is always in sync with backend models
- [ ] get_backends_for_model uses O(1) index lookup
- [ ] get_healthy_backends only returns Healthy status
- [ ] `cargo clippy` passes

**Test Command**: `cargo test registry::tests::test_model_index -- --nocapture`

---

## T07: Implement Status & Model Updates

**Goal**: Implement update_status and update_models operations.

**Files to modify**:
- `src/registry/mod.rs`
- `src/registry/tests.rs`

**Tests to Write First**:
```rust
#[test]
fn test_update_status_changes_status() {
    // Status changes from Healthy to Unhealthy
}

#[test]
fn test_update_status_sets_timestamp() {
    // last_health_check is updated
}

#[test]
fn test_update_status_sets_error() {
    // last_error is set when status is Unhealthy
}

#[test]
fn test_update_status_clears_error() {
    // last_error is cleared when status becomes Healthy
}

#[test]
fn test_update_status_not_found() {
    // Returns error for unknown backend ID
}

#[test]
fn test_update_models_replaces_list() {
    // Model list is completely replaced
}

#[test]
fn test_update_models_updates_index() {
    // Model index reflects new models, removes old
}

#[test]
fn test_update_models_not_found() {
    // Returns error for unknown backend ID
}
```

**Implementation Steps**:
1. Write all 8 tests
2. Implement `update_status()`:
   - Get mutable ref from DashMap
   - Update status, last_health_check = Utc::now()
   - Set/clear last_error based on status
3. Implement `update_models()`:
   - Get mutable ref from DashMap
   - Remove old models from index
   - Replace backend.models
   - Add new models to index
4. Run tests until all pass

**Acceptance Criteria**:
- [ ] All 8 tests pass
- [ ] Status updates set timestamp to current time
- [ ] Model updates maintain index consistency
- [ ] Both return error for unknown backend
- [ ] `cargo clippy` passes

**Test Command**: `cargo test registry::tests::test_update -- --nocapture`

---

## T08: Implement Atomic Counters

**Goal**: Implement thread-safe increment_pending, decrement_pending, update_latency.

**Files to modify**:
- `src/registry/mod.rs`
- `src/registry/tests.rs`

**Tests to Write First**:
```rust
#[test]
fn test_increment_pending_success() {
    // pending_requests increases by 1
}

#[test]
fn test_increment_pending_returns_new_value() {
    // Returns the value after increment
}

#[test]
fn test_decrement_pending_success() {
    // pending_requests decreases by 1
}

#[test]
fn test_decrement_pending_clamps_to_zero() {
    // Never goes negative, stays at 0
}

#[test]
fn test_decrement_pending_at_zero_logs_warning() {
    // Tracing warning emitted (use tracing-test or mock)
}

#[test]
fn test_update_latency_first_sample() {
    // First sample sets the initial value
}

#[test]
fn test_update_latency_ema_calculation() {
    // Verify EMA: new = (sample + 4*old) / 5
}

#[test]
fn test_update_latency_zero_valid() {
    // 0ms is accepted as valid latency
}
```

**Implementation Steps**:
1. Write all 8 tests
2. Implement `increment_pending()`:
   ```rust
   let val = backend.pending_requests.fetch_add(1, Ordering::SeqCst);
   Ok(val + 1)
   ```
3. Implement `decrement_pending()`:
   - Use compare-exchange loop for saturating subtraction
   - Log warning via `tracing::warn!` if value was 0
4. Implement `update_latency()`:
   - If first sample (old == 0), set directly
   - Otherwise: `new = (sample + 4*old) / 5`
   - Use compare-exchange for atomic update
5. Run tests until all pass

**Acceptance Criteria**:
- [ ] All 8 tests pass
- [ ] increment uses fetch_add with SeqCst
- [ ] decrement uses saturating_sub behavior
- [ ] latency uses integer EMA formula
- [ ] Warning logged when decrementing at 0
- [ ] `cargo clippy` passes

**Test Command**: `cargo test registry::tests::test_atomic -- --nocapture`

---

## T09: Add Property-Based Tests

**Goal**: Use proptest to verify atomic counter invariants.

**Files to modify**:
- `src/registry/tests.rs`

**Property Tests to Write**:
```rust
proptest! {
    #[test]
    fn prop_increment_decrement_balanced(n in 1u32..100) {
        // n increments followed by n decrements = 0
    }
    
    #[test]
    fn prop_concurrent_increments_correct(n in 1u32..50) {
        // n concurrent increments result in pending_requests == n
    }
    
    #[test]
    fn prop_latency_bounded(samples in proptest::collection::vec(0u32..10000, 1..100)) {
        // After any sequence of updates, latency is within [min, max] of samples
    }
    
    #[test]
    fn prop_decrement_never_negative(decrements in 1u32..100) {
        // Any number of decrements on empty counter stays at 0
    }
}
```

**Implementation Steps**:
1. Add `use proptest::prelude::*;` to tests.rs
2. Write property tests using proptest! macro
3. Use `tokio::spawn` for concurrent increment test
4. Verify all properties hold

**Acceptance Criteria**:
- [ ] All 4 property tests pass
- [ ] Tests run with multiple random inputs
- [ ] No panics or race conditions detected
- [ ] `cargo test` completes in < 30s

**Test Command**: `cargo test registry::tests::prop_ -- --nocapture`

---

## T10: Concurrency Stress Tests

**Goal**: Verify thread-safety under heavy concurrent load.

**Files to modify**:
- `src/registry/tests.rs`

**Tests to Write**:
```rust
#[tokio::test]
async fn test_concurrent_reads_no_deadlock() {
    // 10,000 concurrent get_backend calls complete
}

#[tokio::test]
async fn test_concurrent_read_write_safe() {
    // Mixed read/write workload completes without panic
}

#[tokio::test]
async fn test_concurrent_add_remove_same_id() {
    // Concurrent add/remove of same ID: no panic, consistent state
}

#[tokio::test]
async fn test_concurrent_model_queries() {
    // Concurrent get_backends_for_model with updates: consistent results
}
```

**Implementation Steps**:
1. Write all 4 async tests
2. Use `Arc<Registry>` for shared access
3. Use `tokio::spawn` to create concurrent tasks
4. Use `tokio::time::timeout` to detect deadlocks
5. Verify consistent final state after all tasks complete

**Acceptance Criteria**:
- [ ] All 4 stress tests pass
- [ ] 10,000 concurrent reads complete in < 5s
- [ ] No panics, deadlocks, or data corruption
- [ ] Tests use tokio runtime

**Test Command**: `cargo test registry::tests::test_concurrent -- --nocapture`

---

## T11: Integration, Docs & Benchmarks

**Goal**: Final cleanup, documentation, and performance validation.

**Files to modify**:
- `src/lib.rs`
- `src/registry/mod.rs`
- `src/registry/backend.rs`
- `src/registry/error.rs`

**Tasks**:
1. Add doc comments to all public items:
   ```rust
   /// The Backend Registry stores all known LLM backends.
   /// 
   /// # Example
   /// ```
   /// use nexus::registry::Registry;
   /// let registry = Registry::new();
   /// ```
   pub struct Registry { ... }
   ```
2. Add `#![deny(missing_docs)]` to lib.rs
3. Run performance benchmark:
   - Create registry with 1000 backends, 10 models each
   - Measure get_backends_for_model() time
   - Assert < 1ms
4. Run memory estimation:
   - Create 100 backends
   - Log approximate memory usage
5. Run full test suite
6. Run clippy and fmt

**Acceptance Criteria**:
- [ ] `cargo test` - all tests pass
- [ ] `cargo clippy -- -D warnings` - no warnings
- [ ] `cargo fmt --check` - properly formatted
- [ ] `cargo doc --no-deps` - docs build without warnings
- [ ] Query benchmark < 1ms with 1000 backends
- [ ] All public items have doc comments

**Test Command**: `cargo test && cargo clippy -- -D warnings && cargo fmt --check`

---

## Execution Order

```
T01 ──► T02 ──► T03 ──┬──► T05 ──► T06 ──► T07 ──┬──► T10 ──► T11
                      │                          │
                      └──► T04 ──────────────────┘
                                                 │
                      T08 ◄──────────────────────┘
                        │
                        └──► T09
```

**Critical Path**: T01 → T02 → T03 → T05 → T06 → T10 → T11

## Progress Tracking

- [ ] **T01**: Project setup & module scaffolding
- [ ] **T02**: Implement enums & Model struct
- [ ] **T03**: Implement Backend struct & BackendView
- [ ] **T04**: Implement RegistryError
- [ ] **T05**: Implement Registry core (add/remove/get)
- [ ] **T06**: Implement model index & queries
- [ ] **T07**: Implement status & model updates
- [ ] **T08**: Implement atomic counters
- [ ] **T09**: Add property-based tests
- [ ] **T10**: Concurrency stress tests
- [ ] **T11**: Integration, docs & benchmarks
