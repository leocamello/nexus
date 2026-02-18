# Implementation Plan: Backend Registry

**Spec**: [spec.md](./spec.md)  
**Status**: Ready for Implementation  
**Estimated Complexity**: Medium

## Approach

Implement the Backend Registry as a thread-safe, in-memory data store using DashMap. Follow strict TDD: write failing tests first, then implement to make them pass.

### Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Primary storage | `DashMap<String, Backend>` | Lock-free concurrent reads, sharded writes |
| Model index | `DashMap<String, Vec<String>>` | Maps model_id → backend_ids for O(1) lookup |
| Atomic counters | `AtomicU32`/`AtomicU64` | Lock-free updates for pending_requests, latency |
| Serialization | `#[derive(Serialize)]` on views | Avoid serializing atomics directly |
| EMA calculation | Integer math with α=0.2 (1/5) | `new = (sample + 4*old) / 5` avoids floats |

### File Structure

```
src/
├── main.rs                 # Entry point (unchanged initially)
├── lib.rs                  # Library root, re-exports modules
└── registry/
    ├── mod.rs              # Registry struct and operations
    ├── backend.rs          # Backend and Model structs
    ├── error.rs            # RegistryError enum
    └── tests.rs            # Unit tests (#[cfg(test)])
```

### Dependencies

All required dependencies already in `Cargo.toml`:
- `dashmap = "6"` ✓
- `chrono = { features = ["serde"] }` ✓
- `serde = { features = ["derive"] }` ✓
- `uuid = { features = ["v4", "serde"] }` ✓
- `tracing` ✓

**New dev-dependency needed**:
```toml
proptest = "1"  # For property-based testing of atomic operations
```

## Implementation Phases

### Phase 1: Data Structures (Tests First)

**Goal**: Define all types with proper derives and serialization.

**Tests to write first**:
1. `test_backend_type_serialization` - BackendType serializes to lowercase strings
2. `test_backend_status_serialization` - BackendStatus serializes correctly
3. `test_model_creation` - Model struct can be created with all fields
4. `test_backend_creation` - Backend struct can be created with all fields
5. `test_backend_json_roundtrip` - Backend serializes to JSON and back

**Implementation**:
1. Create `src/registry/mod.rs` with module declarations
2. Create `src/registry/backend.rs`:
   - `BackendType` enum with Serialize/Deserialize
   - `BackendStatus` enum with Serialize/Deserialize
   - `DiscoverySource` enum with Serialize/Deserialize
   - `Model` struct with all fields
   - `Backend` struct with atomic fields
   - `BackendView` struct for serialization (non-atomic copy)
3. Create `src/registry/error.rs`:
   - `RegistryError` enum with thiserror

**Acceptance**: All 5 tests pass.

---

### Phase 2: Core Registry Operations (Tests First)

**Goal**: Implement add, remove, get operations.

**Tests to write first**:
1. `test_add_backend_success` - Adding backend stores it
2. `test_add_backend_duplicate_error` - Adding duplicate ID returns error
3. `test_remove_backend_success` - Removing backend returns it
4. `test_remove_backend_not_found` - Removing non-existent returns error
5. `test_get_backend_found` - Getting existing backend returns Some
6. `test_get_backend_not_found` - Getting non-existent returns None
7. `test_get_all_backends` - Returns all registered backends
8. `test_backend_count` - Returns correct count

**Implementation**:
1. Add `Registry` struct to `src/registry/mod.rs`:
   ```rust
   pub struct Registry {
       backends: DashMap<String, Backend>,
       model_index: DashMap<String, Vec<String>>,
   }
   ```
2. Implement `Registry::new()`
3. Implement `add_backend()` - insert with duplicate check
4. Implement `remove_backend()` - remove and cleanup model index
5. Implement `get_backend()` - clone backend for return
6. Implement `get_all_backends()` - iterate and clone
7. Implement `backend_count()` - return len()

**Acceptance**: All 8 tests pass.

---

### Phase 3: Model Index & Queries (Tests First)

**Goal**: Implement model-to-backend index and query operations.

**Tests to write first**:
1. `test_get_backends_for_model_single` - One backend with model
2. `test_get_backends_for_model_multiple` - Multiple backends with same model
3. `test_get_backends_for_model_none` - No backends with model
4. `test_get_healthy_backends` - Filters by Healthy status
5. `test_get_healthy_backends_excludes_unhealthy` - Excludes Unhealthy
6. `test_get_healthy_backends_excludes_draining` - Excludes Draining
7. `test_model_count` - Counts unique models
8. `test_model_index_updated_on_add` - Index updated when backend added
9. `test_model_index_updated_on_remove` - Index updated when backend removed

**Implementation**:
1. Update `add_backend()` to populate model_index
2. Update `remove_backend()` to cleanup model_index
3. Implement `get_backends_for_model()` - lookup index, fetch backends
4. Implement `get_healthy_backends()` - filter by status
5. Implement `model_count()` - count index keys

