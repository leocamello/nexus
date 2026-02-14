# Quickstart: Web Dashboard Development

**Feature**: F10 Web Dashboard  
**Branch**: `010-web-dashboard`  
**Prerequisites**: Rust 1.75+, Nexus codebase cloned

---

## Overview

This guide helps developers implement and test the embedded web dashboard. The dashboard displays backend health, model availability, and request history with real-time WebSocket updates.

---

## Development Setup

### 1. Create Feature Branch

```bash
git checkout -b 010-web-dashboard
```

### 2. Install Dependencies (Optional)

For CSS preprocessing (one-time setup):

```bash
# Install Tailwind CSS standalone CLI (optional, for CSS changes)
curl -sLO https://github.com/tailwindlabs/tailwindcss/releases/latest/download/tailwindcss-linux-x64
chmod +x tailwindcss-linux-x64
mv tailwindcss-linux-x64 /usr/local/bin/tailwindcss
```

*Note: Tailwind CSS is precompiled and committed. No build step required for most developers.*

### 3. Create Static Assets Directory

```bash
mkdir -p dashboard
```

---

## Project Structure

```
nexus/
├── src/
│   ├── api/mod.rs           # Add dashboard routes
│   ├── dashboard/           # NEW module
│   │   ├── mod.rs
│   │   ├── handler.rs       # HTTP handlers
│   │   ├── websocket.rs     # WebSocket logic
│   │   ├── history.rs       # Ring buffer
│   │   └── types.rs         # Data types
│   └── lib.rs               # Register module
├── dashboard/               # Static assets (embedded)
│   ├── index.html
│   ├── dashboard.js
│   ├── styles.css
│   └── favicon.ico
└── tests/
    └── contract/dashboard_websocket_test.rs
```

---

## Implementation Order (TDD)

### Phase 1: Write Tests (MUST FAIL)

1. **Contract Tests** (`tests/contract/dashboard_websocket_test.rs`):
   ```bash
   cargo test --test dashboard_websocket_test
   # Expected: FAIL (no types exist yet)
   ```

2. **Integration Tests** (`tests/integration/dashboard_test.rs`):
   ```bash
   cargo test --test dashboard_test
   # Expected: FAIL (no handlers exist yet)
   ```

3. **Unit Tests** (`src/dashboard/history.rs` with `#[cfg(test)]`):
   ```bash
   cargo test dashboard::history::tests
   # Expected: FAIL (no module exists yet)
   ```

### Phase 2: Implement Code (MAKE TESTS PASS)

#### Step 1: Create Dashboard Module

```bash
# Create module structure
mkdir -p src/dashboard
touch src/dashboard/{mod.rs,handler.rs,websocket.rs,history.rs,types.rs}
```

**`src/dashboard/mod.rs`**:
```rust
pub mod handler;
pub mod websocket;
pub mod history;
pub mod types;

pub use handler::{dashboard_handler, assets_handler};
pub use websocket::websocket_handler;
pub use history::RequestHistory;
pub use types::{HistoryEntry, RequestStatus, WebSocketUpdate, UpdateType};
```

**Register in `src/lib.rs`**:
```rust
pub mod dashboard;
```

