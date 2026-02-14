# Data Model: mDNS Discovery (F05)

**Date**: 2025-01-10  
**Phase**: Phase 1 - Design & Contracts

This document defines the data entities and their relationships for the mDNS Discovery feature.

## Core Entities

### 1. MdnsDiscovery

**Purpose**: Main service that monitors the network for LLM backends advertising via mDNS and registers them with the backend registry.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `config` | `DiscoveryConfig` | Owned; immutable after construction |
| `registry` | `Arc<Registry>` | Shared reference to global registry |
| `pending_removal` | `Arc<Mutex<HashMap<String, Instant>>>` | Tracks services awaiting grace period expiry |

**Responsibilities**:
- Create mDNS `ServiceDaemon` and browse for configured service types
- Handle `ServiceResolved` and `ServiceRemoved` events from `mdns-sd`
- Convert discovery events to `Backend` instances and register them
- Track pending removals and clean up stale backends after grace period
- Respond to `CancellationToken` for graceful shutdown

**Lifecycle**: Created once at gateway startup. Consumed by `start()` which moves `self` into a spawned tokio task. Runs until the `CancellationToken` is cancelled.

**Thread Safety**: `pending_removal` uses `Arc<Mutex<HashMap>>` (tokio async mutex). Registry access is thread-safe via `Arc<Registry>` (DashMap-based). The struct itself is moved into a single task, so no concurrent access to `config`.

---

### 2. DiscoveryConfig

**Purpose**: Configuration for mDNS discovery behavior, loaded from TOML config file.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `enabled` | `bool` | Default: `true` |
| `service_types` | `Vec<String>` | Default: `["_ollama._tcp.local", "_llm._tcp.local"]` |
| `grace_period_seconds` | `u64` | Default: `60`; time before removing disappeared services |

**Responsibilities**:
- Control whether mDNS discovery is active
- Define which mDNS service types to browse for
- Configure grace period duration for service disappearance

**Lifecycle**: Deserialized from TOML at startup. Cloned into `MdnsDiscovery` and cleanup task. Immutable after construction.

**Thread Safety**: Implements `Clone`; each task holds its own copy.

---

### 3. DiscoveryEvent

**Purpose**: Internal representation of mDNS service events, decoupling Nexus from the `mdns-sd` library types.

**Attributes**:

| Variant | Fields | Constraints |
|---------|--------|-------------|
| `ServiceFound` | `instance: String`, `service_type: String`, `addresses: Vec<IpAddr>`, `port: u16`, `txt_records: HashMap<String, String>` | `addresses` must not be empty for successful conversion |
| `ServiceRemoved` | `instance: String`, `service_type: String` | Instance must match a previously discovered service |

**Responsibilities**:
- Normalize raw `mdns_sd::ServiceEvent` into a Nexus-internal enum
- Carry all metadata needed to construct a `Backend` or look up an existing one

**Lifecycle**: Created in `handle_mdns_event()` from `mdns_sd::ServiceEvent`, consumed by `handle_event()`. Short-lived (single request cycle).

**Thread Safety**: `Clone + Debug`; passed by reference within a single task.

---

### 4. ParsedService

**Purpose**: Structured metadata extracted from mDNS TXT records, used to configure backend type and URL construction.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `backend_type` | `BackendType` | Inferred from TXT `type` key or service type string |
| `api_path` | `String` | From TXT `api_path` key; default: `""` for Ollama, `"/v1"` for others |
| `version` | `Option<String>` | From TXT `version` key; optional metadata |

**Responsibilities**:
- Parse TXT record `type` field to determine `BackendType` (case-insensitive)
- Infer backend type from service type string when TXT record absent
- Provide default API path based on backend type

**Lifecycle**: Created by `parse_txt_records()`, consumed during `Backend` construction. Short-lived.

**Thread Safety**: `Clone + PartialEq + Eq + Debug`; value type, no shared state.

