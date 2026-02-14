# Implementation Plan: Web Dashboard

**Branch**: `010-web-dashboard` | **Date**: 2024-02-14 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/010-web-dashboard/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Implement an embedded web dashboard served at `/` that displays real-time backend health, model availability matrix, and recent request history. The dashboard must be embedded in the Nexus binary using rust-embed with no external dependencies. It will use vanilla HTML/CSS/JS with precompiled Tailwind CSS for styling and axum WebSocket support for real-time updates, with automatic fallback to HTTP polling. All data is consumed from existing `/v1/stats` and `/v1/models` endpoints.

## Technical Context

**Language/Version**: Rust 1.75+ (stable toolchain)  
**Primary Dependencies**: 
- axum (HTTP framework with WebSocket support)
- tokio (async runtime)
- rust-embed (static file embedding at compile time)
- dashmap (concurrent Registry access)
- serde_json (JSON serialization)
- reqwest (HTTP client, already in use)

**Storage**: In-memory only
- DashMap-based Registry (already exists)
- VecDeque ring buffer for 100-request history (new)
- All state in AppState (no persistence)

**Testing**: cargo test (unit, integration, contract tests)  
**Target Platform**: Linux, macOS, Windows (cross-platform via Rust)

**Project Type**: Single binary web application (embedded dashboard)

**Performance Goals**: 
- Dashboard page load: < 2 seconds on 10 Mbps connection
- WebSocket update latency: < 5 seconds from state change
- Routing overhead: < 5ms (existing constraint maintained)
- Dashboard asset size: < 200KB embedded

**Constraints**: 
- Single binary with embedded assets (no external files)
- Stateless (no session storage, no user preferences)
- No external dependencies at runtime
- Graceful degradation (works without JavaScript)
- Mobile-responsive (down to 320px width)
- Dark mode support via CSS media query

**Scale/Scope**: 
- Support 50+ concurrent WebSocket viewers
- Display up to 100 recent requests
- Handle 20+ backends in dashboard UI
- Minimal binary size increase (< 200KB)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Simplicity Gate
- [x] Using ≤3 main modules for initial implementation?
  - **YES**: `dashboard` module (handlers, WebSocket, types), static assets in `dashboard/` directory, ring buffer integrated into metrics
- [x] No speculative "might need" features?
  - **YES**: Only implementing required features from spec (backend status, model matrix, request history, WebSocket updates)
- [x] No premature optimization?
  - **YES**: Starting with simple ring buffer (VecDeque), basic WebSocket broadcast
- [x] Start with simplest approach that could work?
  - **YES**: Consume existing `/v1/stats` and `/v1/models` endpoints, simple HTML template with vanilla JS

### Anti-Abstraction Gate
- [x] Using Axum/Tokio/reqwest directly (no wrapper layers)?
  - **YES**: Direct use of axum::extract::ws::WebSocket, no custom frameworks
- [x] Single representation for each data type?
  - **YES**: Reusing existing types (BackendView, Model, StatsResponse) plus new WebSocketUpdate type
- [x] No "framework on top of framework" patterns?
  - **YES**: Vanilla HTML/CSS/JS, no React/Vue/Angular
- [x] Abstractions justified by actual (not theoretical) needs?
  - **YES**: rust-embed required for binary embedding (constitutional requirement)

### Integration-First Gate
- [x] API contracts defined before implementation?
  - **YES**: Will define WebSocket message format and HTML structure before coding
- [x] Integration tests planned with real/mock backends?
  - **YES**: Test WebSocket updates with mock state changes, test fallback to polling
- [x] End-to-end flow testable?
  - **YES**: Can test full flow: serve HTML → load via browser → WebSocket connect → receive updates

### Performance Gate
- [x] Routing decision target: < 1ms?
  - **YES**: Dashboard doesn't affect routing (read-only observability)
- [x] Total overhead target: < 5ms?
  - **YES**: Dashboard is separate endpoint, doesn't add overhead to `/v1/chat/completions`
- [x] Memory baseline target: < 50MB?
  - **YES**: Ring buffer (100 entries × ~200 bytes = ~20KB), WebSocket connections (~10KB each for 50 users = ~500KB), embedded assets (~150KB)

**All gates PASSED** ✓

## Project Structure

### Documentation (this feature)

```text
specs/010-web-dashboard/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (technology decisions)
├── data-model.md        # Phase 1 output (entities and state)
├── quickstart.md        # Phase 1 output (developer guide)
├── contracts/           # Phase 1 output (WebSocket protocol)
│   └── websocket.yaml   # WebSocket message format (OpenAPI-style)
└── tasks.md             # Phase 2 output (/speckit.tasks - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
# Single Rust project structure
src/
├── api/
│   └── mod.rs           # Add dashboard routes: GET /, GET /assets/*, GET /ws
├── dashboard/           # NEW module
│   ├── mod.rs           # Re-exports and module structure
│   ├── handler.rs       # Axum handlers for GET / and GET /assets/*
│   ├── websocket.rs     # WebSocket connection handler and broadcast logic
│   ├── history.rs       # Request history ring buffer (VecDeque)
│   └── types.rs         # WebSocketUpdate, HistoryEntry types
├── metrics/
│   └── handler.rs       # Existing /v1/stats handler (no changes)
└── lib.rs               # Register dashboard module

dashboard/               # NEW directory (static assets)
├── index.html           # Main dashboard HTML (with no-JS fallback)
├── dashboard.js         # WebSocket client and DOM updates
├── styles.css           # Precompiled Tailwind CSS
└── favicon.ico          # Optional icon

tests/
├── contract/
│   └── dashboard_websocket_test.rs  # WebSocket message contract tests
├── integration/
│   └── dashboard_test.rs            # Dashboard endpoints integration tests
└── unit/
    └── dashboard_history_test.rs    # Ring buffer unit tests
```

**Structure Decision**: Using single Rust project structure (Option 1 from template) because Nexus is a monolithic binary. The dashboard module is a new top-level module alongside `api`, `metrics`, `registry`, etc. Static assets are embedded at compile time from the `dashboard/` directory using rust-embed. No separate frontend build process needed since we're using vanilla HTML/CSS/JS with precompiled Tailwind.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

No violations. All constitution gates passed without exceptions.