#### Step 2: Implement Types (`src/dashboard/types.rs`)

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub model: String,
    pub backend_id: String,
    pub duration_ms: u64,
    pub status: RequestStatus,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RequestStatus {
    Success,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketUpdate {
    pub update_type: UpdateType,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateType {
    BackendStatus,
    RequestComplete,
    ModelChange,
}
```

Run tests:
```bash
cargo test dashboard::types::tests
# Expected: PASS (if tests exist)
```

#### Step 3: Implement Ring Buffer (`src/dashboard/history.rs`)

See `data-model.md` for full implementation.

```bash
cargo test dashboard::history::tests
# Expected: PASS
```

#### Step 4: Update AppState (`src/api/mod.rs`)

Add new fields:
```rust
use tokio::sync::broadcast;
use crate::dashboard::RequestHistory;

pub struct AppState {
    // ... existing fields
    pub request_history: Arc<RequestHistory>,
    pub ws_broadcast: broadcast::Sender<WebSocketUpdate>,
}
```

Update `AppState::new()`:
```rust
let request_history = RequestHistory::new();
let (ws_broadcast, _) = broadcast::channel(1000);
```

#### Step 5: Implement HTTP Handlers (`src/dashboard/handler.rs`)

```rust
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
};
use rust_embed::RustEmbed;
use std::sync::Arc;

#[derive(RustEmbed)]
#[folder = "dashboard/"]
struct DashboardAssets;

pub async fn dashboard_handler(State(state): State<Arc<AppState>>) -> Response {
    match DashboardAssets::get("index.html") {
        Some(content) => {
            let html = String::from_utf8_lossy(&content.data);
            Html(html.to_string()).into_response()
        }
        None => (StatusCode::NOT_FOUND, "Dashboard not found").into_response(),
    }
}

pub async fn assets_handler(
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Response {
    match DashboardAssets::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.as_ref())],
                content.data,
            ).into_response()
        }
        None => (StatusCode::NOT_FOUND, "Asset not found").into_response(),
    }
}
```

#### Step 6: Implement WebSocket Handler (`src/dashboard/websocket.rs`)

```rust
use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use std::sync::Arc;

pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let mut rx = state.ws_broadcast.subscribe();
    let (mut sender, mut receiver) = socket.split();

    // Send updates to client
    tokio::spawn(async move {
        while let Ok(update) = rx.recv().await {
            let json = serde_json::to_string(&update).unwrap();
            if sender.send(Message::Text(json)).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming messages (ping/pong only)
    while let Some(Ok(msg)) = receiver.next().await {
        if msg.is_close() {
            break;
        }
    }
}
```

#### Step 7: Add Routes (`src/api/mod.rs`)

```rust
use crate::dashboard::{dashboard_handler, assets_handler, websocket_handler};

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(dashboard_handler))
        .route("/assets/*path", get(assets_handler))
        .route("/ws", get(websocket_handler))
        // ... existing routes
        .with_state(state)
}
```

#### Step 8: Create Static Assets

**`dashboard/index.html`** (minimal version):
```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Nexus Dashboard</title>
    <link rel="stylesheet" href="/assets/styles.css">
</head>
<body class="bg-gray-100 dark:bg-gray-900 text-gray-900 dark:text-gray-100">
    <div id="app">
        <h1>Nexus Dashboard</h1>
        <div id="backend-status"></div>
        <div id="request-history"></div>
    </div>
    <script src="/assets/dashboard.js"></script>
</body>
</html>
```

**`dashboard/dashboard.js`** (minimal version):
```javascript
let ws;

function connectWebSocket() {
    ws = new WebSocket(`ws://${location.host}/ws`);
    
    ws.onmessage = (event) => {
        const update = JSON.parse(event.data);
        handleUpdate(update);
    };
    
    ws.onerror = () => {
        console.log('WebSocket error, falling back to polling');
        startPolling();
    };
}

function handleUpdate(update) {
    console.log('Received update:', update);
    // TODO: Update DOM based on update.update_type
}

connectWebSocket();
```

**`dashboard/styles.css`** (precompiled Tailwind - placeholder):
```css
/* Tailwind CSS output will be here */
body {
    font-family: system-ui, sans-serif;
}
```

### Phase 3: Run Tests (SHOULD PASS)

```bash
# All tests
cargo test

# Specific test suites
cargo test dashboard
cargo test --test dashboard_websocket_test
cargo test --test dashboard_test
```

Expected: All tests PASS ✓

---

## Manual Testing

### 1. Start Nexus

```bash
cargo run -- serve
```

### 2. Access Dashboard

Open browser: `http://localhost:8000/` (or your configured port)