---

### 5. Pending Removal Entry

**Purpose**: Tracks a discovered service that has disappeared from mDNS, waiting for grace period expiry before removal.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| Key: `instance` | `String` | mDNS instance name (e.g., `"my-server._ollama._tcp.local"`) |
| Value: `removal_time` | `Instant` | Timestamp when `ServiceRemoved` was received |

**Responsibilities**:
- Record when a service disappeared for grace period calculation
- Allow cancellation if service reappears (removed from map on `ServiceFound`)

**Lifecycle**: Inserted on `ServiceRemoved`, removed either on `ServiceFound` (reappearance) or by cleanup task (grace period expired).

**Thread Safety**: Stored in `Arc<Mutex<HashMap<String, Instant>>>` (tokio async mutex); accessed by both main event loop and cleanup task.

---

## Entity Relationships

```
┌─────────────────────────────┐
│       DiscoveryConfig       │
│                             │
│  - enabled                  │
│  - service_types            │
│  - grace_period_seconds     │
└─────────────────────────────┘
              │
              │ configures
              ▼
┌─────────────────────────────┐        ┌──────────────────────┐
│       MdnsDiscovery         │        │    mdns-sd Daemon    │
│                             │        │                      │
│  - config                   │───────▶│  ServiceDaemon       │
│  - registry ─────────────┐  │ creates│  - browse()          │
│  - pending_removal       │  │        │  - shutdown()        │
└─────────────────────────────┘        └──────────────────────┘
              │                │                    │
              │ handles        │ registers          │ emits
              ▼                │                    ▼
┌─────────────────────────┐    │       ┌──────────────────────┐
│    DiscoveryEvent       │    │       │  mdns_sd::Service    │
│                         │    │       │  Event               │
│  ServiceFound {         │    │       └──────────────────────┘
│    instance, addresses, │    │
│    port, txt_records    │    │
│  }                      │    │
│  ServiceRemoved {       │    │
│    instance             │    │
│  }                      │    │
└─────────────────────────┘    │
         │                     │
         │ parsed by           │
         ▼                     ▼
┌─────────────────────┐  ┌──────────────┐
│   ParsedService     │  │   Registry   │
│                     │  │              │
│  - backend_type     │  │ - backends   │
│  - api_path         │  │ - models     │
│  - version          │  └──────────────┘
└─────────────────────┘
         │
         │ constructs
         ▼
┌─────────────────────┐
│   Backend           │
│                     │
│  - id (UUID)        │
│  - name             │
│  - url              │
│  - backend_type     │
│  - discovery_source │
│    = MDNS           │
│  - metadata         │
│    {mdns_instance}  │
└─────────────────────┘
```

---

## State Transitions

### Service Discovery Lifecycle

```
                    ServiceResolved
  [Not Discovered] ──────────────────▶ [Registered]
                                         │
                                         │ ServiceRemoved
                                         ▼
                                    [Pending Removal]
                                      (status=Unknown)
                                       │           │
                    ServiceResolved     │           │ grace period
                    (reappears)         │           │ expires
                                       │           │
                    ┌──────────────────┘           ▼
                    ▼                        [Removed from
                [Registered]                  Registry]
                (removal cancelled)
```

### Backend Status During Discovery

| Event | Backend Status | Action |
|-------|---------------|--------|
| `ServiceFound` (new) | `Unknown` | Added to registry; health checker will verify |
| `ServiceFound` (reappears) | Unchanged | Removed from `pending_removal` |
| `ServiceRemoved` | Set to `Unknown` | Added to `pending_removal` with timestamp |
| Grace period expired | N/A | Backend removed from registry entirely |

---

## Validation & Constraints

### Service-to-Backend Conversion

**Rule**: A `DiscoveryEvent::ServiceFound` must have at least one IP address to produce a `Backend`.

