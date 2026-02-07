# mDNS Discovery - Code Walkthrough

**Feature**: F05 - mDNS Discovery  
**Audience**: Junior developers joining the project  
**Last Updated**: 2026-02-07

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: events.rs - Discovery Event Types](#file-1-eventsrs---discovery-event-types)
4. [File 2: parser.rs - TXT Record Parsing](#file-2-parserrs---txt-record-parsing)
5. [File 3: mod.rs - The Discovery Engine](#file-3-modrs---the-discovery-engine)
6. [Registry Extensions](#registry-extensions)
7. [CLI Integration](#cli-integration)
8. [Understanding the Tests](#understanding-the-tests)
9. [Discovery Flow Diagram](#discovery-flow-diagram)
10. [Key Patterns](#key-patterns)

---

## The Big Picture

mDNS Discovery automatically finds LLM backends on your local network. When you start Ollama on another computer, Nexus automatically detects it and adds it to the registry - **zero configuration required**.

### What It Does

1. **Listens for mDNS broadcasts** - Services like Ollama advertise themselves on the network
2. **Parses service metadata** - Extracts backend type, port, and capabilities from TXT records
3. **Registers backends automatically** - Adds discovered services to the Registry
4. **Handles disappearing services** - Uses a grace period to avoid flapping

### How It Fits in Nexus

```
┌─────────────────────────────────────────────────────────────────────┐
│                             Nexus                                   │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │              mDNS Discovery (you are here!)                  │   │
│  │  ┌───────────┐  ┌───────────┐  ┌───────────┐                │   │
│  │  │  Browse   │  │  Parse    │  │  Register │                │   │
│  │  │  Network  │──│  Service  │──│  Backend  │                │   │
│  │  └───────────┘  └───────────┘  └───────────┘                │   │
│  └────────────────────────────────────────────────────────────────┘│
│                              │                                     │
│                              ▼                                     │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │                    Backend Registry                          │  │
│  │            (Now includes auto-discovered backends!)          │  │
│  └─────────────────────────────────────────────────────────────┘  │
│                              │                                     │
│            ┌─────────────────┼─────────────────┐                  │
│            ▼                 ▼                 ▼                  │
│       ┌────────┐       ┌────────┐        ┌────────┐              │
│       │ Ollama │       │  vLLM  │        │  Exo   │              │
│       │ (mDNS) │       │(static)│        │ (mDNS) │              │
│       └────────┘       └────────┘        └────────┘              │
└─────────────────────────────────────────────────────────────────────┘
```

### What is mDNS?

mDNS (Multicast DNS) is a protocol that lets devices find each other on a local network without a central DNS server. It's the same technology that lets you find printers or AirPlay devices automatically.

- **Ollama** advertises itself as `_ollama._tcp.local`
- **Generic LLM services** can advertise as `_llm._tcp.local`

> **Note on trailing dots**: The mDNS protocol uses trailing dots in service names (`_ollama._tcp.local.`), but Nexus automatically normalizes service types configured without the trailing dot. You can use either format in your configuration.

---

## File Structure

```
src/discovery/
├── mod.rs          # Main discovery engine (MdnsDiscovery struct)
├── events.rs       # Event types (ServiceFound, ServiceRemoved)
└── parser.rs       # TXT record parsing utilities

src/registry/
└── mod.rs          # Extended with mDNS-specific methods

src/config/
└── discovery.rs    # DiscoveryConfig (enabled, service_types, grace_period)

src/cli/
└── serve.rs        # Starts discovery when server starts
```

---

## File 1: events.rs - Discovery Event Types

This file defines the **events** that flow through the discovery system. It's very simple - just 30 lines!

### The DiscoveryEvent Enum

```rust
/// Events emitted during mDNS service discovery
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    /// A new service was discovered
    ServiceFound {
        instance: String,           // "my-ollama._ollama._tcp.local"
        service_type: String,       // "_ollama._tcp.local"
        addresses: Vec<IpAddr>,     // [192.168.1.10, ::1]
        port: u16,                  // 11434
        txt_records: HashMap<String, String>,  // {"version": "0.1.0"}
    },
    /// A previously discovered service was removed
    ServiceRemoved {
        instance: String,
        service_type: String,
    },
}
```

**Key Points:**

- **`instance`** - The full mDNS name that uniquely identifies this service
- **`addresses`** - Can have multiple IPs (both IPv4 and IPv6)
- **`txt_records`** - Metadata the service advertises about itself
- We use `Clone` so events can be passed around without ownership issues

### Why Two Events?

Services on a network can:
1. **Appear** - When someone starts Ollama
2. **Disappear** - When they shut it down or network changes

We need to handle both cases gracefully.

---

## File 2: parser.rs - TXT Record Parsing

This file extracts **meaningful information** from mDNS TXT records. Services can advertise metadata like their type, API path, and version.

### ParsedService Struct

```rust
/// Parsed service information from mDNS TXT records
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedService {
    pub backend_type: BackendType,  // Ollama, VLLM, Generic, etc.
    pub api_path: String,           // "" for Ollama, "/v1" for others
    pub version: Option<String>,    // "1.0.0" if advertised
}
```

### The parse_txt_records Function

```rust
pub fn parse_txt_records(txt: &HashMap<String, String>, service_type: &str) -> ParsedService {
    // 1. Try to get type from TXT record first
    let backend_type = if let Some(type_str) = txt.get("type") {
        parse_backend_type(type_str)
    } else {
        // 2. Fall back to inferring from service type
        infer_type_from_service_type(service_type)
    };

    // 3. Extract API path (or use sensible default)
    let api_path = txt
        .get("api_path")
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            match backend_type {
                BackendType::Ollama => String::new(),  // Ollama uses root
                _ => "/v1".to_string(),                // Others use /v1
            }
        });

    // 4. Extract version if present
    let version = txt.get("version").map(|s| s.to_string());

    ParsedService { backend_type, api_path, version }
}
```

**What's happening:**

1. First, look for `type=ollama` in TXT records
2. If not found, guess from service type (`_ollama._tcp.local` → Ollama)
3. Get the API path (different backends use different paths)
4. Grab version if advertised

### Type Inference

```rust
fn infer_type_from_service_type(service_type: &str) -> BackendType {
    if service_type.contains("_ollama.") {
        BackendType::Ollama
    } else {
        BackendType::Generic
    }
}
```

**Why this matters:** Ollama advertises `_ollama._tcp.local` but doesn't include a `type` TXT record. We infer the type from the service name itself.

---

## File 3: mod.rs - The Discovery Engine

This is the **heart of the feature** at ~400 lines. It orchestrates the entire discovery process.

### The MdnsDiscovery Struct

```rust
pub struct MdnsDiscovery {
    config: DiscoveryConfig,                              // Settings
    registry: Arc<Registry>,                              // Where to add backends
    pending_removal: Arc<Mutex<HashMap<String, Instant>>>,// Grace period tracker
}
```

**Key Fields:**

- **`config`** - Contains `enabled`, `service_types`, `grace_period_seconds`
- **`registry`** - Shared reference to the backend registry (Arc = thread-safe)
- **`pending_removal`** - Tracks services that disappeared (for grace period)

### Starting Discovery

```rust
impl MdnsDiscovery {
    pub fn start(self, cancel_token: CancellationToken) -> JoinHandle<()> {
        tokio::spawn(async move {
            if !self.config.enabled {
                tracing::info!("mDNS discovery disabled");
                return;
            }

            self.run(cancel_token).await;
        })
    }
}
```

**What's happening:**

1. `tokio::spawn` creates a background task (doesn't block the main thread)
2. If discovery is disabled in config, exit immediately
3. Otherwise, run the main discovery loop

### The Main Discovery Loop

```rust
async fn run(self, cancel_token: CancellationToken) {
    // 1. Create the mDNS daemon
    let daemon = match mdns_sd::ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("mDNS unavailable, discovery disabled: {}", e);
            return;  // Graceful fallback!
        }
    };

    // 2. Browse for each service type
    let mut receivers = Vec::new();
    for service_type in &self.config.service_types {
        match daemon.browse(service_type) {
            Ok(receiver) => {
                receivers.push((service_type.clone(), receiver));
            }
            Err(e) => {
                tracing::error!("Failed to browse for {}: {}", service_type, e);
            }
        }
    }

    // 3. Main event loop
    loop {
        if cancel_token.is_cancelled() {
            break;  // Graceful shutdown
        }

        for (_, receiver) in &receivers {
            if let Ok(event) = receiver.try_recv() {
                self.handle_mdns_event(event).await;
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    daemon.shutdown().ok();
}
```

**Key Concepts:**

- **Graceful fallback**: If mDNS isn't available (Docker, WSL), we log a warning and continue without crashing
- **Multiple service types**: We browse for both `_ollama._tcp.local` and `_llm._tcp.local`
- **Non-blocking polling**: `try_recv()` doesn't block if no events
- **Cancellation token**: Allows clean shutdown when the server stops

### Converting mDNS Events to Our Events

```rust
async fn handle_mdns_event(&self, event: mdns_sd::ServiceEvent) {
    match event {
        mdns_sd::ServiceEvent::ServiceResolved(info) => {
            // Extract addresses (the mdns_sd crate gives us IpAddr directly)
            let addresses: Vec<IpAddr> = info.get_addresses()
                .iter().copied().collect();

            // Extract TXT records into a HashMap
            let txt_records: HashMap<String, String> = info
                .get_properties()
                .iter()
                .map(|prop| (prop.key().to_string(), prop.val_str().to_string()))
                .collect();

            // Create our internal event type
            let discovery_event = DiscoveryEvent::ServiceFound {
                instance: info.get_fullname().to_string(),
                service_type: info.get_type().to_string(),
                addresses,
                port: info.get_port(),
                txt_records,
            };

            self.handle_event(discovery_event).await;
        }
        mdns_sd::ServiceEvent::ServiceRemoved(_, fullname) => {
            let discovery_event = DiscoveryEvent::ServiceRemoved {
                instance: fullname,
                service_type: "".to_string(),
            };
            self.handle_event(discovery_event).await;
        }
        _ => {} // Ignore other events (SearchStarted, etc.)
    }
}
```

**Why convert?** The `mdns_sd` crate has its own event types. We convert to our own `DiscoveryEvent` enum so the rest of our code doesn't depend on the specific mDNS library.

### Handling ServiceFound

```rust
async fn handle_service_found(&self, event: DiscoveryEvent) {
    let DiscoveryEvent::ServiceFound { ref instance, .. } = event else { return };

    // 1. Remove from pending removal (service reappeared!)
    self.pending_removal.lock().await.remove(instance);

    // 2. Convert to Backend
    let Some(backend) = service_event_to_backend(&event) else {
        tracing::warn!("Could not convert service to backend");
        return;
    };

    // 3. Check if URL already exists (manual config takes precedence!)
    if self.registry.has_backend_url(&backend.url) {
        tracing::debug!("Backend URL already exists, skipping");
        return;
    }

    // 4. Add to registry
    match self.registry.add_backend(backend) {
        Ok(()) => {
            tracing::info!(instance = %instance, "Discovered backend via mDNS");
        }
        Err(e) => {
            tracing::error!("Failed to add discovered backend: {}", e);
        }
    }
}
```

**Important design decisions:**

1. **Reappearing services**: If a service comes back, cancel its pending removal
2. **Manual precedence**: If you've configured a backend in `nexus.toml`, mDNS won't override it
3. **Logging**: INFO for discoveries (users want to know!), DEBUG for skips

### Converting to Backend

```rust
fn service_event_to_backend(event: &DiscoveryEvent) -> Option<Backend> {
    let DiscoveryEvent::ServiceFound {
        instance, service_type, addresses, port, txt_records,
    } = event else { return None };

    // Must have at least one address
    if addresses.is_empty() {
        return None;
    }

    // Parse TXT records for metadata
    let parsed = parse_txt_records(txt_records, service_type);

    // Select best IP (prefer IPv4)
    let selected_ip = select_best_ip(addresses);

    // Build URL: http://192.168.1.10:11434/v1
    let url = build_url(selected_ip, *port, &parsed.api_path);

    // Generate human-readable name
    let name = extract_name_from_instance(instance);

    Some(Backend::new(
        uuid::Uuid::new_v4().to_string(),  // New unique ID
        name,
        url,
        parsed.backend_type,
        vec![],                            // Models discovered by health check
        DiscoverySource::MDNS,            // Mark as discovered
        /* metadata with mdns_instance */
    ))
}
```

### IP Address Selection

```rust
fn select_best_ip(addresses: &[IpAddr]) -> IpAddr {
    addresses
        .iter()
        .find(|ip| ip.is_ipv4())     // Prefer IPv4
        .or_else(|| addresses.first()) // Fall back to first (IPv6)
        .copied()
        .unwrap()
}
```

**Why prefer IPv4?** It's more compatible. IPv6 works but requires bracket notation in URLs (`http://[::1]:11434`).

### URL Building

```rust
fn build_url(ip: IpAddr, port: u16, api_path: &str) -> String {
    let host = match ip {
        IpAddr::V4(addr) => addr.to_string(),      // "192.168.1.10"
        IpAddr::V6(addr) => format!("[{}]", addr), // "[::1]"
    };

    if api_path.is_empty() {
        format!("http://{}:{}", host, port)      // http://192.168.1.10:11434
    } else {
        format!("http://{}:{}{}", host, port, api_path)  // http://192.168.1.10:8000/v1
    }
}
```

### Grace Period Handling

When a service disappears, we don't remove it immediately. Network glitches happen!

```rust
async fn handle_service_removed(&self, instance: &str) {
    // 1. Find backend
    let Some(id) = self.registry.find_by_mdns_instance(instance) else {
        return; // Wasn't in registry anyway
    };

    // 2. Set status to Unknown (health check will verify)
    self.registry.update_status(&id, BackendStatus::Unknown, Some("Disappeared from mDNS"));

    // 3. Start grace period timer
    self.pending_removal.lock().await.insert(instance.to_string(), Instant::now());

    tracing::warn!(instance, grace_period = 60, "Backend disappeared, starting grace period");
}
```

### Cleanup Task

Runs every 10 seconds to remove services that didn't come back:

```rust
async fn cleanup_stale_backends(&self) {
    let grace_period = Duration::from_secs(self.config.grace_period_seconds); // 60s
    let now = Instant::now();

    let mut pending = self.pending_removal.lock().await;

    // Find expired services
    let expired: Vec<String> = pending
        .iter()
        .filter(|(_, &time)| now.duration_since(time) > grace_period)
        .map(|(instance, _)| instance.clone())
        .collect();

    // Remove them
    for instance in expired {
        pending.remove(&instance);

        if let Some(id) = self.registry.find_by_mdns_instance(&instance) {
            self.registry.remove_backend(&id)?;
            tracing::info!(instance, "Removed stale backend after grace period");
        }
    }
}
```

---

## Registry Extensions

We added three new methods to the Registry for mDNS support:

```rust
impl Registry {
    /// Check if a backend with this URL already exists
    pub fn has_backend_url(&self, url: &str) -> bool {
        let normalized = url.trim_end_matches('/');
        self.backends.iter().any(|entry| 
            entry.url.trim_end_matches('/') == normalized
        )
    }

    /// Store mDNS instance name for later lookup
    pub fn set_mdns_instance(&self, id: &str, instance: &str) -> Result<()> {
        let mut backend = self.backends.get_mut(id)?;
        backend.metadata.insert("mdns_instance".to_string(), instance.to_string());
        Ok(())
    }

    /// Find backend by mDNS instance name
    pub fn find_by_mdns_instance(&self, instance: &str) -> Option<String> {
        self.backends.iter()
            .find(|entry| entry.metadata.get("mdns_instance") == Some(&instance.to_string()))
            .map(|entry| entry.id.clone())
    }
}
```

**Why URL normalization?** `http://localhost:11434` and `http://localhost:11434/` should be treated as the same URL.

---

## CLI Integration

In `src/cli/serve.rs`, we start discovery when the server starts:

```rust
pub async fn run_serve(args: ServeArgs) -> Result<()> {
    // ... setup registry, config, etc. ...

    // Start mDNS discovery if enabled
    let discovery_handle = if config.discovery.enabled && !args.no_discovery {
        let discovery = MdnsDiscovery::new(
            config.discovery.clone(),
            registry.clone(),
        );
        Some(discovery.start(cancel_token.clone()))
    } else {
        tracing::info!("mDNS discovery disabled");
        None
    };

    // ... start server ...

    // On shutdown, wait for discovery to stop
    if let Some(handle) = discovery_handle {
        handle.await?;
    }
}
```

The `--no-discovery` flag lets users disable it even if enabled in config.

---

## Understanding the Tests

### Parser Tests (7 tests)

```rust
#[test]
fn test_parse_txt_empty() {
    let txt = HashMap::new();
    let parsed = parse_txt_records(&txt, "_ollama._tcp.local");
    assert_eq!(parsed.backend_type, BackendType::Ollama);  // Inferred!
    assert_eq!(parsed.api_path, "");  // Ollama uses root
}

#[test]
fn test_parse_txt_type_case_insensitive() {
    let mut txt = HashMap::new();
    txt.insert("type".to_string(), "OLLAMA".to_string());  // Uppercase
    let parsed = parse_txt_records(&txt, "_llm._tcp.local");
    assert_eq!(parsed.backend_type, BackendType::Ollama);  // Still works!
}
```

### Service Conversion Tests (7 tests)

```rust
#[test]
fn test_service_to_backend_prefers_ipv4() {
    let event = DiscoveryEvent::ServiceFound {
        addresses: vec![
            IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)),  // IPv6 first
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),              // IPv4 second
        ],
        // ...
    };
    let backend = service_event_to_backend(&event).unwrap();
    assert!(backend.url.contains("192.168.1.10"));  // IPv4 was chosen!
}

#[test]
fn test_service_to_backend_ipv6_only() {
    let event = DiscoveryEvent::ServiceFound {
        addresses: vec![IpAddr::V6(Ipv6Addr::LOCALHOST)],  // Only IPv6
        port: 11434,
        // ...
    };
    let backend = service_event_to_backend(&event).unwrap();
    assert!(backend.url.contains("[::1]"));  // Proper bracket notation!
}
```

### Handler Tests (8 tests)

```rust
#[tokio::test]
async fn test_handle_service_found_skips_existing_url() {
    let registry = Arc::new(Registry::new());
    
    // Pre-add a static backend at this URL
    let static_backend = create_backend_with_url("http://192.168.1.10:11434");
    registry.add_backend(static_backend).unwrap();
    
    let discovery = create_test_discovery(registry.clone());
    
    // Try to discover same URL
    discovery.handle_event(create_found_event_at_ip("192.168.1.10")).await;
    
    // Should still be just 1 backend (static preserved!)
    assert_eq!(registry.backend_count(), 1);
}

#[tokio::test]
async fn test_grace_period_expiry_removes_backend() {
    let registry = Arc::new(Registry::new());
    let config = DiscoveryConfig {
        grace_period_seconds: 1,  // Only 1 second for testing
        ..Default::default()
    };
    let discovery = MdnsDiscovery::new(config, registry.clone());
    
    // Discover and then remove
    discovery.handle_event(create_found_event("test-service")).await;
    discovery.handle_event(create_removed_event("test-service")).await;
    
    // Wait for grace period
    tokio::time::sleep(Duration::from_secs(2)).await;
    discovery.cleanup_stale_backends().await;
    
    // Should be gone now
    assert!(registry.find_by_mdns_instance("test-service").is_none());
}
```

---

## Discovery Flow Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Ollama starts on 192.168.1.10                    │
│                Broadcasts: _ollama._tcp.local, port 11434           │
└────────────────────────────────┬────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     mdns-sd Daemon Receives                         │
│                     ServiceEvent::ServiceResolved                   │
└────────────────────────────────┬────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     handle_mdns_event()                             │
│  • Extract addresses: [192.168.1.10]                               │
│  • Extract TXT records: {version: "0.1.0"}                         │
│  • Create DiscoveryEvent::ServiceFound                             │
└────────────────────────────────┬────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     handle_service_found()                          │
│  • parse_txt_records() → BackendType::Ollama, api_path: ""         │
│  • select_best_ip() → 192.168.1.10                                 │
│  • build_url() → http://192.168.1.10:11434                         │
│  • has_backend_url()? → No                                         │
│  • registry.add_backend() ✓                                        │
└────────────────────────────────┬────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     Registry Now Contains                           │
│  • local-ollama (static from config)                               │
│  • ollama-laptop (discovered via mDNS) ← NEW!                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Key Patterns

### Pattern 1: Graceful Fallback

```rust
let daemon = match mdns_sd::ServiceDaemon::new() {
    Ok(d) => d,
    Err(e) => {
        tracing::warn!("mDNS unavailable: {}", e);
        return;  // Don't crash! Just continue without discovery.
    }
};
```

**Why?** mDNS doesn't work in all environments (Docker, WSL). Users shouldn't be forced to disable it manually.

### Pattern 2: Manual Config Takes Precedence

```rust
if self.registry.has_backend_url(&backend.url) {
    tracing::debug!("Backend URL already exists, skipping");
    return;
}
```

**Why?** If you've manually configured a backend with specific settings (priority, name), you don't want mDNS to create a duplicate or override your settings.

### Pattern 3: Grace Period for Stability

```rust
// On removal: Don't delete immediately, start timer
self.pending_removal.insert(instance, Instant::now());

// Every 10 seconds: Check if grace period expired
if now.duration_since(removal_time) > grace_period {
    self.registry.remove_backend(&id);
}
```

**Why?** Network glitches happen. A service might disappear for a few seconds and come back. The grace period (60s default) prevents "flapping" where backends are constantly added and removed.

### Pattern 4: Async Mutex for Shared State

```rust
pending_removal: Arc<Mutex<HashMap<String, Instant>>>

// Usage:
self.pending_removal.lock().await.insert(instance, Instant::now());
```

**Why `Arc<Mutex<T>>`?**
- `Arc` lets multiple tasks share the HashMap
- `Mutex` ensures only one task modifies it at a time
- We use `tokio::sync::Mutex` (not `std::sync::Mutex`) because we `.await` while holding the lock

### Pattern 5: Type Inference Fallback

```rust
let backend_type = txt.get("type")
    .map(parse_backend_type)
    .unwrap_or_else(|| infer_type_from_service_type(service_type));
```

**Why?** Not all services advertise their type in TXT records. Ollama doesn't! So we fall back to inferring from the service name.

---

## Summary

| File | Purpose | Lines |
|------|---------|-------|
| `events.rs` | Define ServiceFound/ServiceRemoved events | 85 |
| `parser.rs` | Parse TXT records into backend metadata | 124 |
| `mod.rs` | Main discovery engine with graceful handling | ~450 |

**Total**: ~660 lines for complete mDNS discovery with:
- Automatic backend registration
- Grace period handling
- Manual config precedence
- Cross-platform support
- 29 tests for edge cases

The feature enables **zero-configuration** deployment in homelabs where multiple LLM servers can be automatically discovered and utilized by Nexus.
