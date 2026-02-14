# Data Model: Web Dashboard

**Feature**: F10 Web Dashboard  
**Branch**: `010-web-dashboard`  
**Date**: 2024-02-14

## Overview

This document defines the data structures for the web dashboard feature. The dashboard is stateless and read-only, consuming data from existing Registry and MetricsCollector. New entities are minimal: request history entries and WebSocket update messages.

---

## Entity 1: HistoryEntry

**Purpose**: Represents a single completed request in the 100-entry ring buffer.

**Source**: Created by dashboard module when requests complete (hooked into request lifecycle).

**Lifecycle**: 
- Created: When request completes (success or error)
- Updated: Never (immutable once created)
- Deleted: Automatically when buffer exceeds 100 entries (FIFO)

**Storage**: In-memory VecDeque in AppState, wrapped in Arc<RwLock> for concurrent access.

### Fields

| Field | Type | Required | Validation | Description |
|-------|------|----------|------------|-------------|
| `timestamp` | `DateTime<Utc>` | Yes | Must be valid UTC timestamp | When the request completed |
| `model` | `String` | Yes | Non-empty, max 256 chars | Model name requested (e.g., "llama3:70b") |
| `backend_id` | `String` | Yes | Non-empty, must match a backend ID | Which backend served the request |
| `duration_ms` | `u64` | Yes | >= 0 | Request duration in milliseconds |
| `status` | `RequestStatus` | Yes | Must be Success or Error | Outcome of the request |
| `error_message` | `Option<String>` | No | Max 1024 chars if present | Error details if status is Error |

### Relationships

- **Backend**: `backend_id` references a Backend in the Registry (many-to-one)
- **Model**: `model` is a string reference to a Model (not a foreign key, as models can be removed)

### Example

```rust
use chrono::Utc;

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
```

### Serialization (JSON)

```json
{
  "timestamp": "2024-02-14T10:30:45.123Z",
  "model": "llama3:70b",
  "backend_id": "ollama-local-001",
  "duration_ms": 1250,
  "status": "success",
  "error_message": null
}
```

---

## Entity 2: WebSocketUpdate

**Purpose**: Represents a real-time update message sent from server to dashboard clients via WebSocket.

**Source**: Created by dashboard module when state changes (backend status, new request, model change).

**Lifecycle**:
- Created: When observable state changes (health check completes, request finishes, backend added/removed)
- Sent: Broadcasted to all connected WebSocket clients via tokio::sync::broadcast
- Deleted: Immediately after broadcast (no persistence)

**Storage**: Transient (broadcast channel), not persisted.

### Fields

| Field | Type | Required | Validation | Description |
|-------|------|----------|------------|-------------|
| `update_type` | `UpdateType` | Yes | Must be valid enum variant | Type of update (backend_status, request_complete, model_change) |
| `data` | `serde_json::Value` | Yes | Must be valid JSON | Update payload (structure depends on update_type) |

### Update Types

#### BackendStatus Update

Sent when a backend's health status changes.

**Data Structure**:
```json
{
  "update_type": "backend_status",
  "data": {
    "id": "ollama-local-001",
    "name": "Ollama Local",
    "status": "healthy",
    "last_health_check": "2024-02-14T10:30:45.123Z",
    "pending_requests": 3,
    "avg_latency_ms": 1250
  }
}
```

#### RequestComplete Update

Sent when a request finishes (added to history buffer).

**Data Structure**:
```json
{
  "update_type": "request_complete",
  "data": {
    "timestamp": "2024-02-14T10:30:45.123Z",
    "model": "llama3:70b",
    "backend_id": "ollama-local-001",
    "duration_ms": 1250,
    "status": "success",
    "error_message": null
  }
}
```

#### ModelChange Update

Sent when models are added/removed from backends (rare, but possible during discovery).

**Data Structure**:
```json
{
  "update_type": "model_change",
  "data": {
    "backend_id": "ollama-local-001",
    "models": [
      {
        "id": "llama3:70b",
        "name": "Llama 3 70B",
        "context_length": 8192,
        "supports_vision": false,
        "supports_tools": true,
        "supports_json_mode": true,
        "max_output_tokens": null
      }
    ]
  }
}
```

