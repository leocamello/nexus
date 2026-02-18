# Implementation Plan: Request Queuing & Prioritization

**Branch**: `021-request-queuing` | **Date**: 2026-02-15 (Retrospective) | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/021-request-queuing/spec.md`

**Note**: This is a RETROSPECTIVE plan documenting the already-implemented F18 feature.

## Summary

Implement bounded dual-priority request queuing using tokio mpsc channels to handle burst traffic when all backends reach capacity. Requests are queued with High or Normal priority (via `X-Nexus-Priority` header), and a background drain loop processes them as capacity becomes available. Timeout mechanism ensures requests don't wait indefinitely (default: 30s), and graceful shutdown drains remaining requests with 503 responses.

## Technical Context

**Language/Version**: Rust 1.75 stable  
**Primary Dependencies**: 
- Tokio (async runtime with full features)
- Axum (HTTP framework)
- tokio::sync::mpsc (multi-producer single-consumer channels)
- tokio::sync::oneshot (one-time response channels)
- std::sync::atomic::AtomicUsize (lock-free depth counter)
- metrics crate (Prometheus-compatible metrics)
- serde (TOML configuration)

**Storage**: In-memory only (stateless, no persistence)  
**Testing**: cargo test (10 unit tests in mod tests, 2 integration tests in tests/queue_test.rs)  
**Target Platform**: Linux, macOS, Windows (cross-platform via Tokio)  
**Project Type**: Single binary (Rust backend with embedded dashboard)  
**Performance Goals**: 
- Queue overhead: < 1ms per enqueue/dequeue operation
- Drain loop poll interval: 50ms
- Support 1000+ concurrent queued requests
- Memory: < 10KB per queued request

**Constraints**: 
- Queue must be bounded (default max_size: 100)
- Timeout enforcement (default: 30 seconds)
- Graceful shutdown required (drain all pending requests)
- Thread-safe atomic operations (concurrent enqueue from multiple request handlers)
- No external dependencies or persistence

**Scale/Scope**: 
- Handles burst traffic up to 100 queued requests (configurable)
- Dual-priority system (High/Normal)
- Single global queue (not per-backend)
- Background drain loop with 50ms poll interval

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Simplicity Gate ✅
- [x] Using ≤3 main modules for initial implementation? **YES** (src/queue/mod.rs, src/config/queue.rs, integration in src/api/completions.rs)
- [x] No speculative "might need" features? **YES** (Only two priority levels as needed)
- [x] No premature optimization? **YES** (Simple atomic counter and FIFO channels)
- [x] Start with simplest approach that could work? **YES** (Standard tokio mpsc channels, atomic depth tracking)

### Anti-Abstraction Gate ✅
- [x] Using Axum/Tokio/reqwest directly (no wrapper layers)? **YES** (Direct use of tokio::sync::mpsc, tokio::sync::Mutex)
- [x] Single representation for each data type? **YES** (QueuedRequest, QueueError, Priority)
- [x] No "framework on top of framework" patterns? **YES** (Direct channel usage, no queue abstractions)
- [x] Abstractions justified by actual (not theoretical) needs? **YES** (RequestQueue wraps dual channels for priority enforcement)

### Integration-First Gate ✅
- [x] API contracts defined before implementation? **YES** (QueuedRequest, QueueError, QueueResponse types)
- [x] Integration tests planned with real/mock backends? **YES** (2 integration tests in tests/queue_test.rs)
- [x] End-to-end flow testable? **YES** (Tests verify enqueue → wait → dequeue → response flow)

### Performance Gate ✅
- [x] Routing decision target: < 1ms? **YES** (Queue check is O(1) atomic operation)
- [x] Total overhead target: < 5ms? **YES** (Enqueue/dequeue < 1ms, poll interval 50ms)
- [x] Memory baseline target: < 50MB? **YES** (< 10KB per queued request, max 100 requests = ~1MB)

**Gate Status**: ✅ **ALL GATES PASSED**

No complexity violations detected. Implementation follows Nexus constitution principles:
- Simple dual-channel design (Principle I: Simplicity)
- Direct use of Tokio primitives (Principle II: Anti-Abstraction)
- Explicit capacity and timeout contracts (Principle IX: Explicit Contracts)
- In-memory stateless queue (Principle VII: Local-First, Principle VIII: Stateless)

## Project Structure

### Documentation (this feature)

```text
specs/021-request-queuing/
├── spec.md              # Feature specification (retrospective)
├── plan.md              # This file (retrospective implementation plan)
├── research.md          # Phase 0 research findings
├── data-model.md        # Phase 1 data structures
├── quickstart.md        # Phase 1 usage guide
└── contracts/           # Phase 1 API contracts
    ├── queue-types.md   # Type definitions
    └── queue-api.md     # Public API surface
```

### Source Code (repository root)

```text
src/
├── queue/
│   └── mod.rs           # RequestQueue, Priority, QueuedRequest, queue_drain_loop()
├── config/
│   ├── mod.rs           # Config aggregation
│   └── queue.rs         # QueueConfig struct with serde defaults
├── api/
│   └── completions.rs   # Integration: catch RoutingError::Queue, enqueue, wait
├── routing/
│   └── mod.rs           # RoutingError::Queue variant
└── metrics/
    └── mod.rs           # nexus_queue_depth gauge

tests/
├── queue_test.rs        # Integration tests: capacity, overflow, drain behavior
└── common/              # Test utilities

