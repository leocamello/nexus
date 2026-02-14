# Implementation Plan: Structured Request Logging

**Branch**: `011-structured-logging` | **Date**: 2025-02-14 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/011-structured-logging/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Implement comprehensive structured logging for every request passing through Nexus, capturing essential metadata (timestamp, request_id, model, backend, status, latency, tokens), routing decisions, and retry/failover chains. Uses the existing `tracing` infrastructure with JSON formatting support, ensuring all logs are queryable and compatible with standard log aggregators while maintaining privacy by default (no message content logged).

## Technical Context

**Language/Version**: Rust 1.75 (stable)  
**Primary Dependencies**: axum 0.7, tokio 1.x (full features), tracing 0.1, tracing-subscriber 0.3 (with json feature)  
**Storage**: N/A (in-memory only, stateless by design)  
**Testing**: cargo test with integration tests for log output verification  
**Target Platform**: Linux/macOS/Windows server (single binary)
**Project Type**: Single project (Rust library + binary)  
**Performance Goals**: < 1ms logging overhead per request, non-blocking log writes  
**Constraints**: No blocking on logging failures, privacy-safe by default (no message content), OpenAI API compatibility maintained  
**Scale/Scope**: 10,000+ requests per minute logging throughput

**Existing Architecture**:
- Logging: `tracing` crate with `tracing-subscriber` already integrated (src/main.rs)
- LoggingConfig: Exists in `src/config/logging.rs` with `level: String` and `format: LogFormat` (Pretty/Json)
- Request handling: `src/api/completions.rs` handles chat completions with axum
- Routing: `src/routing/mod.rs` contains Router with select_backend() that returns RoutingResult
- Metrics: `src/metrics/` already tracks request_id, model, backend, latency, tokens using the `metrics` crate
- Request ID: Already used in metrics but needs to be generated and propagated through tracing spans
- UUID: Already in Cargo.toml (v1 with v4 feature) for correlation IDs

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Simplicity Gate
- [x] Using ≤3 main modules for initial implementation? **YES** - Extends existing logging module + adds tracing layer in completions handler
- [x] No speculative "might need" features? **YES** - Only implements required FR-001 through FR-015 from spec
- [x] No premature optimization? **YES** - Uses existing tracing infrastructure, adds structured fields incrementally
- [x] Start with simplest approach that could work? **YES** - Leverage existing tracing spans, add fields via span::record()

### Anti-Abstraction Gate
- [x] Using Axum/Tokio/reqwest directly (no wrapper layers)? **YES** - Direct tracing macros, no logging abstraction layer
- [x] Single representation for each data type? **YES** - RequestLogEntry as span fields, not separate struct hierarchy
- [x] No "framework on top of framework" patterns? **YES** - Uses tracing directly, extends LoggingConfig minimally
- [x] Abstractions justified by actual (not theoretical) needs? **YES** - No new abstractions, extends existing patterns

### Integration-First Gate
- [x] API contracts defined before implementation? **YES** - Log entry schema defined in data-model.md (Phase 1)
- [x] Integration tests planned with real/mock backends? **YES** - Test log output format and field presence
- [x] End-to-end flow testable? **YES** - Send request, verify structured log entry emitted with all required fields

### Performance Gate
- [x] Routing decision target: < 1ms? **YES** - Logging adds fields to existing spans, no additional routing overhead
- [x] Total overhead target: < 5ms? **YES** - Target < 1ms logging overhead, non-blocking writes
- [x] Memory baseline target: < 50MB? **YES** - No additional state, tracing subscriber buffers are bounded

**Status**: ✅ All gates pass. No complexity justification needed.

## Project Structure

### Documentation (this feature)

```text
specs/011-structured-logging/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
│   └── log-schema.json  # JSON schema for log entries
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
src/
├── config/
│   └── logging.rs           # EXTEND: Add enable_content_logging, component_levels fields
├── api/
│   └── completions.rs       # MODIFY: Add request_id generation, span instrumentation
├── routing/
│   └── mod.rs              # MODIFY: Add route_reason to RoutingResult
├── logging/                 # NEW MODULE: Structured logging utilities
│   ├── mod.rs              # Re-exports and initialization
│   ├── middleware.rs       # Request ID middleware for axum
│   └── fields.rs           # Field extraction helpers (tokens, latency, status)
└── main.rs                 # MODIFY: Configure JSON layer for tracing-subscriber

tests/
├── integration/
│   └── logging_test.rs     # NEW: Verify log output format and field presence
└── unit/
    └── logging/
        ├── fields_test.rs  # NEW: Test field extraction logic
        └── middleware_test.rs  # NEW: Test request ID propagation
```

**Structure Decision**: Single project structure maintained. Adds new `src/logging/` module for structured logging utilities while extending existing modules minimally. All changes integrate with existing tracing infrastructure per Constitution principle VIII (Stateless by Design) and X (Precise Measurement).

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

**No violations**. All Constitution gates passed both pre-design and post-design evaluation.

---

## Post-Design Constitution Re-Check

*Re-evaluated after Phase 1 design artifacts generated.*

### Simplicity Gate
- [x] Using ≤3 main modules? **YES** - Only extends existing modules (config/logging.rs, api/completions.rs, routing/mod.rs) + adds src/logging/ module (1 new module)
- [x] No speculative features? **YES** - All features mapped directly to functional requirements (FR-001 to FR-015)
- [x] No premature optimization? **YES** - Uses existing tracing infrastructure, no custom async runtime or buffer management
- [x] Simplest approach? **YES** - Tracing span fields instead of separate logging structs

### Anti-Abstraction Gate
- [x] Direct framework use? **YES** - Uses tracing macros directly, no wrapper abstraction layer
- [x] Single representation? **YES** - RequestLogEntry is conceptual only (span fields), not a separate struct hierarchy
- [x] No framework-on-framework? **YES** - Extends existing tracing setup, no new logging framework
- [x] Justified abstractions? **YES** - Helper functions in src/logging/fields.rs only for field extraction (token counting), not general abstraction

### Integration-First Gate
- [x] Contracts defined? **YES** - contracts/log-schema.json provides JSON Schema for log entries
- [x] Integration tests planned? **YES** - tests/integration/logging_test.rs will verify JSON output format and field presence
- [x] End-to-end testable? **YES** - Send request → verify structured log entry with all required fields

### Performance Gate
- [x] Routing decision < 1ms? **YES** - No additional routing overhead (logging happens after routing)
- [x] Total overhead < 5ms? **YES** - Estimated ~187µs per request (research.md performance budget)
- [x] Memory baseline < 50MB? **YES** - No additional state, tracing buffers are bounded by subscriber config

**Status**: ✅ All gates still pass post-design. Implementation proceeds to Phase 2 (tasks generation via /speckit.tasks command).

---

## Phase 0 & 1 Outputs

**Phase 0 - Research** (COMPLETED):
- ✅ `research.md` - Comprehensive research on tracing patterns, JSON layer config, non-blocking logging, component filtering
- All NEEDS CLARIFICATION items resolved
- Performance budget validated: <1ms overhead target met (~187µs estimated)

**Phase 1 - Design** (COMPLETED):
- ✅ `data-model.md` - RequestLogEntry field definitions, LoggingConfig extensions, RoutingResult extensions, validation rules
- ✅ `contracts/log-schema.json` - JSON Schema for log entries with examples
- ✅ `quickstart.md` - Configuration guide, query patterns, log aggregator integration, troubleshooting
- ✅ Agent context updated (GitHub Copilot instructions)

**Next Phase**: Phase 2 - Task generation (run `/speckit.tasks` command separately)
