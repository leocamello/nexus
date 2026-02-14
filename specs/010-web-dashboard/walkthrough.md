# F10: Web Dashboard — Code Walkthrough

**Feature**: F10 - Web Dashboard  
**Audience**: Junior developers joining the project  
**Last Updated**: 2025-07-18

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: types.rs — Data Types](#file-1-typesrs--data-types)
4. [File 2: history.rs — Ring Buffer](#file-2-historyrs--ring-buffer)
5. [File 3: handler.rs — HTTP Handlers](#file-3-handlerrs--http-handlers)
6. [File 4: websocket.rs — Real-Time Updates](#file-4-websocketrs--real-time-updates)
7. [File 5: mod.rs — Module Exports](#file-5-modrs--module-exports)
8. [File 6: index.html — HTML Structure](#file-6-indexhtml--html-structure)
9. [File 7: styles.css — Styling and Responsiveness](#file-7-stylescss--styling-and-responsiveness)
10. [File 8: dashboard.js — Frontend Logic](#file-8-dashboardjs--frontend-logic)
11. [File 9: api/mod.rs — Wiring It All Together](#file-9-apimodrs--wiring-it-all-together)
12. [File 10: api/completions.rs — Request Recording Hook](#file-10-apicompletionsrs--request-recording-hook)
13. [File 11: health/mod.rs — Backend Status Broadcasting](#file-11-healthmodrs--backend-status-broadcasting)
14. [File 12: cli/serve.rs — Broadcast Channel Wiring](#file-12-cliservers--broadcast-channel-wiring)
15. [How Data Flows](#how-data-flows)
16. [Key Tests Explained](#key-tests-explained)
17. [Key Rust Concepts](#key-rust-concepts)
18. [Common Questions](#common-questions)

---

## The Big Picture

Think of the Web Dashboard as the **cockpit window** for Nexus. Without it, the engine still routes requests — but you can't see what's happening without `curl`-ing endpoints or reading logs. The dashboard gives operators a live, visual overview of their entire LLM fleet: which backends are healthy, which models are available, and what requests are flowing through the system.

### What Problem Does This Solve?

When Nexus is running with multiple backends and models, operators need to quickly answer:

- "Are all my backends healthy right now?"
- "Which models are available on which backends?"
- "Did that request just succeed or fail?"
- "How long are requests taking?"

The dashboard answers all of these in a browser window that updates in real-time via WebSocket — no manual refreshing needed. If WebSocket fails, it automatically falls back to polling. If JavaScript is disabled entirely, it falls back to a `<meta>` refresh every 5 seconds. Three layers of resilience.

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                  How Data Reaches the Browser                           │
│                                                                         │
│  Data Sources                  Transport              Browser           │
│  ════════════                  ═════════              ═══════           │
│                                                                         │
│  ┌──────────────┐                                                       │
│  │ Health       │──┐                                                    │
│  │ Checker      │  │  broadcast::Sender                                 │
│  │              │  │  (ws_broadcast)        ┌─────────────┐             │
│  │ Checks each  │  ├──────────────────────▶│  WebSocket  │             │
│  │ backend's    │  │   BackendStatus        │  /ws        │─────┐      │
│  │ health and   │  │   ModelChange          └─────────────┘     │      │
│  │ models       │  │                                             │      │
│  └──────────────┘  │                                             ▼      │
│                    │                               ┌──────────────────┐ │
│  ┌──────────────┐  │                               │   dashboard.js   │ │
│  │ Completions  │──┘                               │                  │ │
│  │ Handler      │   RequestComplete                │  • Renders cards │ │
│  │              │                                  │  • Updates table │ │
│  │ Records each │                                  │  • Shows status  │ │
│  │ request      │                                  └──────────────────┘ │
│  │ result       │                                          ▲            │
│  └──────────────┘                                          │            │
│         │                                                  │            │
│         ▼                                                  │            │
│  ┌──────────────┐     ┌─────────────┐                      │            │
│  │ Request      │────▶│ GET         │──────────────────────┘            │
│  │ History      │     │ /v1/history  │   (polling fallback)             │
│  │ (ring buffer)│     └─────────────┘                                   │
│  └──────────────┘                                                       │
│                                                                         │
│  Initial page load:                                                     │
│  ┌──────────────┐     ┌─────────────┐                                   │
│  │ Registry +   │────▶│ GET /       │──▶ HTML with injected JSON        │
│  │ Metrics      │     │ (dashboard) │    (no extra fetch needed)        │
│  └──────────────┘     └─────────────┘                                   │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## File Structure

```
src/
├── dashboard/                   ← NEW MODULE (this feature)
│   ├── mod.rs                   # Module exports and public API
│   ├── types.rs                 # HistoryEntry, RequestStatus, WebSocketUpdate, UpdateType
│   ├── history.rs               # Ring buffer (VecDeque + RwLock, 100-entry cap)
│   ├── handler.rs               # HTTP handlers for /, /assets/*, /v1/history
│   └── websocket.rs             # WebSocket handler + update constructors
├── api/
│   ├── mod.rs                   # MODIFIED: added request_history + ws_broadcast to AppState
│   └── completions.rs           # MODIFIED: added record_request_completion() hook
├── health/
│   └── mod.rs                   # MODIFIED: broadcasts BackendStatus + ModelChange updates
├── cli/
│   └── serve.rs                 # MODIFIED: wires broadcast channel to health checker
dashboard/                       ← FRONTEND ASSETS (embedded at compile time)
├── index.html                   # HTML structure with data injection point
├── styles.css                   # Dark mode, responsive grid, status indicators
└── dashboard.js                 # WebSocket client, polling fallback, DOM rendering
tests/
├── dashboard_contract.rs        # WebSocket message schema validation
└── dashboard_integration.rs     # HTTP endpoint integration tests
```

---

## File 1: types.rs — Data Types

This file defines four types that form the **contract** between the Rust backend and the JavaScript frontend. Every piece of data that flows through WebSocket or the history API is shaped by these types.

### HistoryEntry — One Completed Request

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: u64,              // Unix seconds (e.g., 1720000000)
    pub model: String,               // "llama3:8b"
    pub backend_id: String,          // "ollama-local"
    pub duration_ms: u64,            // 1523
    pub status: RequestStatus,       // Success or Error
    pub error_message: Option<String>, // None for success, Some("...") for errors
}
```

**Why `u64` for timestamp?** Unix timestamps fit comfortably in `u64` and are easy to convert in JavaScript (`new Date(timestamp * 1000)`). No timezone headaches.

**Why `Option<String>` for error_message?** Successful requests don't need an error message. Using `Option` makes that explicit — `None` means success, `Some(msg)` means something went wrong. In JSON, this serializes as `null` vs `"Connection refused"`.

### RequestStatus — Success or Error

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RequestStatus {
    Success,
    Error,
}
```

A simple two-variant enum. We derive `PartialEq` and `Eq` so we can compare statuses in tests and conditionals. In JSON, this becomes `"Success"` or `"Error"`.

### WebSocketUpdate — The Envelope

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketUpdate {
    pub update_type: UpdateType,     // What kind of update
    pub data: serde_json::Value,     // The payload (flexible JSON)
}
```

**Why `serde_json::Value` instead of a typed enum?** The `data` field holds different shapes depending on `update_type` — backend lists, request entries, or model arrays. Using `serde_json::Value` avoids a complex tagged union and keeps the type simple. The JavaScript side inspects `update_type` to know how to parse `data`.

### UpdateType — Three Kinds of Updates

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UpdateType {
    BackendStatus,     // Health checker found status changes
    RequestComplete,   // A chat completion finished
    ModelChange,       // Backend's model list changed
}
```

These three cover everything the dashboard needs to stay current. Each maps to a handler function in `dashboard.js`.

---

## File 2: history.rs — Ring Buffer

This file implements a **fixed-size circular buffer** that stores the last 100 completed requests. Think of it like a security camera that records on a loop — once the tape is full, the oldest footage gets overwritten.

### Why VecDeque?

```rust
pub struct RequestHistory {
    entries: RwLock<VecDeque<HistoryEntry>>,
    capacity: usize,
}
```

`VecDeque` (double-ended queue) is the perfect data structure here because:

| Operation | VecDeque | Vec |
|-----------|----------|-----|
| Push to back | O(1) | O(1) |
| Pop from front | **O(1)** | **O(n)** ← must shift all elements |
| Random access | O(1) | O(1) |

Since we push new entries to the back and evict old entries from the front, `VecDeque` gives us O(1) for both operations. With `Vec`, evicting from the front would require shifting every element — terrible for a hot path.

### Why RwLock (Not Mutex)?

```rust
entries: RwLock<VecDeque<HistoryEntry>>,
```

`RwLock` allows **multiple readers OR one writer**. The dashboard handler reads the history (via `get_all()`) far more often than the completions handler writes to it (via `push()`). With `Mutex`, every read would block every other read. With `RwLock`, ten dashboard clients can read simultaneously.

### The Push Method — FIFO Eviction with Validation

```rust
pub fn push(&self, mut entry: HistoryEntry) {
    // Validate timestamp is not in the future
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    if entry.timestamp > now + 60 {
        entry.timestamp = now;  // Allow 60s clock skew
    }

    // Truncate model name to 256 chars
    if entry.model.len() > 256 {
        entry.model.truncate(256);
    }

    // Truncate error message to 1024 chars
    if let Some(ref mut error_msg) = entry.error_message {
        if error_msg.len() > 1024 {
            error_msg.truncate(1024);
        }
    }

    let mut entries = self.entries.write().unwrap();
    if entries.len() >= self.capacity {
        entries.pop_front();   // Evict oldest
    }
    entries.push_back(entry);  // Add newest
}
```

**Three defensive validations before storing:**

1. **Timestamp clamping** — If a clock is wildly off (more than 60 seconds in the future), we clamp to `now`. This prevents a single bad entry from confusing the dashboard's time display.
2. **Model name truncation** — Model names come from backends and could theoretically be very long. 256 chars is generous but bounded.
3. **Error message truncation** — Error messages from backends can be arbitrarily long stack traces. 1024 chars is enough for debugging without blowing up memory.

**The eviction logic is two lines:**
```rust
if entries.len() >= self.capacity {
    entries.pop_front();   // O(1) — remove the oldest entry
}
entries.push_back(entry);  // O(1) — add the new entry
```

### Ring Buffer Unit Tests

The test module in `history.rs` covers the critical behaviors:

**`test_new_creates_empty_history`** — Sanity check that a fresh buffer is empty:
```rust
let history = RequestHistory::new();
assert_eq!(history.len(), 0);
assert!(history.is_empty());
```

**`test_ring_buffer_eviction_fifo`** — The most important test. Adds 105 entries to a buffer with capacity 100:
```rust
for i in 0..105 {
    history.push(/* entry with timestamp = i */);
}

assert_eq!(history.len(), 100);          // Capped at 100
assert_eq!(entries[0].timestamp, 5);     // Entries 0-4 were evicted
assert_eq!(entries[99].timestamp, 104);  // Latest entry is last
```

This verifies FIFO (First In, First Out) — the oldest entries are evicted first, and we never exceed capacity.

**`test_get_all_returns_chronological_order`** — Verifies that `get_all()` returns entries in push order (insertion order), not sorted by timestamp. This is a subtle but important distinction: if you push entries with timestamps `[4, 3, 2, 1, 0]`, you get them back in that order.

---

## File 3: handler.rs — HTTP Handlers

This file serves the dashboard HTML, static assets, and the history JSON endpoint. It uses `rust-embed` to compile frontend files directly into the binary.

### Embedded Assets with rust-embed

```rust
#[derive(RustEmbed)]
#[folder = "dashboard/"]
struct DashboardAssets;
```

**What `rust-embed` does:** At compile time, it reads every file in the `dashboard/` directory and embeds them as byte arrays in the binary. At runtime, `DashboardAssets::get("index.html")` returns the file contents without touching the filesystem. This means:

- **No "file not found" at runtime** — if it compiled, the files are there
- **Single binary deployment** — no need to ship a `dashboard/` folder alongside the binary
- **Zero disk I/O** — assets are served from memory

### dashboard_handler — The Main Page

```rust
pub async fn dashboard_handler(State(state): State<Arc<AppState>>) -> Response {
    match DashboardAssets::get("index.html") {
        Some(content) => {
            // ... parse HTML, inject data, return
        }
        None => (StatusCode::INTERNAL_SERVER_ERROR, "Dashboard HTML not found").into_response(),
    }
}
```

The interesting part is the **data injection**. Instead of having the browser immediately make API calls on load, we embed the current state directly into the HTML:

```rust
// Generate initial stats data
let stats = crate::metrics::types::StatsResponse { /* ... */ };
let stats_json = serde_json::to_string(&stats).unwrap_or_else(|_| "{}".to_string());

// Generate initial models data
let models_json = serde_json::to_string(&models).unwrap_or_else(|_| "{}".to_string());

// Inject into HTML template
let initial_data = format!(r#"{{"stats":{}, "models":{}}}"#, stats_json, models_json);
let updated_html = html.replace(
    r#"<script id="initial-data" type="application/json">
        {}
    </script>"#,
    &format!(
        r#"<script id="initial-data" type="application/json">
        {}
    </script>"#,
        initial_data
    ),
);
```

**Why inject data into HTML?** This eliminates the "loading spinner" on first page load. The browser gets pre-rendered data immediately — no round-trip to `/v1/stats` and `/v1/models` needed. The JavaScript reads this injected JSON on `DOMContentLoaded`.

### history_handler — Request History API

```rust
pub async fn history_handler(State(state): State<Arc<AppState>>) -> Response {
    let entries = state.request_history.get_all();
    axum::Json(entries).into_response()
}
```

Simple and clean — reads the ring buffer and returns JSON. Used by the frontend's `fetchRequestHistory()` function and as a polling fallback.

### assets_handler — Static Files

```rust
pub async fn assets_handler(Path(path): Path<String>) -> Response {
    match DashboardAssets::get(&path) {
        Some(content) => {
            let mime_type = mime_guess::from_path(&path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime_type.as_ref())], body).into_response()
        }
        None => (StatusCode::NOT_FOUND, "Asset not found").into_response(),
    }
}
```

Serves CSS and JS files with correct MIME types. `mime_guess` determines the type from the file extension (`.css` → `text/css`, `.js` → `application/javascript`). This is important — browsers refuse to execute scripts served with the wrong MIME type.

### SystemSummary and Helpers

```rust
pub struct SystemSummary {
    pub uptime_seconds: u64,
    pub total_requests: u64,
}
```

A simple struct for the dashboard header cards. The `calculate_uptime` helper converts a start `Instant` to a `Duration`.

---

## File 4: websocket.rs — Real-Time Updates

This is where the magic of live updates happens. Instead of the browser polling every few seconds, the server **pushes** updates the instant something changes.

### The WebSocket Handler

```rust
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}
```

`WebSocketUpgrade` is an axum extractor that handles the HTTP → WebSocket protocol upgrade. The `on_upgrade` callback runs once the connection is established.

### handle_socket — Two Concurrent Tasks

```rust
async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to broadcast channel
    let mut rx = state.ws_broadcast.subscribe();

    // Task 1: Forward broadcast messages → WebSocket
    let send_task = tokio::spawn(async move {
        while let Ok(update) = rx.recv().await {
            match serde_json::to_string(&update) {
                Ok(json) => {
                    if json.len() > 10 * 1024 {
                        continue;  // Skip oversized messages
                    }
                    if (sender.send(Message::Text(json)).await).is_err() {
                        break;     // Client disconnected
                    }
                }
                Err(e) => tracing::error!("Failed to serialize: {}", e),
            }
        }
    });

    // Task 2: Handle incoming messages (ping/pong, close)
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Close(_) => break,
                Message::Ping(_) => { /* axum handles pong automatically */ }
                _ => { /* Ignore other messages */ }
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }
}
```

**The split pattern:** `socket.split()` divides the WebSocket into a sender half and a receiver half. This lets us run sending and receiving as **separate concurrent tasks**. Without splitting, we'd need a `Mutex` to share the socket.

**The broadcast pattern:** `state.ws_broadcast.subscribe()` creates a new receiver for the broadcast channel. Every subscriber gets every message. When the health checker or completions handler sends an update, all connected WebSocket clients receive it simultaneously.

**The 10KB guard:** Messages larger than 10KB are dropped rather than truncated. Truncating JSON would produce malformed data. In practice, backend status updates are a few hundred bytes.

**`tokio::select!` for cleanup:** When either task finishes (client disconnects or broadcast channel closes), the other task is cancelled. This prevents orphaned tasks from accumulating.

### Update Constructor Functions

```rust
pub fn create_backend_status_update(backends: Vec<BackendView>) -> WebSocketUpdate {
    WebSocketUpdate {
        update_type: UpdateType::BackendStatus,
        data: serde_json::to_value(backends).unwrap_or(serde_json::Value::Null),
    }
}

pub fn create_request_complete_update(entry: HistoryEntry) -> WebSocketUpdate {
    WebSocketUpdate {
        update_type: UpdateType::RequestComplete,
        data: serde_json::to_value(entry).unwrap_or(serde_json::Value::Null),
    }
}

pub fn create_model_change_update(backend_id: String, models: Vec<serde_json::Value>) -> WebSocketUpdate {
    WebSocketUpdate {
        update_type: UpdateType::ModelChange,
        data: serde_json::json!({
            "backend_id": backend_id,
            "models": models,
        }),
    }
}
```

These are **factory functions** — they standardize message creation so every caller produces consistent JSON shapes. Without them, each call site would hand-build JSON, inviting inconsistencies.

---

## File 5: mod.rs — Module Exports

```rust
pub mod handler;
pub mod history;
pub mod types;
pub mod websocket;

pub use handler::{assets_handler, dashboard_handler, history_handler};
pub use websocket::websocket_handler;
```

**What `pub use` does:** It re-exports items so callers can write `crate::dashboard::dashboard_handler` instead of `crate::dashboard::handler::dashboard_handler`. This keeps import paths clean — the module's internal file structure is hidden from consumers.

---

## File 6: index.html — HTML Structure

The HTML is structured into five visual sections:

```html
<!-- 1. Header with connection status indicator -->
<header class="header">
    <h1>Nexus Dashboard</h1>
    <div id="connection-status" class="connection-status">
        <span class="status-dot"></span>
        <span class="status-text">Connecting...</span>
    </div>
</header>

<!-- 2. Summary cards (uptime, requests, backends, models) -->
<section class="summary-section">
    <div class="summary-cards">
        <div class="summary-card">
            <div class="summary-label">Uptime</div>
            <div id="uptime" class="summary-value">--</div>
        </div>
        <!-- ... more cards ... -->
    </div>
</section>

<!-- 3. Backend status grid (cards with health indicators) -->
<section class="backend-section">
    <div id="backend-cards" class="backend-grid"></div>
</section>

<!-- 4. Model availability matrix (table with ✓/— indicators) -->
<section class="model-section">
    <table id="model-matrix" class="model-matrix">
        <!-- Dynamic columns per backend -->
    </table>
</section>

<!-- 5. Request history table (last 100 requests) -->
<section class="history-section">
    <table id="history-table">
        <!-- Time, Model, Backend, Duration, Status -->
    </table>
</section>
```

### The Data Injection Point

```html
<script id="initial-data" type="application/json">
    {}
</script>
```

This `<script>` tag with `type="application/json"` is **not executed as JavaScript**. The browser treats it as inert data. The Rust `dashboard_handler` replaces `{}` with the actual stats and models JSON. The JavaScript reads it via `document.getElementById('initial-data').textContent`.

### The No-JavaScript Fallback

```html
<noscript>
    <div class="noscript-banner">
        <p><strong>JavaScript is disabled.</strong></p>
        <p>Enable JavaScript for full dashboard functionality.</p>
        <a href="/" class="refresh-button">Refresh Now</a>
    </div>
    <meta http-equiv="refresh" content="5">
</noscript>
```

The `<noscript>` block only renders if JavaScript is disabled. The `<meta http-equiv="refresh" content="5">` tells the browser to reload the page every 5 seconds — providing a degraded but functional experience. Since `dashboard_handler` injects fresh data on every page load, each refresh shows current state.

---

## File 7: styles.css — Styling and Responsiveness

### Dark Mode via CSS Custom Properties

```css
:root {
    --bg-primary: #ffffff;
    --success: #28a745;
    --error: #dc3545;
    /* ... */
}

@media (prefers-color-scheme: dark) {
    :root {
        --bg-primary: #1a1a1a;
        --success: #4caf50;   /* Brighter green for WCAG AA on dark bg */
        --error: #f44336;
        /* ... */
    }
}
```

**Why different colors for dark mode?** The light mode green (`#28a745`) doesn't have enough contrast against a dark background to meet WCAG AA accessibility standards. The dark mode uses brighter variants (`#4caf50`) that are readable on dark surfaces.

### Responsive Backend Grid

```css
/* Mobile: 1 column */
@media (max-width: 767px) {
    .backend-grid { grid-template-columns: 1fr; }
}

/* Tablet: 2 columns */
@media (min-width: 768px) and (max-width: 1023px) {
    .backend-grid { grid-template-columns: repeat(2, 1fr); }
}

/* Desktop: 3 columns */
@media (min-width: 1024px) {
    .backend-grid { grid-template-columns: repeat(3, 1fr); }
}
```

Three breakpoints ensure the backend cards look good on phones (1 column), tablets (2 columns), and desktops (3 columns).

### Status Indicators

```css
.backend-card.healthy { border-left: 4px solid var(--success); }
.backend-card.unhealthy { border-left: 4px solid var(--error); }
.backend-card.unknown { border-left: 4px solid var(--warning); }

.status-dot.connected {
    background: var(--success);
    animation: pulse 2s infinite;    /* Pulsing green = alive */
}
```

Backend cards use a colored left border for at-a-glance status. The connection status dot in the header pulses when connected — a visual heartbeat that confirms the WebSocket is alive.

### Mobile Touch Targets

```css
@media (max-width: 767px) {
    .backend-card, .summary-card, .status-error {
        min-height: 44px;   /* Apple's recommended touch target size */
    }
    input, select, textarea {
        font-size: 16px;    /* Prevents iOS auto-zoom on focus */
    }
}
```

Two important mobile-specific rules: 44px minimum tap targets (per Apple HIG) and 16px minimum font size to prevent iOS Safari from zooming in when a user focuses an input.

---

## File 8: dashboard.js — Frontend Logic

### Initialization

```javascript
document.addEventListener('DOMContentLoaded', () => {
    loadInitialData();         // Read injected JSON from HTML
    fetchSystemSummary();      // Fetch /v1/stats
    fetchModels();             // Fetch /v1/models
    fetchRequestHistory();     // Fetch /v1/history
    connectWebSocket();        // Open WebSocket to /ws

    setInterval(fetchSystemSummary, 5000);   // Refresh stats every 5s
    setInterval(fetchModels, 30000);          // Refresh models every 30s
});
```

**Why both inject AND fetch?** The injected data provides instant display, but the fetches ensure we have the latest data in case anything changed between the HTML being generated and the JavaScript executing. The intervals keep the data fresh even if WebSocket messages are missed.

### WebSocket Connection with Exponential Backoff

```javascript
function handleWebSocketClose(event) {
    if (reconnectAttempts < MAX_RECONNECT_ATTEMPTS) {
        reconnectAttempts++;
        // Exponential backoff: 3s, 6s, 12s, 24s, 48s (capped at 60s)
        currentReconnectDelay = Math.min(
            BASE_RECONNECT_DELAY * Math.pow(2, reconnectAttempts - 1),
            60000
        );
        setTimeout(connectWebSocket, currentReconnectDelay);
    } else {
        startPolling();  // Give up on WebSocket, fall back to polling
    }
}
```

**Exponential backoff** prevents a reconnection storm. If the server is temporarily down, hammering it with reconnects makes things worse. The delays double each time: 3s → 6s → 12s → 24s → 48s. After 5 failures, it switches to polling mode.

### WebSocket Message Dispatch

```javascript
function handleWebSocketMessage(event) {
    const update = JSON.parse(event.data);

    switch (update.update_type) {
        case 'BackendStatus':
            handleBackendStatusUpdate(update.data);  // Refresh backend cards
            break;
        case 'RequestComplete':
            handleRequestCompleteUpdate(update.data); // Add row to history
            break;
        case 'ModelChange':
            handleModelChangeUpdate(update.data);     // Refresh model matrix
            break;
    }
}
```

The `update_type` field (from `UpdateType` in Rust) determines which handler processes the data. This is a simple **command pattern** — the message tells the client what to do.

### Polling Fallback

```javascript
function startPolling() {
    pollingInterval = setInterval(async () => {
        const response = await fetch('/v1/stats');
        if (response.ok) {
            const data = await response.json();
            updateSystemSummary(data);
        }
    }, 5000);
}
```

Polling is the **last resort** after WebSocket fails. It provides degraded real-time updates (5-second lag instead of instant) but keeps the dashboard functional. When WebSocket reconnects successfully, polling stops automatically via `stopPolling()`.

### Request History DOM Management

```javascript
function addRequestToHistory(entry) {
    const row = renderRequestRow(entry);
    tbody.insertBefore(row, tbody.firstChild);   // Newest at top

    while (tbody.children.length > 100) {
        tbody.removeChild(tbody.lastChild);       // Cap at 100 rows
    }
}
```

This mirrors the ring buffer on the server side — 100-entry cap, FIFO eviction. New entries appear at the top (reverse chronological), and when the table exceeds 100 rows, the oldest rows are removed from the bottom.

---

## File 9: api/mod.rs — Wiring It All Together

Two new fields were added to `AppState`:

```rust
pub struct AppState {
    // ... existing fields ...

    /// Request history ring buffer for dashboard
    pub request_history: Arc<RequestHistory>,
    /// WebSocket broadcast channel for dashboard real-time updates
    pub ws_broadcast: broadcast::Sender<WebSocketUpdate>,
}
```

And in the constructor:

```rust
// Create request history ring buffer for dashboard
let request_history = Arc::new(RequestHistory::new());

// Create WebSocket broadcast channel for dashboard real-time updates
let (ws_broadcast, _) = broadcast::channel(1000);
```

**Why `broadcast::channel(1000)`?** The `1000` is the channel capacity — how many messages can be buffered if a subscriber falls behind. If a WebSocket client is slow and 1000 messages pile up, older messages are dropped. This prevents a slow client from causing memory to grow unboundedly.

**Why discard the receiver `_`?** `broadcast::channel` returns `(Sender, Receiver)`. We only need the `Sender` in `AppState` — each WebSocket handler creates its own `Receiver` via `ws_broadcast.subscribe()`. The initial receiver is unused.

### Route Registration

```rust
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Dashboard routes
        .route("/", get(crate::dashboard::dashboard_handler))
        .route("/assets/*path", get(crate::dashboard::assets_handler))
        .route("/ws", get(crate::dashboard::websocket_handler))
        // API routes
        .route("/v1/chat/completions", post(completions::handle))
        .route("/v1/models", get(models::handle))
        .route("/v1/history", get(crate::dashboard::history_handler))
        .route("/health", get(health::handle))
        .route("/metrics", get(crate::metrics::handler::metrics_handler))
        .route("/v1/stats", get(crate::metrics::handler::stats_handler))
        // ...
}
```

Four new routes: `/` (dashboard HTML), `/assets/*path` (CSS/JS), `/ws` (WebSocket), and `/v1/history` (history API).

---

## File 10: api/completions.rs — Request Recording Hook

The `record_request_completion` function is called at every exit point of the completions handler — both success and error paths:

```rust
fn record_request_completion(
    state: &Arc<AppState>,
    model: &str,
    backend_id: &str,
    duration_ms: u64,
    status: crate::dashboard::types::RequestStatus,
    error_message: Option<String>,
) {
    let entry = HistoryEntry {
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        model: model.to_string(),
        backend_id: backend_id.to_string(),
        duration_ms,
        status: status.clone(),
        error_message,
    };

    // Push to request history ring buffer
    state.request_history.push(entry.clone());

    // Broadcast update to WebSocket clients
    let update = create_request_complete_update(entry);
    let _ = state.ws_broadcast.send(update);
}
```

**Two things happen:**
1. The entry is pushed into the ring buffer (for the `/v1/history` endpoint and initial page loads)
2. The entry is broadcast to all WebSocket clients (for real-time updates)

**Why `let _ = state.ws_broadcast.send(update)`?** The `_` discards the `Result`. If no WebSocket clients are connected, `send()` returns `Err` (no receivers). This is expected and harmless — we don't want a missing dashboard client to cause an error log on every request.

**Three call sites in the handler:**
- Routing error (model not found, etc.) → `status: Error, backend_id: "none"`
- Successful proxy response → `status: Success`
- Final retry failure → `status: Error` with the backend error message

---

## File 11: health/mod.rs — Backend Status Broadcasting

The health checker was extended to broadcast updates after each health check cycle:

```rust
pub struct HealthChecker {
    // ... existing fields ...
    ws_broadcast: Option<tokio::sync::broadcast::Sender<WebSocketUpdate>>,
}

impl HealthChecker {
    pub fn with_broadcast(mut self, sender: broadcast::Sender<WebSocketUpdate>) -> Self {
        self.ws_broadcast = Some(sender);
        self
    }
}
```

**Why `Option<Sender>`?** The health checker can work without a broadcast channel (e.g., in tests or when the dashboard is disabled). Using `Option` makes the broadcast channel opt-in.

**Why a builder method (`with_broadcast`)?** This follows the builder pattern — chain it onto `new()` when you want broadcasting, skip it when you don't. No need to change every existing test.

### Backend Status Broadcast

After all backends are checked, the full backend list is broadcast:

```rust
pub async fn check_all_backends(&self) -> Vec<(String, HealthCheckResult)> {
    // ... check each backend ...

    // Broadcast backend status update after all checks complete
    if let Some(ws_broadcast) = &self.ws_broadcast {
        let backends = self.registry.get_all_backends();
        let backend_views: Vec<_> = backends.iter().map(|b| b.into()).collect();
        let update = create_backend_status_update(backend_views);
        let _ = ws_broadcast.send(update);
    }

    results
}
```

**Why send `BackendView` (not `Backend`)?** `Backend` contains atomics and internal state that can't be serialized. `BackendView` is the serializable "view model" — simple fields, no atomics. The `From<&Backend> for BackendView` conversion handles this.

### Model Change Broadcast

When a backend's model list changes during health checks:

```rust
fn broadcast_model_change(&self, backend_id: &str, models: &[Model]) {
    if let Some(ref sender) = self.ws_broadcast {
        let model_values: Vec<serde_json::Value> = models.iter().map(|m| {
            serde_json::json!({
                "id": m.id,
                "name": m.name,
                "context_length": m.context_length,
                "supports_vision": m.supports_vision,
                "supports_tools": m.supports_tools,
                "supports_json_mode": m.supports_json_mode,
                "max_output_tokens": m.max_output_tokens,
            })
        }).collect();

        let update = create_model_change_update(backend_id.to_string(), model_values);
        let _ = sender.send(update);
    }
}
```

This is called in two places: when models are successfully discovered (new list), and when a backend fails (empty list, signaling models are unavailable).

---

## File 12: cli/serve.rs — Broadcast Channel Wiring

The serve command connects the health checker to the broadcast channel:

```rust
pub async fn run_serve(args: ServeArgs) -> Result<(), Box<dyn std::error::Error>> {
    // ...

    // Build API router and get AppState (to access ws_broadcast)
    let (app, app_state) = build_api_router(registry.clone(), config_arc);

    // Start health checker with broadcast sender
    let health_handle = if config.health_check.enabled {
        let checker = HealthChecker::new(registry.clone(), config.health_check.clone())
            .with_broadcast(app_state.ws_broadcast.clone());
        Some(checker.start(cancel_token.clone()))
    } else {
        None
    };

    // ...
}
```

**The key line is `.with_broadcast(app_state.ws_broadcast.clone())`** — this gives the health checker a clone of the broadcast sender from `AppState`. Both the health checker and the completions handler now publish to the same channel, and all WebSocket clients receive updates from both sources.

**Why `.clone()` on the sender?** `broadcast::Sender` is cheap to clone — it's an `Arc` internally. Multiple producers can send to the same channel simultaneously.

---

## How Data Flows

Here's the complete lifecycle of a request, from arrival to dashboard display:

```
┌─────────────────────────────────────────────────────────────────────────┐
│           Step-by-Step: Request → Dashboard Update                      │
│                                                                         │
│  ① Client sends POST /v1/chat/completions                              │
│     │                                                                   │
│  ② completions::handle() starts timer, routes to backend                │
│     │                                                                   │
│  ③ Backend responds (success or error)                                  │
│     │                                                                   │
│  ④ record_request_completion() is called:                               │
│     │                                                                   │
│     ├──▶ state.request_history.push(entry)                              │
│     │    └── Ring buffer stores it (evicts oldest if at 100)            │
│     │                                                                   │
│     └──▶ state.ws_broadcast.send(RequestComplete update)                │
│          └── Broadcast channel fans out to all subscribers              │
│               │                                                         │
│  ⑤ websocket.rs :: send_task receives the update:                       │
│     │                                                                   │
│     ├── Serializes WebSocketUpdate to JSON                              │
│     ├── Checks size < 10KB                                              │
│     └── Sends via WebSocket to browser                                  │
│               │                                                         │
│  ⑥ dashboard.js :: handleWebSocketMessage() receives it:                │
│     │                                                                   │
│     ├── Parses JSON                                                     │
│     ├── Reads update_type: "RequestComplete"                            │
│     └── Calls addRequestToHistory(data)                                 │
│               │                                                         │
│  ⑦ New row appears at top of Request History table                      │
│     └── Oldest row removed if > 100 rows                                │
└─────────────────────────────────────────────────────────────────────────┘
```

The same pattern applies for backend status changes (triggered by the health checker) and model changes (triggered during health check model discovery).

---

## Key Tests Explained

### Contract Tests (tests/dashboard_contract.rs)

These tests verify that the **JSON shape** of WebSocket messages matches what the frontend expects. They don't test HTTP endpoints — just serialization and deserialization.

**`test_websocket_update_deserialization_with_backend_status`** — Can we deserialize a `BackendStatus` message from raw JSON?

```rust
let json_data = json!({
    "update_type": "BackendStatus",
    "data": { "backend_id": "backend-1", "status": "Healthy", "pending_requests": 5 }
});

let result: Result<WebSocketUpdate, _> = serde_json::from_value(json_data);
assert!(result.is_ok());
assert_eq!(update.update_type, UpdateType::BackendStatus);
```

**Why this matters:** If someone renames `UpdateType::BackendStatus` to `BackendHealth`, this test catches the break immediately. The frontend expects `"BackendStatus"` in the JSON.

**`test_backend_status_data_includes_all_required_fields`** — Verifies the round-trip (serialize → deserialize) preserves all required fields:

```rust
let data = deserialized.data.as_object().unwrap();
assert!(data.contains_key("backend_id"));
assert!(data.contains_key("status"));
assert!(data.contains_key("pending_requests"));
```

**`test_model_data_includes_capabilities_fields`** — Verifies that model change messages include capability flags (`vision`, `tools`, `json_mode`) with correct types:

```rust
let capabilities = model_obj["capabilities"].as_object().unwrap();
assert!(capabilities["vision"].is_boolean());
assert!(capabilities["tools"].is_boolean());
assert!(capabilities["json_mode"].is_boolean());
assert!(model_obj["context_length"].is_number());
```

### Integration Tests (tests/dashboard_integration.rs)

These tests verify that HTTP endpoints respond correctly.

**`test_dashboard_endpoint_returns_200_with_html`** — The root endpoint serves HTML:

```rust
let request = Request::builder().uri("/").body(Body::empty()).unwrap();
let response = app.call(request).await.unwrap();

assert_eq!(response.status(), StatusCode::OK);
assert!(content_type.to_str().unwrap().contains("text/html"));
```

**`test_assets_endpoint_returns_css_with_correct_mime`** — CSS assets get the right MIME type:

```rust
let request = Request::builder().uri("/assets/styles.css").body(Body::empty()).unwrap();
// Verify status is OK and content-type contains "text/css"
```

**`test_model_matrix_reflects_model_removal_when_backend_offline`** — End-to-end test that adds a backend with models, removes it, and verifies the registry reflects the change:

```rust
state.registry.add_backend(backend).unwrap();
assert_eq!(backends[0].models.len(), 2);

state.registry.remove_backend("test-backend-1").unwrap();
assert_eq!(backends.len(), 0);
```

### Unit Tests (src/dashboard/handler.rs)

**`test_system_summary_serialization`** — Verifies `SystemSummary` serializes correctly:

```rust
let summary = SystemSummary { uptime_seconds: 3600, total_requests: 1234 };
let json = serde_json::to_string(&summary).unwrap();
assert!(json.contains("3600"));
assert!(json.contains("1234"));
```

---

## Key Rust Concepts

| Concept | What It Means | Example in This Code |
|---------|---------------|----------------------|
| `RwLock<T>` | Multiple readers or one writer | `RequestHistory.entries` — many dashboard readers, one completions writer |
| `VecDeque<T>` | Double-ended queue with O(1) push/pop at both ends | Ring buffer backing store |
| `broadcast::channel` | Multi-producer, multi-consumer channel | `ws_broadcast` — health checker + completions send; WebSocket handlers receive |
| `WebSocketUpgrade` | Axum extractor for HTTP → WebSocket upgrade | `websocket_handler` parameter |
| `socket.split()` | Separates read/write halves of a WebSocket | Enables concurrent send/receive tasks |
| `tokio::select!` | Wait for the first of multiple futures to complete | Cleanup when either WebSocket task finishes |
| `rust-embed` | Embeds files into the binary at compile time | `DashboardAssets` struct for HTML/CSS/JS |
| `serde_json::Value` | Untyped JSON value | `WebSocketUpdate.data` — flexible payload |
| `Arc<T>` | Thread-safe shared ownership | `AppState`, `RequestHistory` shared across handlers |
| `Option<Sender>` | Optional capability (builder pattern) | Health checker works with or without broadcast |

---

## Common Questions

### "Why not use Server-Sent Events (SSE) instead of WebSocket?"

WebSocket is bidirectional, which lets us handle ping/pong for connection health monitoring. SSE would work for the server→client direction but doesn't support client→server messages. Also, axum has excellent WebSocket support built in.

### "Why embed frontend files instead of serving from disk?"

Three reasons: (1) single-binary deployment — no "where's the dashboard folder?" issues, (2) no filesystem I/O at runtime — assets are served from memory, (3) compile-time validation — if a file is missing, the build fails rather than a user getting a 404.

### "What happens if the WebSocket broadcast channel fills up?"

With `broadcast::channel(1000)`, if a subscriber falls 1000 messages behind, it gets a `RecvError::Lagged` error. The current implementation treats this the same as any other receive error — the send task breaks out of the loop and the WebSocket closes. The client then reconnects with exponential backoff.

### "Why is the ring buffer capacity hardcoded to 100?"

100 provides a good balance: enough history to spot trends, small enough to render quickly in the browser. The capacity is set at construction (`RequestHistory::new()`) and could be made configurable in a future version. The frontend's `addRequestToHistory` also caps at 100 rows to stay in sync.

### "How does the no-JavaScript fallback work?"

The `dashboard_handler` injects fresh data into the HTML on every request. The `<noscript>` block adds `<meta http-equiv="refresh" content="5">`, which tells the browser to reload the page every 5 seconds. Each reload gets fresh injected data. It's not real-time, but it works without any JavaScript.

### "What if two completions finish at the exact same time?"

Both call `state.request_history.push()`, which acquires a write lock on the `RwLock`. One will execute first, the other waits. Both entries are stored correctly. The broadcast channel handles concurrent sends natively — `broadcast::Sender` is thread-safe.
