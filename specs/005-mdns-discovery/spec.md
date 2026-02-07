# Feature Specification: mDNS Discovery

**Feature ID**: F05  
**Feature Branch**: `feature/f05-mdns-discovery`  
**Created**: 2026-02-07  
**Status**: 🚧 In Progress  
**Priority**: P1 (Phase 2)  

## Overview

Automatically discover LLM backends on the local network using mDNS/Bonjour (also known as Zeroconf). This enables zero-configuration deployment where Ollama and other compatible services are detected as they appear on the network.

This is essential for the homelab cluster use case where multiple GPU-powered nodes run exo or Ollama, and Nexus automatically routes requests to available backends without manual configuration.

## Dependencies

- **F02: Backend Registry** - Discovery adds backends to the registry
- **F03: Health Checker** - Triggers immediate health check on discovery

## Technical Stack

| Component | Choice | Rationale |
|-----------|--------|-----------|
| mDNS Library | `mdns-sd` | Cross-platform, async-compatible, actively maintained |
| Runtime | `tokio` | Consistent with rest of Nexus |
| Communication | `tokio::sync::mpsc` | Send discovery events to registry |

## Supported Service Types

| Service Type | Backend Type | Notes |
|--------------|--------------|-------|
| `_ollama._tcp.local` | Ollama | Ollama advertises this by default |
| `_llm._tcp.local` | Generic | Proposed standard for LLM services |

## User Scenarios & Testing

### User Story 1 - Auto-Discover Ollama on Network (Priority: P1)

As a user, I want Nexus to automatically find Ollama instances on my local network so I don't have to manually configure each backend.

**Why this priority**: This is the core value proposition of mDNS discovery - zero configuration.

**Independent Test**: Can be tested with a mock mDNS browser that simulates service events.

**Acceptance Scenarios**:

1. **Given** mDNS discovery is enabled, **When** an Ollama instance advertises `_ollama._tcp.local`, **Then** Nexus adds it to the registry with type Ollama
2. **Given** discovered backend, **When** I check the registry, **Then** the backend has `DiscoverySource::MDNS` and correct URL
3. **Given** multiple Ollama instances on network, **When** they all advertise, **Then** all are discovered and added to registry

---

### User Story 2 - Handle Service Disappearance (Priority: P1)

As a user, I want Nexus to gracefully handle backends that go offline so the system remains stable without manual intervention.

**Why this priority**: Network services can disappear at any time; graceful handling prevents errors.

**Independent Test**: Can be tested by simulating service removal events.

**Acceptance Scenarios**:

1. **Given** a discovered backend, **When** the service disappears from mDNS, **Then** backend status is set to Unknown
2. **Given** a backend marked Unknown, **When** grace period (60s) expires without reappearance, **Then** backend is removed from registry
3. **Given** a backend marked Unknown, **When** service reappears within grace period, **Then** backend status is restored (via health check)

---

### User Story 3 - Parse TXT Records for Metadata (Priority: P2)

As a user with custom LLM services, I want Nexus to read TXT records so it can detect backend type and capabilities.

**Why this priority**: Enhances discovery but not required for basic Ollama support.

**Independent Test**: Can be tested by providing mock TXT records and verifying parsed metadata.

**Acceptance Scenarios**:

1. **Given** TXT record with `type=vllm`, **When** service is discovered, **Then** backend type is set to VLLM
2. **Given** TXT record with `api_path=/v1`, **When** service is discovered, **Then** URL includes the api_path
3. **Given** no TXT records, **When** Ollama service is discovered, **Then** defaults are used (type=Ollama)

---

### User Story 4 - Graceful Fallback When mDNS Unavailable (Priority: P1)

As a user running in Docker or WSL, I want Nexus to work even when mDNS is not available so I can still use static configuration.

**Why this priority**: mDNS may not work in all environments; Nexus must remain functional.

**Independent Test**: Can be tested by simulating mDNS initialization failure.

**Acceptance Scenarios**:

1. **Given** mDNS is unavailable, **When** Nexus starts, **Then** it logs a warning and continues with static backends only
2. **Given** mDNS is disabled in config, **When** Nexus starts, **Then** no mDNS browser is created
3. **Given** mDNS fails mid-operation, **When** error occurs, **Then** existing discovered backends remain in registry

---

### User Story 5 - Manual Config Takes Precedence (Priority: P1)

As a user, I want my manual configuration to override discovered backends so I can customize settings like priority.

**Why this priority**: Users need control over backend behavior.

**Independent Test**: Can be tested by having both static and discovered backends with same URL.

**Acceptance Scenarios**:

1. **Given** static backend at `http://192.168.1.10:11434`, **When** same IP discovered via mDNS, **Then** static config is preserved (no duplicate)
2. **Given** discovered backend, **When** I add manual backend with same URL, **Then** manual replaces discovered
3. **Given** mixed static and discovered backends, **When** I list backends, **Then** I can distinguish by `discovery_source` field

---