**Dashboard Features:**
- **System Summary**: Shows uptime, total requests, active backends, and available models
- **Backend Status**: Real-time health indicators for each backend with metrics (pending requests, latency)
- **Model Availability Matrix**: Grid showing which models are available on which backends with capabilities (vision, tools, JSON mode)
- **Request History**: Last 100 requests with timestamps, models, backends, durations, and error details (click error rows to expand)
- **Connection Status**: Indicator showing WebSocket connection status (connected/polling/disconnected)

### 3. Test WebSocket

Open browser DevTools → Network → WS tab → Verify WebSocket connection to `/ws`

You should see:
- Initial connection message
- Periodic backend status updates (every 5 seconds)
- Model change updates when backends are added/removed
- Request complete updates when requests finish

### 4. Trigger Updates

In another terminal:
```bash
# Trigger backend health check (will send backend_status update)
curl http://localhost:8000/v1/models

# Send a request (will send request_complete update)
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"llama3:70b","messages":[{"role":"user","content":"hello"}]}'
```

Verify dashboard updates in real-time.

### 5. Test JavaScript-Disabled Mode

Disable JavaScript in browser settings → Reload dashboard → Verify:
- Yellow banner appears at top: "JavaScript is disabled"
- Page auto-refreshes every 5 seconds
- "Refresh Now" button works
- Initial data embedded in page loads correctly

### 6. Test Mobile View

Open DevTools → Toggle device toolbar → Select mobile device → Verify:
- Backend cards stack vertically on mobile (320px, 375px widths)
- Model matrix scrolls horizontally
- Request history table remains readable
- Touch targets are at least 44x44px
- Font size is 16px minimum (prevents iOS zoom)
- Dark mode works based on system preference

### 7. Test Reconnection & Polling Fallback

1. Kill Nexus server while dashboard is open
2. Observe connection status changes to "Disconnected"
3. Restart Nexus
4. Verify dashboard reconnects within 3-60 seconds (exponential backoff)
5. If reconnection fails 5 times, verify status changes to "Polling Mode"

### 8. Test Edge Cases

**No Backends:**
- Start Nexus without configured backends
- Verify "No backends configured" message appears

**No History:**
- Fresh Nexus instance with no requests
- Verify "No requests recorded yet" message appears

**Long Model Names:**
- Add backend with model name > 50 characters
- Verify name truncates with ellipsis, hover shows full name

**Null Latency:**
- Backend with no recorded latency
- Verify "N/A" displays instead of error

---

## Debugging Tips

### WebSocket Not Connecting

1. Check console for errors: `ws.readyState` should be 1 (OPEN)
2. Verify route registered: `GET /ws` in Axum router
3. Check firewall: WebSocket uses same port as HTTP

### Dashboard Not Loading

1. Verify `rust-embed` in `Cargo.toml`:
   ```toml
   [dependencies]
   rust-embed = "8.0"
   ```
2. Check `dashboard/` directory exists with files
3. Rebuild: `cargo clean && cargo build`

### Updates Not Appearing

1. Verify broadcast channel created in AppState
2. Check broadcast sender is called when state changes
3. Verify WebSocket client subscribed to receiver

---

## Code Style

Follow Nexus coding standards:
- Run `cargo fmt` before committing
- Run `cargo clippy` and fix warnings
- Add doc comments to public functions
- Write tests before implementation (TDD)

---

## Next Steps

After implementation:

1. Run verification checklist: `.specify/templates/implementation-verification.md`
2. Generate tasks: `speckit.tasks` command
3. Create PR with tests passing
4. Update constitution if new patterns emerge

---

## Resources

- **Feature Spec**: `specs/010-web-dashboard/spec.md`
- **Data Model**: `specs/010-web-dashboard/data-model.md`
- **WebSocket Protocol**: `specs/010-web-dashboard/contracts/websocket.md`
- **Axum WebSocket Docs**: https://docs.rs/axum/latest/axum/extract/ws/
- **rust-embed Docs**: https://docs.rs/rust-embed/latest/rust_embed/
