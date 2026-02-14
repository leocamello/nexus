# Research: mDNS Discovery (F05)

**Date**: 2026-02-08
**Status**: Implemented (PR #70)

This document captures the technical decisions made during F05 implementation, alternatives considered, and rationale for each choice.

## Research Questions & Findings

### 1. mDNS Crate Selection

**Question**: Which Rust crate should we use for mDNS service discovery?

**Decision**: Use `mdns-sd` (v0.11) for all mDNS operations.

**Rationale**:
- Pure Rust implementation — no C dependencies, preserves single-binary goal
- Provides both browsing (discovery) and registration APIs
- Built-in `ServiceDaemon` with async-friendly `Receiver` for events
- Cross-platform support (Linux, macOS, Windows)
- Active maintenance and reasonable API surface

**Alternatives Considered**:
- **`libmdns`**: Rejected — lower-level API, requires manual DNS record construction, less ergonomic for service browsing
- **`zeroconf`**: Rejected — wraps system libraries (Avahi on Linux, Bonjour on macOS), introducing platform-specific C dependencies that break single-binary builds
- **`mdns` crate**: Rejected — focused on queries only, no service browsing abstraction, would require building the browse/resolve flow manually
- **Avahi D-Bus bindings**: Rejected — Linux-only, requires Avahi daemon running, not cross-platform

**References**:
- https://docs.rs/mdns-sd/latest/mdns_sd/
- https://crates.io/crates/mdns-sd

---

### 2. Service Type Naming Convention

**Question**: What mDNS service types should Nexus browse for?

**Decision**: Browse `_ollama._tcp.local` and `_llm._tcp.local` by default, configurable via `discovery.service_types`.

**Rationale**:
- `_ollama._tcp.local` matches Ollama's native mDNS advertisement
- `_llm._tcp.local` provides a generic service type for non-Ollama backends (vLLM, llama.cpp, exo)
- Configurable list allows users to add custom service types without code changes
- Default covers the two most common discovery scenarios

**Implementation**:
```rust
impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            service_types: vec![
                "_ollama._tcp.local".to_string(),
                "_llm._tcp.local".to_string(),
            ],
            grace_period_seconds: 60,
        }
    }
}
```

**Alternatives Considered**:
- **Single generic type (`_llm._tcp`)**: Rejected — would miss Ollama instances that only advertise `_ollama._tcp`
- **IANA-registered type**: Rejected — registration process is slow and the LLM ecosystem is evolving too fast for formal registration
- **Hard-coded list**: Rejected — limits extensibility; users with custom backends need to add their own types

---

### 3. Service Type Trailing Dot Normalization

**Question**: How do we handle the mdns-sd requirement for trailing dots on service types?

**Decision**: Automatically normalize service types by appending a trailing dot if missing.

**Rationale**:
- `mdns-sd` requires service types to end with `.` (e.g., `_ollama._tcp.local.`)
- Users naturally write `_ollama._tcp.local` without the trailing dot
- Auto-normalization prevents confusing "browse failed" errors
- Zero-config philosophy: don't require users to know protocol internals

**Implementation**:
```rust
let normalized = if service_type.ends_with('.') {
    service_type.clone()
} else {
    format!("{}.", service_type)
};
```

**Alternatives Considered**:
- **Require trailing dot in config**: Rejected — violates zero-config philosophy, creates a documentation burden
- **Validate and error**: Rejected — failing on a missing dot is unfriendly; we can fix it automatically

---

### 4. Grace Period Design for Service Removal

**Question**: How should Nexus handle services that disappear from mDNS?

**Decision**: Two-phase removal with configurable grace period (default: 60s). On `ServiceRemoved`, mark backend as `Unknown` and add to `pending_removal` map. A cleanup task removes backends after the grace period expires.

**Rationale**:
- mDNS services can briefly disappear due to network glitches, not actual backend shutdowns
- Immediate removal would cause unnecessary routing disruptions
- Grace period allows transient disappearances to self-heal via `ServiceFound` re-events
- `Unknown` status prevents routing to the backend while keeping it in the registry
- Background cleanup task runs every 10 seconds to check for expired entries

**Implementation**:
```rust
// On ServiceRemoved: mark Unknown + start grace timer
async fn handle_service_removed(&self, instance: &str) {
    self.registry.update_status(&id, BackendStatus::Unknown, ...);
    self.pending_removal.lock().await.insert(instance.to_string(), Instant::now());
}

// On ServiceFound: cancel pending removal
async fn handle_service_found(&self, event: DiscoveryEvent) {
    self.pending_removal.lock().await.remove(instance);
    // ... register backend
}

// Periodic cleanup: remove expired entries
async fn cleanup_stale_backends(&self) {
    let grace_period = Duration::from_secs(self.config.grace_period_seconds);
    // Remove backends where now - removal_time > grace_period
}
```

**Alternatives Considered**:
- **Immediate removal**: Rejected — too aggressive; network blips would cause unnecessary churn
- **Never remove (rely on health checks)**: Rejected — would accumulate stale entries indefinitely, wasting health check cycles
- **Exponential backoff before removal**: Rejected — adds complexity without proportional benefit; fixed grace period is simpler and predictable

---

### 5. TXT Record Format for Backend Metadata

**Question**: What metadata should backends advertise in mDNS TXT records?

**Decision**: Three optional TXT record keys: `type` (backend type), `api_path` (API base path), and `version` (backend version).

**Rationale**:
- `type` enables correct `BackendType` classification without probing the backend
- `api_path` handles backends with non-standard API paths (e.g., `/v1` for vLLM vs root for Ollama)
- `version` is informational metadata stored in the registry
- All fields optional — service type inference provides fallback for `type`, defaults handle `api_path`
- mDNS TXT records are key-value pairs with 255-byte limit per entry, well within our needs

**Implementation**:
```rust
pub fn parse_txt_records(txt: &HashMap<String, String>, service_type: &str) -> ParsedService {
    let backend_type = if let Some(type_str) = txt.get("type") {
        parse_backend_type(type_str)  // "ollama", "vllm", "llamacpp", etc.
    } else {
        infer_type_from_service_type(service_type)  // "_ollama._tcp" → Ollama
    };
    let api_path = txt.get("api_path").unwrap_or_else(|| /* defaults by type */);
    // ...
}
```

**Alternatives Considered**:
- **Structured JSON in TXT records**: Rejected — TXT records have 255-byte limit per entry; JSON is verbose and fragile to parse
- **Separate DNS SRV record fields**: Rejected — SRV records only carry priority/weight/port/target; no room for custom metadata
- **Probe-based detection**: Rejected — would require HTTP requests during discovery, adding latency and complexity to the hot path

---

### 6. Background Task Architecture

**Question**: How should the mDNS discovery loop run alongside other Nexus components?

**Decision**: `MdnsDiscovery::start()` spawns a `tokio::spawn` task that runs the main event loop, plus a nested cleanup task. Both respond to a shared `CancellationToken` for graceful shutdown.

**Rationale**:
- `tokio::spawn` integrates naturally with Nexus's existing async runtime
- `CancellationToken` pattern matches the health checker's shutdown mechanism (established in F03)
- Two nested tasks: main loop (polls mdns-sd receivers every 100ms) and cleanup task (runs every 10s)
- Non-blocking `try_recv()` on mdns-sd receivers prevents stalling the event loop
- If mDNS daemon creation fails, logs a warning and returns (graceful degradation, not panic)

**Implementation**:
```rust
pub fn start(self, cancel_token: CancellationToken) -> JoinHandle<()> {
    tokio::spawn(async move {
        if !self.config.enabled { return; }
        self.run(cancel_token).await;
    })
}

async fn run(self, cancel_token: CancellationToken) {
    let daemon = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => { tracing::warn!("mDNS unavailable: {}", e); return; }
    };
    // ... browse + event loop with 100ms sleep between polls
}
```

**Alternatives Considered**:
- **Callback-based API**: Rejected — mdns-sd uses channel-based receivers, not callbacks; adapting would add unnecessary wrapper complexity
- **Dedicated thread (std::thread)**: Rejected — wastes an OS thread; tokio tasks are lighter and integrate with the existing runtime
- **Blocking recv() in loop**: Rejected — would prevent cancellation token checks; non-blocking `try_recv()` with sleep allows responsive shutdown

---

### 7. IP Address Selection Strategy

**Question**: When a service advertises multiple IP addresses, which should Nexus use?

**Decision**: Prefer IPv4 over IPv6, falling back to the first available address.

**Rationale**:
- IPv4 addresses work universally across local networks without link-local scope issues
- IPv6 link-local addresses (`fe80::`) require scope ID (`%eth0`) which varies by interface
- Most LLM backends are accessed over IPv4 on local networks
- Simple `find(is_ipv4).or(first)` logic — no complex scoring

**Implementation**:
```rust
fn select_best_ip(addresses: &[IpAddr]) -> IpAddr {
    addresses.iter()
        .find(|ip| ip.is_ipv4())
        .or_else(|| addresses.first())
        .copied()
        .unwrap()
}
```

**Alternatives Considered**:
- **Always use first address**: Rejected — IPv6 link-local addresses cause connectivity issues on some networks
- **Try all addresses with health check**: Rejected — too slow for the discovery path; health checks happen separately
- **User-configurable preference**: Rejected — over-engineering for a rare edge case; IPv4 preference is the right default

---

### 8. Duplicate URL Prevention

**Question**: How do we prevent the same backend from being registered twice (e.g., static config + mDNS discovery)?

**Decision**: Check `registry.has_backend_url()` before adding mDNS-discovered backends. If a URL already exists (from static config or prior discovery), skip silently.

**Rationale**:
- A user might configure a static backend AND have it advertise via mDNS
- Static backends should take precedence (user explicitly configured them)
- URL-based deduplication is simple and deterministic
- DEBUG-level log prevents noise while remaining diagnosable

**Implementation**:
```rust
if self.registry.has_backend_url(&backend.url) {
    tracing::debug!(url = %backend.url, "Backend URL already exists, skipping");
    return;
}
```

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| mDNS unavailable on host | Medium | Graceful fallback: log warning, continue without discovery |
| Network glitches cause false removals | High | Grace period (default 60s) absorbs transient disappearances |
| Service type mismatch | Medium | Configurable service types + automatic trailing dot normalization |
| IPv6 link-local connectivity issues | Low | IPv4 preferred; IPv6 used only as fallback |
| Duplicate registrations | Low | URL-based deduplication before registry insertion |
| Cross-platform mDNS differences | Medium | Pure-Rust mdns-sd avoids OS-specific dependencies |

---

## References

- [mdns-sd crate documentation](https://docs.rs/mdns-sd/latest/mdns_sd/)
- [RFC 6762 - Multicast DNS](https://www.rfc-editor.org/rfc/rfc6762)
- [RFC 6763 - DNS-Based Service Discovery](https://www.rfc-editor.org/rfc/rfc6763)
- [mDNS service type naming conventions](http://www.dns-sd.org/ServiceTypes.html)
- [Nexus LEARNINGS.md - F05 section](../../docs/LEARNINGS.md)