## Discovery Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│                         STARTUP                                      │
├─────────────────────────────────────────────────────────────────────┤
│  1. Check if discovery.enabled in config                            │
│  2. Create mDNS ServiceDaemon                                       │
│  3. Browse for each service_type in config                          │
│  4. Spawn background task to handle events                          │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    EVENT: ServiceResolved                           │
├─────────────────────────────────────────────────────────────────────┤
│  1. Extract hostname, IP addresses, port from SRV record            │
│  2. Parse TXT records for metadata (type, api_path, version)        │
│  3. Build backend URL: http://{ip}:{port}{api_path}                 │
│  4. Check if URL already exists in registry (static or discovered)  │
│  5. If new: Add to registry with DiscoverySource::MDNS              │
│  6. Trigger immediate health check                                  │
│  7. Log discovery at INFO level                                     │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    EVENT: ServiceRemoved                            │
├─────────────────────────────────────────────────────────────────────┤
│  1. Find backend by mDNS instance name                              │
│  2. Set status to Unknown                                           │
│  3. Record removal timestamp                                        │
│  4. Start grace period timer (60s default)                          │
│  5. If not seen again: Remove from registry                         │
│  6. Log removal at WARN level                                       │
└─────────────────────────────────────────────────────────────────────┘
```

## TXT Record Parsing

```
# Ollama default (minimal)
version=0.1.0

# Proposed LLM standard (extended)
type=vllm           # Backend type: ollama, vllm, llamacpp, exo, generic
api_path=/v1        # API base path (default: empty for Ollama, /v1 for others)
version=1.0.0       # Service version
models=llama3,qwen  # Comma-separated model hints (optional)
```

## Configuration

```toml
[discovery]
# Enable/disable mDNS discovery
enabled = true

# Service types to browse for
service_types = ["_ollama._tcp.local", "_llm._tcp.local"]

# Seconds to wait before removing disappeared service
grace_period_seconds = 60
```

## API Changes

### Registry Additions

```rust
impl Registry {
    /// Check if a backend with this URL already exists (normalized comparison)
    pub fn has_backend_url(&self, url: &str) -> bool;
    
    /// Store mDNS instance name on a backend for later lookup
    pub fn set_mdns_instance(&self, id: &str, instance: &str) -> Result<(), RegistryError>;
    
    /// Find backend by mDNS instance name (for removal on ServiceRemoved)
    pub fn find_by_mdns_instance(&self, instance: &str) -> Option<String>;
}
```

### Backend Struct Addition

```rust
// Add to existing Backend struct
pub struct Backend {
    // ... existing fields ...
    
    /// mDNS instance name for discovered backends (None for static/manual)
    mdns_instance: Option<String>,
}
```

### New Module: `src/discovery/`

```rust
pub struct MdnsDiscovery {
    config: DiscoveryConfig,
    registry: Arc<Registry>,
    /// Track services pending removal: instance_name -> removal_timestamp
    pending_removal: Arc<Mutex<HashMap<String, Instant>>>,
}

impl MdnsDiscovery {
    pub fn new(config: DiscoveryConfig, registry: Arc<Registry>) -> Self;
    
    /// Start discovery in background task, returns JoinHandle
    /// The task will:
    /// 1. Create mDNS daemon and browse for service types
    /// 2. Handle ServiceFound/ServiceRemoved events
    /// 3. Run cleanup task every 10s to remove stale backends
    pub fn start(self, cancel_token: CancellationToken) -> JoinHandle<()>;
}
```

### Health Check Integration

Discovery does NOT directly call the health checker. Instead:
1. Discovery adds backend to registry with `BackendStatus::Unknown`
2. Health checker's periodic loop (every 30s) will check the new backend
3. For faster initial check, discovery can call `registry.update_status()` to mark as `Unknown`, which the health checker treats as priority

This keeps the components decoupled - discovery only interacts with the registry.

## Edge Cases

| Edge Case | Handling |
|-----------|----------|
| Multiple instances same IP, different ports | Treat as separate backends (unique by URL) |
| Service disappears then reappears | Keep existing backend ID, cancel pending removal |
| mDNS not available (Docker, WSL) | Log warning, continue with static config only |
| Manual config at same URL as discovered | Skip discovery - static config takes precedence (check `has_backend_url` first) |
| IPv6 addresses | Support both; select first IPv4 if present, else first IPv6 |
| Service on loopback (127.x.x.x) | Include it (useful for local testing) |
| Rapid service flapping | Grace period (60s default) prevents rapid add/remove cycles |

## Acceptance Criteria

- [ ] Discovers Ollama instances automatically
- [ ] Discovers `_llm._tcp.local` services
- [ ] Handles service appearing/disappearing
- [ ] Grace period prevents flapping (60s default)
- [ ] Works on macOS, Linux, Windows
- [ ] Graceful fallback if mDNS unavailable
- [ ] Manual config takes precedence over discovered
- [ ] Logs discovery events at appropriate levels
- [ ] Integrates with health checker for immediate checks
- [ ] Cleanup task removes stale backends

## Non-Goals (Out of Scope)

- Service advertisement (Nexus doesn't advertise itself)
- Custom mDNS record types beyond TXT
- mDNS proxy for cross-subnet discovery
- Service prioritization based on TXT records (use manual config)

## Performance Requirements

| Metric | Target |
|--------|--------|
| Discovery latency | < 5s from service advertisement to registry |
| Memory overhead | < 10MB for mDNS browser |
| CPU usage (idle) | < 1% when no services changing |

## Security Considerations

- mDNS is inherently local network only (not routable)
- Discovered backends should be treated with same trust as static
- No authentication via mDNS (use backend-level auth if needed)
- Log source IP of discovered services for audit trail