**Acceptance**: All 9 tests pass, query < 1ms with 100 backends.

---

### Phase 4: Status & Model Updates (Tests First)

**Goal**: Implement status and model list updates.

**Tests to write first**:
1. `test_update_status_healthy_to_unhealthy` - Status changes
2. `test_update_status_sets_timestamp` - last_health_check updated
3. `test_update_status_sets_error` - last_error populated
4. `test_update_status_clears_error` - last_error cleared on healthy
5. `test_update_status_not_found` - Returns error for unknown ID
6. `test_update_models_replaces_list` - Model list replaced
7. `test_update_models_updates_index` - Model index reflects changes
8. `test_update_models_not_found` - Returns error for unknown ID

**Implementation**:
1. Implement `update_status()`:
   - Get mutable ref via DashMap
   - Update status, last_health_check, last_error
2. Implement `update_models()`:
   - Remove old model entries from index
   - Update backend's model list
   - Add new model entries to index

**Acceptance**: All 8 tests pass.

---

### Phase 5: Atomic Counters (Tests First)

**Goal**: Implement thread-safe counter operations with property tests.

**Tests to write first**:
1. `test_increment_pending_success` - Counter increments
2. `test_increment_pending_returns_new_value` - Returns value after increment
3. `test_decrement_pending_success` - Counter decrements
4. `test_decrement_pending_clamps_to_zero` - Never goes negative
5. `test_decrement_pending_logs_warning_at_zero` - Warning logged (check with tracing-test)
6. `test_update_latency_first_sample` - Initial value set
7. `test_update_latency_ema_calculation` - EMA formula correct
8. `test_update_latency_zero_valid` - 0ms accepted

**Property tests** (proptest):
1. `prop_increment_decrement_balanced` - n increments then n decrements = 0
2. `prop_concurrent_increments_correct` - Final count matches increment count
3. `prop_latency_bounded` - EMA stays within reasonable bounds

**Implementation**:
1. Implement `increment_pending()`:
   - `fetch_add(1, Ordering::SeqCst)`
   - Return new value
2. Implement `decrement_pending()`:
   - Use compare-exchange loop for saturating subtraction
   - Log warning via tracing if was 0
3. Implement `update_latency()`:
   - Calculate EMA: `new = (sample + 4*old) / 5`
   - Use compare-exchange for atomic update

**Acceptance**: All 8 unit tests + 3 property tests pass.

---

### Phase 6: Concurrency Stress Tests

**Goal**: Validate thread-safety under load.

**Tests to write**:
1. `test_concurrent_reads_no_deadlock` - 10,000 concurrent reads complete
2. `test_concurrent_read_write_safe` - Mixed read/write workload
3. `test_concurrent_add_remove_same_id` - No panic, last wins
4. `test_concurrent_model_queries` - Model index consistent

**Implementation**:
1. Use `tokio::spawn` to create concurrent tasks
2. Use `Arc<Registry>` for shared access
3. Verify no panics, deadlocks, or data corruption

**Acceptance**: All stress tests pass within 5 seconds.

---

### Phase 7: Integration & Cleanup

**Goal**: Wire up module, verify performance.

**Tasks**:
1. Create `src/lib.rs` with `pub mod registry;`
2. Add benchmark for `get_backends_for_model()` with 1000 backends
3. Verify memory usage < 10KB per backend
4. Run `cargo clippy` and `cargo fmt`
5. Add doc comments with examples to public items

**Acceptance**: 
- `cargo test` passes
- `cargo clippy -- -D warnings` passes
- `cargo fmt --check` passes
- Query benchmark < 1ms

## Task Summary

| Phase | Tasks | Tests | Priority |
|-------|-------|-------|----------|
| 1. Data Structures | 3 files | 5 tests | P1 |
| 2. Core Operations | 7 functions | 8 tests | P1 |
| 3. Model Queries | 5 functions | 9 tests | P1 |
| 4. Status Updates | 2 functions | 8 tests | P1 |
| 5. Atomic Counters | 3 functions | 8 + 3 prop | P1 |
| 6. Stress Tests | - | 4 tests | P2 |
| 7. Integration | cleanup | benchmarks | P2 |

**Total**: ~20 functions, ~45 tests

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| DashMap serialization | Use BackendView wrapper for non-atomic copy |
| Atomic EMA precision | Use integer math, accept ±1ms variance |
| Model index staleness | Always update index in same operation as backend |
| Memory overhead | Benchmark with 100 backends, verify < 1MB total |

## Definition of Done

- [x] All 45+ tests pass
- [x] Property tests for counters pass
- [x] Stress tests complete without deadlock
- [x] `cargo clippy -- -D warnings` passes
- [x] `cargo fmt --check` passes
- [x] Query by model < 1ms with 1000 backends
- [x] Memory < 10KB per backend
- [x] Doc comments on all public items
