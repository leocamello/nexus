# Research: Web Dashboard Technology Decisions

**Feature**: F10 Web Dashboard  
**Branch**: `010-web-dashboard`  
**Date**: 2024-02-14

## Overview

This document captures research and technology decisions for implementing the embedded web dashboard. All decisions align with Nexus constitutional principles: single binary, no external dependencies, stateless design.

---

## Decision 1: Static Asset Embedding Strategy

**Decision**: Use `rust-embed` crate with compile-time embedding

**Rationale**:
- Constitutional requirement: "All assets (dashboard, config templates) embedded in binary"
- rust-embed is the standard Rust solution for embedding files at compile time
- Zero-cost abstraction: assets compiled directly into binary using `include_bytes!` macro
- Supports MIME type detection and compression (gzip)
- Well-maintained, 4.5k+ stars on GitHub
- Used successfully in similar projects (see axum examples)

**Alternatives Considered**:
1. **include_str!/include_bytes! macros directly**
   - Rejected: Would require manual MIME type handling and path routing
   - More boilerplate code for something rust-embed solves elegantly
2. **Runtime file serving from disk**
   - Rejected: Violates "single binary" constitutional requirement
   - Would require dashboard files to be distributed separately
3. **Build script that generates Rust code**
   - Rejected: More complex than rust-embed, reinventing the wheel

**Implementation Notes**:
```rust
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "dashboard/"]
struct DashboardAssets;
```

---

## Decision 2: CSS Framework Approach

**Decision**: Use precompiled Tailwind CSS (standalone CLI build)

**Rationale**:
- Tailwind provides utility classes for rapid responsive design
- Precompiled approach means zero runtime dependencies
- Generated `styles.css` is embedded as a static file
- Mobile responsiveness and dark mode come "free" with Tailwind utilities
- Binary size impact: ~30-50KB for a minimal Tailwind build (within 200KB budget)

**Alternatives Considered**:
1. **No CSS framework (hand-written CSS)**
   - Rejected: Would require manual media queries for mobile, dark mode
   - More code to maintain for responsive grid layouts
2. **Bootstrap or other framework**
   - Rejected: Heavier than Tailwind, less modern utility approach
3. **Runtime Tailwind (CDN)**
   - Rejected: Violates "no external dependencies" principle
   - Would fail in offline environments

**Implementation Notes**:
- Use Tailwind standalone CLI: `npx tailwindcss -i input.css -o dashboard/styles.css --minify`
- Run once during development, commit the generated CSS
- No Node.js runtime dependency for users
- Use Tailwind's JIT mode for minimal output size

---

## Decision 3: WebSocket Implementation

**Decision**: Use axum's built-in WebSocket support (`axum::extract::ws`)

**Rationale**:
- Already part of the Axum dependency tree (no new dependencies)
- Integrates seamlessly with existing async Axum handlers
- Provides `Message` enum for text/binary/ping/pong/close
- Handles protocol upgrade from HTTP automatically
- Constitutional principle: "Use Axum/Tokio directly (no wrapper layers)"

**Alternatives Considered**:
1. **tokio-tungstenite directly**
   - Rejected: axum::extract::ws is a thin wrapper over tungstenite
   - No benefit to bypassing Axum's integration
2. **Server-Sent Events (SSE)**
   - Rejected: One-way only (server → client), no client → server channel
   - WebSocket spec requirement in feature spec
3. **Long polling only**
   - Rejected: Higher latency, more resource intensive
   - WebSocket is the primary approach with polling as fallback

**Implementation Notes**:
```rust
use axum::extract::ws::{WebSocket, WebSocketUpgrade};

async fn websocket_handler(ws: WebSocketUpgrade, state: State<Arc<AppState>>) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}
```

---

## Decision 4: Request History Ring Buffer

**Decision**: Use `std::collections::VecDeque` with fixed capacity of 100

**Rationale**:
- VecDeque is the standard Rust ring buffer implementation
- Efficient O(1) push/pop at both ends
- No external dependencies (std library)
- Simple capacity management: when full, pop_front() before push_back()
- Thread-safe when wrapped in Arc<RwLock<VecDeque>>