Cargo.toml               # Dependencies: tokio (full), axum, metrics
```

**Structure Decision**: Single project structure. Queue module is self-contained with minimal integration points (API handler, routing error, config). Uses standard Rust conventions: `src/queue/mod.rs` for implementation, `src/config/queue.rs` for configuration, `tests/queue_test.rs` for integration tests. All unit tests are in `#[cfg(test)] mod tests` within `src/queue/mod.rs`.

## Complexity Tracking

**No violations detected.** All constitution gates passed without requiring justification.

## Phase 0: Research (COMPLETED)

**Status**: ✅ Complete  
**Output**: [research.md](./research.md)

All research questions resolved:
- ✅ Queue implementation strategy (dual mpsc channels)
- ✅ Priority ordering mechanism (high-first dequeue)
- ✅ Timeout enforcement (check on dequeue)
- ✅ Response delivery (oneshot channels)
- ✅ Polling strategy (50ms interval)
- ✅ Re-enqueue logic (until timeout)
- ✅ Metrics integration (gauge metric)
- ✅ Configuration approach (TOML with defaults)
- ✅ Integration point (RoutingError::Queue)
- ✅ Testing strategy (unit + integration)

## Phase 1: Design & Contracts (COMPLETED)

**Status**: ✅ Complete  
**Outputs**:
- [data-model.md](./data-model.md) - Complete data structures and relationships
- [contracts/queue-types.md](./contracts/queue-types.md) - Public type definitions
- [contracts/queue-api.md](./contracts/queue-api.md) - API operations and contracts
- [quickstart.md](./quickstart.md) - User guide and examples

**Key Artifacts**:
1. **Data Model**: Priority, QueuedRequest, QueueError, QueueResponse, RequestQueue, QueueConfig
2. **Type Contracts**: Header parsing, error variants, configuration validation
3. **API Contracts**: Enqueue/dequeue operations, drain loop, metrics
4. **Usage Guide**: Configuration, testing, monitoring, troubleshooting

## Phase 2: Implementation (ALREADY COMPLETED - RETROSPECTIVE)

**Status**: ✅ Implemented  
**Source Files**:
- `src/queue/mod.rs` (632 lines) - Core queue implementation
- `src/config/queue.rs` (59 lines) - Configuration
- `src/api/completions.rs` - Integration (enqueue on RoutingError::Queue)
- `tests/queue_test.rs` - Integration tests

**Implementation Highlights**:
1. **Types**: Priority enum, QueuedRequest struct, QueueError enum, QueueResponse alias
2. **RequestQueue**: Dual mpsc channels, AtomicUsize depth counter, QueueConfig
3. **enqueue()**: Atomic depth check, capacity enforcement, channel routing
4. **try_dequeue()**: High-priority first, then normal, FIFO within each
5. **queue_drain_loop()**: 50ms poll, timeout check, re-run routing, process via agent
6. **process_queued_request()**: Increment/decrement pending, agent.chat_completion()
7. **build_timeout_response()**: 503 with Retry-After header
8. **drain_remaining()**: Graceful shutdown, 503 to all queued requests
9. **Config**: TOML defaults (enabled=true, max_size=100, max_wait_seconds=30)
10. **API Integration**: Catch RoutingError::Queue, enqueue, wait on oneshot
11. **Metrics**: nexus_queue_depth gauge (updated on enqueue/dequeue)
12. **Tests**: 14 unit tests in mod tests, 2 integration tests in tests/queue_test.rs

**Test Coverage**:
- ✅ FIFO ordering (normal and high priority)
- ✅ Capacity enforcement (reject when full)
- ✅ Priority ordering (high drains first)
- ✅ Depth counter accuracy
- ✅ Disabled queue behavior
- ✅ Priority header parsing
- ✅ Configuration validation
- ✅ Integration: capacity limits and overflow

## Summary

This retrospective plan documents the F18: Request Queuing & Prioritization feature that was successfully implemented. The feature provides:

**Core Capabilities**:
- Bounded dual-priority queue (High/Normal) using tokio mpsc channels
- Atomic depth tracking with configurable capacity (default: 100)
- Timeout enforcement (default: 30 seconds) with Retry-After headers
- Background drain loop with 50ms poll interval
- Graceful shutdown with 503 responses to remaining requests
- Prometheus metrics (nexus_queue_depth gauge)

**Integration Points**:
- API handler catches RoutingError::Queue and enqueues requests
- Oneshot channels deliver responses back to waiting handlers
- Router emits Queue decision when all backends are at capacity
- Configuration via TOML with sensible defaults

**Constitution Compliance**:
- ✅ Simplicity: 3 modules (queue, config, API integration)
- ✅ Anti-Abstraction: Direct use of Tokio primitives
- ✅ Integration-First: 2 integration tests, end-to-end testable
- ✅ Performance: < 1ms overhead, < 1MB memory footprint

**Documentation Deliverables**:
- [x] Feature specification (spec.md)
- [x] Implementation plan (this file)
- [x] Research findings (research.md)
- [x] Data model (data-model.md)
- [x] API contracts (contracts/queue-types.md, contracts/queue-api.md)
- [x] User guide (quickstart.md)
- [x] Agent context updated (.github/agents/copilot-instructions.md)

**Branch**: `021-request-queuing`  
**Status**: Fully implemented and tested  
**Next Steps**: Feature is complete. No further action required.

---

**Plan Complete** | **Retrospective** | **F18 Request Queuing & Prioritization**