### Rust Definition

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketUpdate {
    pub update_type: UpdateType,
    pub data: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateType {
    BackendStatus,
    RequestComplete,
    ModelChange,
}
```

---

## Entity 3: RequestHistory (Container)

**Purpose**: Thread-safe container for the ring buffer of HistoryEntry items.

**Source**: Created once at startup, stored in AppState.

**Lifecycle**:
- Created: At server startup (in AppState::new)
- Updated: On every request completion (push new entry)
- Deleted: Never (exists for lifetime of server process)

**Storage**: Arc<RwLock<VecDeque<HistoryEntry>>> in AppState.

### Operations

| Operation | Method | Complexity | Thread Safety |
|-----------|--------|------------|---------------|
| Add entry | `push(&self, entry: HistoryEntry)` | O(1) amortized | Write lock (tokio::sync::RwLock) |
| Get all entries | `get_all(&self) -> Vec<HistoryEntry>` | O(n) where n=100 | Read lock |
| Get count | `len(&self) -> usize` | O(1) | Read lock |
| Clear (for testing) | `clear(&self)` | O(n) | Write lock |

### Capacity Management

- **Fixed capacity**: 100 entries (const HISTORY_CAPACITY)
- **Eviction policy**: FIFO (first-in, first-out)
- **Implementation**: When pushing to a full buffer, call `pop_front()` before `push_back()`

### Rust Definition

```rust
use std::collections::VecDeque;
use tokio::sync::RwLock;
use std::sync::Arc;

const HISTORY_CAPACITY: usize = 100;

pub struct RequestHistory {
    entries: RwLock<VecDeque<HistoryEntry>>,
}

impl RequestHistory {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            entries: RwLock::new(VecDeque::with_capacity(HISTORY_CAPACITY)),
        })
    }

    pub async fn push(&self, entry: HistoryEntry) {
        let mut entries = self.entries.write().await;
        if entries.len() >= HISTORY_CAPACITY {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    pub async fn get_all(&self) -> Vec<HistoryEntry> {
        let entries = self.entries.read().await;
        entries.iter().cloned().collect()
    }

    pub async fn len(&self) -> usize {
        let entries = self.entries.read().await;
        entries.len()
    }
}
```

---

## Entity 4: DashboardState (New AppState Field)

**Purpose**: Extends AppState with dashboard-specific state (history buffer and WebSocket broadcaster).

**Modification to Existing AppState**:

```rust
// In src/api/mod.rs (existing file)
pub struct AppState {
    // ... existing fields
    pub registry: Arc<Registry>,
    pub config: Arc<NexusConfig>,
    pub http_client: reqwest::Client,
    pub router: Arc<routing::Router>,
    pub start_time: Instant,
    pub metrics_collector: Arc<MetricsCollector>,
    
    // NEW: Dashboard-specific state
    pub request_history: Arc<RequestHistory>,
    pub ws_broadcast: broadcast::Sender<WebSocketUpdate>,
}
```

**Initialization Changes**:

```rust
impl AppState {
    pub fn new(registry: Arc<Registry>, config: Arc<NexusConfig>) -> Self {
        // ... existing initialization code
        
        // Initialize dashboard components
        let request_history = RequestHistory::new();
        let (ws_broadcast, _) = broadcast::channel(1000); // 1000-message buffer
        
        Self {
            // ... existing fields
            request_history,
            ws_broadcast,
        }
    }
}
```

---

## Existing Entities (Reused)

The dashboard consumes these existing entities without modification:

### BackendView (from src/registry/backend.rs)

Used for displaying backend status in the dashboard.

**Fields used by dashboard**:
- `id`: Backend identifier
- `name`: Display name
- `status`: Health status (Healthy/Unhealthy/Unknown)
- `last_health_check`: Timestamp of last check
- `pending_requests`: Current queue depth
- `avg_latency_ms`: Average latency
- `backend_type`: Backend type (Ollama, vLLM, etc.)

### Model (from src/registry/backend.rs)

Used for displaying model availability matrix.

**Fields used by dashboard**:
- `id`: Model identifier
- `name`: Display name
- `context_length`: Max context window
- `supports_vision`: Boolean flag
- `supports_tools`: Boolean flag
- `supports_json_mode`: Boolean flag

### StatsResponse (from src/metrics/types.rs)

Used by the `/v1/stats` endpoint (already exists, no changes needed).

**Fields**:
- `uptime_seconds`: Server uptime
- `requests`: RequestStats (total, success, errors)
- `backends`: Vec<BackendStats>
- `models`: Vec<ModelStats>

---

## Data Flow Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                        AppState                              │
│                                                              │
│  ┌──────────────┐  ┌────────────────┐  ┌─────────────────┐ │
│  │   Registry   │  │ RequestHistory │  │  ws_broadcast   │ │
│  │   (existing) │  │   (NEW)        │  │    (NEW)        │ │
│  └──────────────┘  └────────────────┘  └─────────────────┘ │
└─────────────────────────────────────────────────────────────┘
         │                     │                      │
         │ read               │ write                │ broadcast
         ▼                     ▼                      ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│ GET /v1/stats   │  │ Request         │  │ WebSocket       │
│ (existing)      │  │ Completion Hook │  │ Clients         │
│                 │  │ (NEW)           │  │ (NEW)           │
└─────────────────┘  └─────────────────┘  └─────────────────┘
         │                     │                      │
         │ JSON                │ HistoryEntry         │ WebSocketUpdate
         ▼                     ▼                      ▼
┌─────────────────────────────────────────────────────────────┐
│                     Dashboard (Browser)                      │
│                                                              │
│  - Backend Status Cards                                      │
│  - Model Availability Matrix                                 │
│  - Request History Table                                     │
└─────────────────────────────────────────────────────────────┘
```

---

## State Transitions

### Backend Status

```
Unknown ──health check──> Healthy
   │                        │
   └──health check──> Unhealthy
   
Healthy ──health fail──> Unhealthy
Unhealthy ──health succeed──> Healthy
```

*Dashboard observes these transitions via Registry and broadcasts updates.*

### Request History

```
[Empty Buffer] ──request completes──> [1 entry]
[1 entry] ──request completes──> [2 entries]
...
[99 entries] ──request completes──> [100 entries]
[100 entries] ──request completes──> [100 entries] (oldest evicted)
```

*Dashboard writes to buffer and broadcasts updates.*

---

## Validation Rules

### HistoryEntry Validation

- `timestamp`: Must not be in the future (allow up to 5 seconds clock skew)
- `model`: Must be non-empty, max 256 characters
- `backend_id`: Must be non-empty, should match a known backend (soft validation, backend may have been removed)
- `duration_ms`: Must be >= 0, reasonable upper bound is 300,000 ms (5 minutes)
- `error_message`: If present, max 1024 characters (truncate longer errors)

### WebSocketUpdate Validation

- `update_type`: Must be a valid UpdateType enum variant
- `data`: Must deserialize to the expected structure for the given update_type
- Message size: Reasonable limit of 10KB per message (prevent memory exhaustion)

---

## Memory Footprint Estimate

| Entity | Size per Instance | Max Instances | Total Memory |
|--------|------------------|---------------|--------------|
| HistoryEntry | ~200 bytes | 100 (fixed) | ~20 KB |
| WebSocketUpdate | ~500 bytes | 1000 (broadcast buffer) | ~500 KB |
| WebSocket connections | ~10 KB | 50 (max concurrent) | ~500 KB |
| **Total** | | | **~1 MB** |

This is well within the constitutional memory constraints (<50MB baseline + <10KB per backend).

---

## Summary

**New Entities**: 
- HistoryEntry (request history record)
- WebSocketUpdate (real-time update message)
- RequestHistory (ring buffer container)

**Modified Entities**:
- AppState (added request_history and ws_broadcast fields)

**Reused Entities**:
- BackendView (display backend status)
- Model (display model capabilities)
- StatsResponse (consume existing endpoint)

All entities are designed for in-memory operation with no persistence. The dashboard is a read-only view with minimal write operations (history buffer updates). Thread safety is achieved through tokio RwLock and broadcast channels.