**Alternatives Considered**:
1. **bounded-vec-deque crate**
   - Rejected: Unnecessary dependency for simple fixed-size buffer
   - VecDeque + manual capacity check is trivial
2. **ringbuf crate**
   - Rejected: More complex API, designed for lock-free scenarios
   - Our use case (occasional writes with dashboard reads) doesn't need lock-free
3. **Vec with circular indexing**
   - Rejected: More error-prone, VecDeque is designed for this pattern

**Implementation Notes**:
```rust
use std::collections::VecDeque;
use tokio::sync::RwLock;

const HISTORY_CAPACITY: usize = 100;

pub struct RequestHistory {
    entries: RwLock<VecDeque<HistoryEntry>>,
}

impl RequestHistory {
    pub async fn push(&self, entry: HistoryEntry) {
        let mut entries = self.entries.write().await;
        if entries.len() >= HISTORY_CAPACITY {
            entries.pop_front();
        }
        entries.push_back(entry);
    }
}
```

---

## Decision 5: WebSocket Update Broadcasting

**Decision**: Use tokio broadcast channel with bounded capacity

**Rationale**:
- tokio::sync::broadcast is designed for 1:N message distribution
- Each WebSocket subscriber gets a receiver clone
- Bounded capacity (e.g., 1000) prevents memory growth
- Slow consumers automatically lag (don't block fast consumers)
- Already using Tokio, no new dependencies

**Alternatives Considered**:
1. **Manual fan-out with Vec<WebSocket>**
   - Rejected: Complex error handling when connections close
   - Would need to track and clean up dead connections
2. **mpsc + clone pattern**
   - Rejected: mpsc is 1:1, would need multiple clones
   - broadcast is purpose-built for this use case
3. **External message broker (Redis, NATS)**
   - Rejected: Violates "no external dependencies" principle

**Implementation Notes**:
```rust
use tokio::sync::broadcast;

const BROADCAST_CAPACITY: usize = 1000;

// In AppState
pub struct AppState {
    // ... existing fields
    pub ws_broadcast: broadcast::Sender<WebSocketUpdate>,
}

// Subscribe in WebSocket handler
let mut rx = state.ws_broadcast.subscribe();
```

---

## Decision 6: Fallback Polling Mechanism

**Decision**: Client-side polling with exponential backoff on WebSocket failure

**Rationale**:
- Feature spec requirement: "Fallback to polling every 5 seconds when WebSocket unavailable"
- Polling implemented in JavaScript (no server changes needed)
- Uses existing `/v1/stats` and `/v1/models` endpoints
- Client detects WebSocket failure and automatically switches to polling
- Exponential backoff on repeated failures: 5s → 10s → 30s → 60s (cap at 60s)

**Alternatives Considered**:
1. **Server-side polling endpoint**
   - Rejected: Existing `/v1/stats` already provides all needed data
   - No new endpoint needed
2. **Always poll (no WebSocket)**
   - Rejected: Feature spec explicitly requires WebSocket with fallback
   - Polling is less efficient for real-time updates
3. **No fallback (WebSocket required)**
   - Rejected: Feature spec requires graceful degradation

**Implementation Notes** (JavaScript):
```javascript
let ws;
let pollingInterval;
let isPolling = false;

function connectWebSocket() {
    try {
        ws = new WebSocket('ws://' + location.host + '/ws');
        ws.onopen = () => { stopPolling(); };
        ws.onerror = () => { startPolling(); };
        ws.onclose = () => { startPolling(); };
    } catch (e) {
        startPolling();
    }
}

function startPolling() {
    if (isPolling) return;
    isPolling = true;
    pollingInterval = setInterval(fetchStats, 5000);
}
```

---

## Decision 7: JavaScript-Disabled Fallback

**Decision**: Server-side HTML rendering with `<noscript>` and meta refresh

**Rationale**:
- Feature spec requirement: "Function with JavaScript disabled"
- Render complete HTML with data on initial page load
- Use `<meta http-equiv="refresh" content="5">` for auto-refresh
- Display "Refresh" button in `<noscript>` section
- JavaScript progressively enhances with real-time updates

**Alternatives Considered**:
1. **Separate no-JS page**
   - Rejected: Duplicates content, harder to maintain
   - Progressive enhancement is better UX
2. **API-only, no fallback**
   - Rejected: Feature spec explicitly requires no-JS support
3. **Server-side rendering framework (Tera, Askama)**
   - Rejected: Adds template engine dependency
   - Simple HTML with inline data is sufficient

**Implementation Notes**:
```html
<!-- In index.html -->
<noscript>
  <meta http-equiv="refresh" content="5">
  <div class="alert">JavaScript is disabled. Page refreshes every 5 seconds. 
    <a href="/">Refresh now</a>
  </div>
</noscript>

<script id="initial-data" type="application/json">
  {{{stats_json}}}
</script>
```

---

## Decision 8: Dark Mode Support

**Decision**: CSS media query `@media (prefers-color-scheme: dark)`

**Rationale**:
- Feature spec requirement: "Support dark mode using CSS prefers-color-scheme"
- No JavaScript required, purely CSS-based
- Respects system preference automatically
- Tailwind has built-in `dark:` variant for utilities
- Zero overhead, no user preference storage needed (stateless requirement)

**Alternatives Considered**:
1. **Toggle button with localStorage**
   - Rejected: Requires persistent state (violates stateless principle)
   - Feature spec says "using CSS prefers-color-scheme", not user toggle
2. **Separate dark.css file**
   - Rejected: Increases bundle size, harder to maintain
   - Media query approach is standard

**Implementation Notes**:
```css
/* Tailwind approach */
<div class="bg-white dark:bg-gray-900 text-black dark:text-white">
```

---

## Decision 9: Model Capabilities Display

**Decision**: Use icon indicators with CSS badges for vision/tools/JSON mode

**Rationale**:
- Visual indicators are more scannable than text
- Tailwind badge components fit mobile screens
- Model struct already has boolean flags (supports_vision, supports_tools, supports_json_mode)
- Context length displayed as "32k" format (human-readable)

**Alternatives Considered**:
1. **Text labels only**
   - Rejected: Takes more space, less visual
2. **Unicode emoji**
   - Rejected: Inconsistent rendering across platforms
3. **SVG icons**
   - Rejected: Increases bundle size unnecessarily

**Implementation Notes**:
```html
<div class="flex gap-1">
  <span class="badge" title="Vision">👁️</span>
  <span class="badge" title="Tools">🔧</span>
  <span class="badge" title="JSON">{ }</span>
  <span class="text-sm">32k</span>
</div>
```

---

## Decision 10: Testing Strategy

**Decision**: Three-tier testing: contract → integration → unit

**Rationale**:
- Constitutional requirement: "Test-First Development (NON-NEGOTIABLE)"
- Contract tests: WebSocket message format validation
- Integration tests: HTTP endpoint responses, WebSocket lifecycle
- Unit tests: Ring buffer logic, history entry creation
- No E2E browser tests (out of scope for MVP)

**Test Execution Order** (per TDD requirement):
1. Write contract tests for WebSocket messages → MUST FAIL (red)
2. Write integration tests for endpoints → MUST FAIL (red)
3. Write unit tests for ring buffer → MUST FAIL (red)
4. Implement dashboard handlers → tests PASS (green)
5. Refactor → tests remain PASS (refactor)

**Implementation Notes**:
```rust
// tests/contract/dashboard_websocket_test.rs
#[test]
fn websocket_update_must_deserialize() {
    let json = r#"{"type":"backend_status","data":{...}}"#;
    let update: WebSocketUpdate = serde_json::from_str(json).expect("valid JSON");
    assert_eq!(update.update_type, "backend_status");
}
```

---

## Summary

All technology decisions align with Nexus constitutional principles:
- ✅ Single binary: rust-embed compiles assets into executable
- ✅ No runtime dependencies: precompiled Tailwind, vanilla JS, std library VecDeque
- ✅ Stateless: no localStorage, no sessions, no user preferences
- ✅ Use Axum/Tokio directly: axum::extract::ws, tokio::sync::broadcast
- ✅ Graceful degradation: works without JavaScript, automatic WebSocket → polling fallback

No new external crates beyond rust-embed (compile-time only). All runtime dependencies are already in use (axum, tokio, serde).