**Implementation**:
```rust
fn service_event_to_backend(event: &DiscoveryEvent) -> Option<Backend> {
    // Returns None if addresses.is_empty()
    if addresses.is_empty() { return None; }
    // ...
}
```

### IP Address Selection

**Rule**: IPv4 addresses are preferred over IPv6 for URL construction.

**Implementation**: `select_best_ip()` iterates addresses, returns first IPv4 or falls back to first IPv6. IPv6 addresses are wrapped in brackets (`[::1]`).

### URL Deduplication

**Rule**: A discovered backend is skipped if a backend with the same URL already exists in the registry (regardless of discovery source).

**Implementation**: `registry.has_backend_url(&backend.url)` is checked before `add_backend()`. This ensures static configuration takes precedence over mDNS discovery.

### Service Type Normalization

**Rule**: Service types must end with a trailing dot for `mdns-sd` compatibility.

**Implementation**: `format!("{}.", service_type)` appended if not already present.

### TXT Record Type Parsing

**Rule**: Backend type from TXT `type` field is case-insensitive. Recognized values: `ollama`, `vllm`, `llamacpp`/`llama.cpp`, `exo`, `openai`. Unrecognized values map to `Generic`.

---

## Thread Safety

**Requirement**: Discovery runs as a background task alongside health checker, API server, and cleanup task.

**Implementation**:
- `MdnsDiscovery` is moved into a single tokio task (no concurrent access to struct)
- `pending_removal` is `Arc<Mutex<HashMap>>` shared between event loop and cleanup task
- `Registry` is `Arc<Registry>` with `DashMap` internals (lock-free concurrent access)
- `CancellationToken` is `Clone + Send + Sync` for cross-task shutdown signaling
- Cleanup task spawned separately, polls every 10 seconds

---

## Performance Characteristics

| Operation | Target Latency | Implementation |
|-----------|----------------|----------------|
| Handle ServiceFound event | < 1ms | HashMap lookup + Registry insert |
| Handle ServiceRemoved event | < 1ms | Registry lookup + Mutex insert |
| TXT record parsing | < 10µs | String matching, no allocation beyond result |
| IP address selection | < 1µs | Linear scan of small vec (typically 1-2 addresses) |
| URL deduplication check | < 10µs | DashMap lookup in Registry |
| Grace period cleanup | < 100µs | Iterate pending_removal HashMap (typically < 10 entries) |
| Event loop polling | 100ms intervals | `tokio::time::sleep` between `try_recv()` polls |
| Cleanup task interval | 10 seconds | `tokio::time::interval` |

**Memory Overhead**: ~1KB per discovered backend (Backend struct + metadata HashMap + pending_removal entry).

---

## Testing Strategy

### Unit Tests

1. **Service-to-backend conversion**: Verify URL construction, backend type inference, name extraction, IPv4/IPv6 handling, empty address rejection
2. **TXT record parsing**: Empty records, `type` field (case-insensitive), `api_path` override, unknown keys ignored, default API paths per backend type
3. **IP selection**: IPv4 preferred, IPv6-only fallback, bracket formatting for IPv6

### Integration Tests

1. **Service found**: Backend added to registry with correct URL, type, and `DiscoverySource::MDNS`
2. **URL deduplication**: Static backend at same URL prevents duplicate registration
3. **Service removed**: Backend status set to Unknown, added to pending removal
4. **Grace period expiry**: Backend removed after configured grace period
5. **Service reappearance**: Pending removal cancelled, backend preserved
6. **Disabled discovery**: Returns immediately without creating daemon
7. **Cancellation token**: Discovery shuts down cleanly on cancellation

---

## Future Extensions

### Not in Scope

1. **Service advertisement**: Nexus does not advertise itself via mDNS
2. **Cross-subnet discovery**: No mDNS proxy support
3. **Dynamic service type configuration**: Requires restart to change service types
4. **Model hints from TXT records**: `models=` TXT field parsed but not used for pre-registration
