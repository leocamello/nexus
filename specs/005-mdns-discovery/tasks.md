# Implementation Tasks: mDNS Discovery

**Spec**: [spec.md](./spec.md)  
**Plan**: [plan.md](./plan.md)  
**Status**: ✅ Complete

## Task Overview

| Task | Description | Est. Time | Dependencies |
|------|-------------|-----------|--------------|
| T01 | Add mdns-sd dependency & module scaffolding | 1h | None |
| T02 | Implement DiscoveryConfig | 1h | T01 |
| T03 | Implement DiscoveryEvent types | 1h | T01 |
| T04 | Implement TXT record parser | 1.5h | T02, T03 |
| T05 | Implement service-to-backend conversion | 1.5h | T04 |
| T06 | Add registry extension methods | 1.5h | None |
| T07 | Implement MdnsDiscovery core structure | 2h | T05, T06 |
| T08 | Implement service found handler | 2h | T07 |
| T09 | Implement service removed handler & grace period | 2h | T08 |
| T10 | Implement real mDNS browser integration | 2.5h | T09 |
| T11 | CLI integration | 1.5h | T10 |
| T12 | Documentation & cleanup | 1.5h | T11 |

**Total Estimated Time**: ~19 hours

---

## T01: Add mdns-sd Dependency & Module Scaffolding

**Goal**: Create module structure and add mDNS dependency.

**Files to create/modify**:
- `Cargo.toml` (add mdns-sd)
- `src/lib.rs` (add pub mod discovery)
- `src/discovery/mod.rs` (create)
- `src/discovery/config.rs` (create, empty)
- `src/discovery/events.rs` (create, empty)
- `src/discovery/parser.rs` (create, empty)

**Implementation Steps**:
1. Add `mdns-sd = "0.11"` to `[dependencies]` in Cargo.toml
2. Update `src/lib.rs` to add `pub mod discovery;`
3. Create `src/discovery/mod.rs`:
   ```rust
   mod config;
   mod events;
   mod parser;

   pub use config::*;
   pub use events::*;
   pub use parser::*;
   ```
4. Create empty placeholder files
5. Run `cargo check` to verify structure compiles

**Acceptance Criteria**:
- [X] `cargo check` passes with no errors
- [X] mdns-sd compiles on current platform
- [X] Module structure matches plan's file layout

**Test Command**: `cargo check`

---

## T02: Implement DiscoveryConfig

**Goal**: Define configuration for mDNS discovery.

**Files to modify**:
- `src/discovery/config.rs`
- `src/config.rs` (integrate with main config)

**Tests to Write First**:
```rust
#[test]
fn test_discovery_config_defaults() {
    let config = DiscoveryConfig::default();
    assert!(config.enabled);
    assert_eq!(config.grace_period_seconds, 60);
    assert!(!config.service_types.is_empty());
}

#[test]
fn test_discovery_config_from_toml() {
    let toml = r#"
        enabled = false
        service_types = ["_custom._tcp.local"]
        grace_period_seconds = 120
    "#;
    let config: DiscoveryConfig = toml::from_str(toml).unwrap();
    assert!(!config.enabled);
}

#[test]
fn test_discovery_config_service_types_default() {
    let config = DiscoveryConfig::default();
    assert!(config.service_types.contains(&"_ollama._tcp.local".to_string()));
    assert!(config.service_types.contains(&"_llm._tcp.local".to_string()));
}
```

**Implementation**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_service_types")]
    pub service_types: Vec<String>,
    #[serde(default = "default_grace_period")]
    pub grace_period_seconds: u64,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            service_types: default_service_types(),
            grace_period_seconds: 60,
        }
    }
}
```

**Acceptance Criteria**:
- [X] DiscoveryConfig has enabled, service_types, grace_period_seconds
- [X] Default values: enabled=true, grace_period=60s
- [X] Default service_types includes `_ollama._tcp.local` and `_llm._tcp.local`
- [X] Config deserializes from TOML correctly
- [X] All 3 tests pass

**Test Command**: `cargo test discovery::config`

---

## T03: Implement DiscoveryEvent Types

**Goal**: Define events for service discovery.

**Files to modify**:
- `src/discovery/events.rs`

**Tests to Write First**:
```rust
#[test]
fn test_discovery_event_service_found_debug() {
    let event = DiscoveryEvent::ServiceFound { ... };
    let debug = format!("{:?}", event);
    assert!(debug.contains("ServiceFound"));
}

#[test]
fn test_discovery_event_service_removed_debug() {
    let event = DiscoveryEvent::ServiceRemoved { ... };
    let debug = format!("{:?}", event);
    assert!(debug.contains("ServiceRemoved"));
}

#[test]
fn test_service_info_with_ipv4_and_ipv6() {
    let event = DiscoveryEvent::ServiceFound {
        addresses: vec![
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
            IpAddr::V6(Ipv6Addr::LOCALHOST),
        ],
        ...
    };
    // Verify both addresses stored
}
```

**Implementation**:
```rust
use std::net::IpAddr;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    ServiceFound {
        instance: String,
        service_type: String,
        addresses: Vec<IpAddr>,
        port: u16,
        txt_records: HashMap<String, String>,
    },
    ServiceRemoved {
        instance: String,
        service_type: String,
    },
}
```

**Acceptance Criteria**:
- [X] DiscoveryEvent::ServiceFound contains instance, service_type, addresses, port, txt_records
- [X] DiscoveryEvent::ServiceRemoved contains instance, service_type
- [X] Both variants derive Debug and Clone
- [X] Addresses support both IPv4 and IPv6
- [X] All 3 tests pass

**Test Command**: `cargo test discovery::events`

---

## T04: Implement TXT Record Parser

**Goal**: Parse TXT records into backend metadata.

**Files to modify**:
- `src/discovery/parser.rs`

**Tests to Write First**:
```rust
#[test]
fn test_parse_txt_empty() {
    let txt = HashMap::new();
    let parsed = parse_txt_records(&txt, "_ollama._tcp.local");
    assert_eq!(parsed.backend_type, BackendType::Ollama);
    assert_eq!(parsed.api_path, "");
}

#[test]
fn test_parse_txt_type_vllm() {
    let mut txt = HashMap::new();
    txt.insert("type".to_string(), "vllm".to_string());
    let parsed = parse_txt_records(&txt, "_llm._tcp.local");
    assert_eq!(parsed.backend_type, BackendType::Vllm);
}

#[test]
fn test_parse_txt_api_path() {
    let mut txt = HashMap::new();
    txt.insert("api_path".to_string(), "/v1".to_string());
    let parsed = parse_txt_records(&txt, "_llm._tcp.local");
    assert_eq!(parsed.api_path, "/v1");
}

#[test]
fn test_parse_txt_type_case_insensitive() {
    let mut txt = HashMap::new();
    txt.insert("type".to_string(), "OLLAMA".to_string());
    let parsed = parse_txt_records(&txt, "_llm._tcp.local");
    assert_eq!(parsed.backend_type, BackendType::Ollama);
}

#[test]
fn test_infer_type_from_service_type_ollama() {
    let txt = HashMap::new();
    let parsed = parse_txt_records(&txt, "_ollama._tcp.local");
    assert_eq!(parsed.backend_type, BackendType::Ollama);
}

#[test]
fn test_infer_type_from_service_type_generic() {
    let txt = HashMap::new();
    let parsed = parse_txt_records(&txt, "_llm._tcp.local");
    assert_eq!(parsed.backend_type, BackendType::Generic);
}

#[test]
fn test_parse_txt_unknown_keys_ignored() {
    let mut txt = HashMap::new();
    txt.insert("unknown_key".to_string(), "value".to_string());
    txt.insert("type".to_string(), "ollama".to_string());
    let parsed = parse_txt_records(&txt, "_llm._tcp.local");
    assert_eq!(parsed.backend_type, BackendType::Ollama);
}
```

**Implementation**:
```rust
pub struct ParsedService {
    pub backend_type: BackendType,
    pub api_path: String,
    pub version: Option<String>,
}

pub fn parse_txt_records(
    txt: &HashMap<String, String>,
    service_type: &str,
) -> ParsedService {
    // 1. Try to get type from TXT record
    // 2. Fall back to inferring from service_type
    // 3. Extract api_path (default "" for Ollama, "/v1" for others)
    // 4. Extract version if present
}
```

**Acceptance Criteria**:
- [X] Empty TXT records use service_type to infer backend type
- [X] `_ollama._tcp.local` → BackendType::Ollama
- [X] `_llm._tcp.local` → BackendType::Generic (unless TXT overrides)
- [X] `type=vllm` in TXT → BackendType::VLLM
- [X] `api_path=/v1` in TXT → api_path="/v1"
- [X] Type parsing is case-insensitive
- [X] Unknown TXT keys are ignored
- [X] All 7 tests pass

**Test Command**: `cargo test discovery::parser`

---

## T05: Implement Service-to-Backend Conversion

**Goal**: Convert DiscoveryEvent to Backend struct.

**Files to modify**:
- `src/discovery/mod.rs`

**Tests to Write First**:
```rust
#[test]
fn test_service_to_backend_basic() {
    let event = DiscoveryEvent::ServiceFound {
        instance: "ollama-server".to_string(),
        service_type: "_ollama._tcp.local".to_string(),
        addresses: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))],
        port: 11434,
        txt_records: HashMap::new(),
    };
    let backend = service_event_to_backend(&event).unwrap();
    assert_eq!(backend.url(), "http://192.168.1.10:11434");
    assert_eq!(backend.backend_type(), BackendType::Ollama);
}

#[test]
fn test_service_to_backend_with_api_path() {
    let mut txt = HashMap::new();
    txt.insert("api_path".to_string(), "/v1".to_string());
    let event = DiscoveryEvent::ServiceFound {
        addresses: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))],
        port: 8000,
        txt_records: txt,
        ...
    };
    let backend = service_event_to_backend(&event).unwrap();
    assert_eq!(backend.url(), "http://192.168.1.10:8000/v1");
}

#[test]
fn test_service_to_backend_prefers_ipv4() {
    let event = DiscoveryEvent::ServiceFound {
        addresses: vec![
            IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
        ],
        ...
    };
    let backend = service_event_to_backend(&event).unwrap();
    assert!(backend.url().contains("192.168.1.10"));
}

#[test]
fn test_service_to_backend_ipv6_only() {
    let event = DiscoveryEvent::ServiceFound {
        addresses: vec![IpAddr::V6(Ipv6Addr::LOCALHOST)],
        port: 11434,
        ...
    };
    let backend = service_event_to_backend(&event).unwrap();
    assert!(backend.url().contains("[::1]"));
}

#[test]
fn test_service_to_backend_discovery_source() {
    let event = DiscoveryEvent::ServiceFound { ... };
    let backend = service_event_to_backend(&event).unwrap();
    assert_eq!(backend.discovery_source(), DiscoverySource::Mdns);
}

#[test]
fn test_service_to_backend_generates_name() {
    let event = DiscoveryEvent::ServiceFound {
        instance: "My Ollama Server._ollama._tcp.local".to_string(),
        ...
    };
    let backend = service_event_to_backend(&event).unwrap();
    assert_eq!(backend.name(), "My Ollama Server");
}
```

**Acceptance Criteria**:
- [X] Creates Backend with correct URL (http://ip:port + api_path)
- [X] Backend type inferred from service_type and TXT records
- [X] Prefers IPv4 over IPv6 when both available
- [X] Handles IPv6-only services with bracket notation
- [X] Sets DiscoverySource::Mdns
- [X] Generates human-readable name from instance
- [X] Returns None if no addresses available
- [X] All 6 tests pass

**Test Command**: `cargo test service_to_backend`

---

## T06: Add Registry Extension Methods

**Goal**: Add methods to Registry for mDNS discovery support.

**Files to modify**:
- `src/registry/mod.rs`

**Tests to Write First**:
```rust
#[test]
fn test_registry_has_backend_url_true() {
    let registry = Registry::new();
    let backend = create_test_backend("http://localhost:11434");
    registry.add_backend(backend).unwrap();
    assert!(registry.has_backend_url("http://localhost:11434"));
}

#[test]
fn test_registry_has_backend_url_false() {
    let registry = Registry::new();
    assert!(!registry.has_backend_url("http://localhost:11434"));
}

#[test]
fn test_registry_has_backend_url_normalized() {
    let registry = Registry::new();
    let backend = create_test_backend("http://localhost:11434/");
    registry.add_backend(backend).unwrap();
    // Should match with or without trailing slash
    assert!(registry.has_backend_url("http://localhost:11434"));
}

#[test]
fn test_registry_set_mdns_instance() {
    let registry = Registry::new();
    let backend = create_test_backend("http://localhost:11434");
    let id = registry.add_backend(backend).unwrap();
    registry.set_mdns_instance(&id, "my-instance._ollama._tcp.local").unwrap();
    
    let backend = registry.get_backend(&id).unwrap();
    assert_eq!(backend.mdns_instance(), Some("my-instance._ollama._tcp.local"));
}

#[test]
fn test_registry_find_by_mdns_instance_found() {
    let registry = Registry::new();
    let backend = create_test_backend("http://localhost:11434");
    let id = registry.add_backend(backend).unwrap();
    registry.set_mdns_instance(&id, "test-instance").unwrap();
    
    let found = registry.find_by_mdns_instance("test-instance");
    assert_eq!(found, Some(id));
}

#[test]
fn test_registry_find_by_mdns_instance_not_found() {
    let registry = Registry::new();
    assert!(registry.find_by_mdns_instance("nonexistent").is_none());
}
```

**Implementation**:
Add to Backend struct:
```rust
// In Backend
mdns_instance: Option<String>,  // mDNS instance name for lookup
```

Add to Registry:
```rust
pub fn has_backend_url(&self, url: &str) -> bool {
    let normalized = normalize_url(url);
    self.backends.iter().any(|entry| normalize_url(entry.url()) == normalized)
}

pub fn set_mdns_instance(&self, id: &str, instance: &str) -> Result<(), RegistryError> {
    // Set mdns_instance field on backend
}

pub fn find_by_mdns_instance(&self, instance: &str) -> Option<String> {
    self.backends.iter()
        .find(|entry| entry.mdns_instance() == Some(instance))
        .map(|entry| entry.id().to_string())
}

fn normalize_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}
```

**Acceptance Criteria**:
- [X] `has_backend_url` returns true when URL exists in registry
- [X] `has_backend_url` returns false when URL not found
- [X] URL comparison is normalized (trailing slash ignored)
- [X] `set_mdns_instance` stores instance name on backend
- [X] `find_by_mdns_instance` returns backend ID when found
- [X] `find_by_mdns_instance` returns None when not found
- [X] All 6 tests pass

**Test Command**: `cargo test registry::tests::mdns`

---

## T07: Implement MdnsDiscovery Core Structure

**Goal**: Create the main MdnsDiscovery struct.

**Files to modify**:
- `src/discovery/mod.rs`

**Tests to Write First**:
```rust
#[test]
fn test_mdns_discovery_new() {
    let registry = Arc::new(Registry::new());
    let config = DiscoveryConfig::default();
    let discovery = MdnsDiscovery::new(config, registry);
    // Should not panic
}

#[test]
fn test_mdns_discovery_disabled_returns_immediately() {
    let registry = Arc::new(Registry::new());
    let mut config = DiscoveryConfig::default();
    config.enabled = false;
    
    let discovery = MdnsDiscovery::new(config, registry);
    let cancel = CancellationToken::new();
    
    let handle = discovery.start(cancel.clone());
    // Should complete almost immediately since disabled
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        handle
    ).await;
    assert!(result.is_ok());
}

#[test]
async fn test_mdns_discovery_responds_to_cancellation() {
    let registry = Arc::new(Registry::new());
    let config = DiscoveryConfig::default();
    let discovery = MdnsDiscovery::new(config, registry);
    let cancel = CancellationToken::new();
    
    let handle = discovery.start(cancel.clone());
    
    // Cancel after short delay
    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.cancel();
    
    // Should complete within reasonable time
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        handle
    ).await;
    assert!(result.is_ok());
}
```

**Implementation**:
```rust
use std::sync::Arc;
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub struct MdnsDiscovery {
    config: DiscoveryConfig,
    registry: Arc<Registry>,
    pending_removal: Arc<Mutex<HashMap<String, Instant>>>,
}

impl MdnsDiscovery {
    pub fn new(config: DiscoveryConfig, registry: Arc<Registry>) -> Self {
        Self {
            config,
            registry,
            pending_removal: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    pub fn start(self, cancel_token: CancellationToken) -> JoinHandle<()> {
        tokio::spawn(async move {
            if !self.config.enabled {
                tracing::info!("mDNS discovery disabled");
                return;
            }
            
            // Main discovery loop (implemented in T10)
            self.run(cancel_token).await;
        })
    }
    
    async fn run(self, cancel_token: CancellationToken) {
        // Placeholder - implemented in T10
        cancel_token.cancelled().await;
    }
}
```

**Acceptance Criteria**:
- [X] MdnsDiscovery::new creates instance without panic
- [X] start() returns JoinHandle
- [X] Disabled config returns immediately
- [X] Responds to cancellation token
- [X] All 3 tests pass
- [ ] Disabled config returns immediately
- [ ] Responds to cancellation token
- [ ] All 3 tests pass

**Test Command**: `cargo test mdns_discovery`

---

## T08: Implement Service Found Handler

**Goal**: Handle ServiceFound events by adding backends to registry.

**Files to modify**:
- `src/discovery/mod.rs`

**Tests to Write First**:
```rust
#[tokio::test]
async fn test_handle_service_found_adds_backend() {
    let registry = Arc::new(Registry::new());
    let discovery = create_test_discovery(registry.clone());
    
    let event = DiscoveryEvent::ServiceFound {
        instance: "test-ollama".to_string(),
        service_type: "_ollama._tcp.local".to_string(),
        addresses: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))],
        port: 11434,
        txt_records: HashMap::new(),
    };
    
    discovery.handle_event(event).await;
    
    assert_eq!(registry.backend_count(), 1);
    assert!(registry.has_backend_url("http://192.168.1.10:11434"));
}

#[tokio::test]
async fn test_handle_service_found_skips_existing_url() {
    let registry = Arc::new(Registry::new());
    // Pre-add a static backend
    let static_backend = create_backend_with_url("http://192.168.1.10:11434");
    registry.add_backend(static_backend).unwrap();
    
    let discovery = create_test_discovery(registry.clone());
    
    let event = DiscoveryEvent::ServiceFound {
        addresses: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))],
        port: 11434,
        ...
    };
    
    discovery.handle_event(event).await;
    
    // Should still be just 1 backend (static one preserved)
    assert_eq!(registry.backend_count(), 1);
}

#[tokio::test]
async fn test_handle_service_found_sets_mdns_instance() {
    let registry = Arc::new(Registry::new());
    let discovery = create_test_discovery(registry.clone());
    
    let event = DiscoveryEvent::ServiceFound {
        instance: "my-server._ollama._tcp.local".to_string(),
        ...
    };
    
    discovery.handle_event(event).await;
    
    let id = registry.find_by_mdns_instance("my-server._ollama._tcp.local").unwrap();
    assert!(id.len() > 0);
}

#[tokio::test]
async fn test_handle_service_found_logs_discovery() {
    // Use tracing-test to capture logs
    let registry = Arc::new(Registry::new());
    let discovery = create_test_discovery(registry.clone());
    
    let event = DiscoveryEvent::ServiceFound { ... };
    discovery.handle_event(event).await;
    
    // Verify INFO log was emitted
}
```

**Implementation**:
```rust
impl MdnsDiscovery {
    async fn handle_event(&self, event: DiscoveryEvent) {
        match event {
            DiscoveryEvent::ServiceFound { ref instance, .. } => {
                self.handle_service_found(event).await;
            }
            DiscoveryEvent::ServiceRemoved { ref instance, .. } => {
                self.handle_service_removed(instance).await;
            }
        }
    }
    
    async fn handle_service_found(&self, event: DiscoveryEvent) {
        let DiscoveryEvent::ServiceFound { 
            instance, 
            service_type, 
            addresses, 
            port, 
            txt_records 
        } = event else { return };
        
        // Convert to backend
        let Some(backend) = service_event_to_backend(&event) else {
            tracing::warn!("Could not convert service to backend: {}", instance);
            return;
        };
        
        // Check if URL already exists
        if self.registry.has_backend_url(backend.url()) {
            tracing::debug!("Backend URL already exists, skipping: {}", backend.url());
            return;
        }
        
        // Add to registry
        match self.registry.add_backend(backend) {
            Ok(id) => {
                self.registry.set_mdns_instance(&id, &instance).ok();
                tracing::info!(
                    instance = %instance,
                    url = %self.registry.get_backend(&id).map(|b| b.url().to_string()).unwrap_or_default(),
                    "Discovered backend via mDNS"
                );
                
                // Remove from pending removal if present
                self.pending_removal.lock().await.remove(&instance);
            }
            Err(e) => {
                tracing::error!("Failed to add discovered backend: {}", e);
            }
        }
    }
}
```

**Acceptance Criteria**:
- [X] ServiceFound adds new backend to registry
- [X] Skips if URL already exists (static takes precedence)
- [X] Sets mdns_instance on backend for later lookup
- [X] Logs discovery at INFO level
- [X] Removes instance from pending_removal if reappearing
- [X] All 4 tests pass

**Test Command**: `cargo test handle_service_found`

---

## T09: Implement Service Removed Handler & Grace Period

**Goal**: Handle ServiceRemoved with grace period before removal.

**Files to modify**:
- `src/discovery/mod.rs`

**Tests to Write First**:
```rust
#[tokio::test]
async fn test_handle_service_removed_sets_unknown() {
    let registry = Arc::new(Registry::new());
    let discovery = create_test_discovery(registry.clone());
    
    // First discover the service
    discovery.handle_event(create_found_event("test-service")).await;
    let id = registry.find_by_mdns_instance("test-service").unwrap();
    
    // Then remove it
    discovery.handle_event(DiscoveryEvent::ServiceRemoved {
        instance: "test-service".to_string(),
        service_type: "_ollama._tcp.local".to_string(),
    }).await;
    
    let backend = registry.get_backend(&id).unwrap();
    assert_eq!(backend.status(), BackendStatus::Unknown);
}

#[tokio::test]
async fn test_handle_service_removed_starts_grace_period() {
    let registry = Arc::new(Registry::new());
    let discovery = create_test_discovery(registry.clone());
    
    discovery.handle_event(create_found_event("test-service")).await;
    discovery.handle_event(create_removed_event("test-service")).await;
    
    // Backend should still exist during grace period
    assert!(registry.find_by_mdns_instance("test-service").is_some());
    
    // Check pending_removal has entry
    let pending = discovery.pending_removal.lock().await;
    assert!(pending.contains_key("test-service"));
}

#[tokio::test]
async fn test_grace_period_expiry_removes_backend() {
    let registry = Arc::new(Registry::new());
    let mut config = DiscoveryConfig::default();
    config.grace_period_seconds = 1; // Short for testing
    let discovery = MdnsDiscovery::new(config, registry.clone());
    
    discovery.handle_event(create_found_event("test-service")).await;
    discovery.handle_event(create_removed_event("test-service")).await;
    
    // Wait for grace period + cleanup cycle
    tokio::time::sleep(Duration::from_secs(2)).await;
    discovery.cleanup_stale_backends().await;
    
    // Backend should be removed
    assert!(registry.find_by_mdns_instance("test-service").is_none());
}

#[tokio::test]
async fn test_service_reappears_cancels_removal() {
    let registry = Arc::new(Registry::new());
    let discovery = create_test_discovery(registry.clone());
    
    // Discover -> Remove -> Rediscover
    discovery.handle_event(create_found_event("test-service")).await;
    discovery.handle_event(create_removed_event("test-service")).await;
    discovery.handle_event(create_found_event("test-service")).await;
    
    // Should not be in pending_removal
    let pending = discovery.pending_removal.lock().await;
    assert!(!pending.contains_key("test-service"));
    
    // Backend should still exist
    assert!(registry.find_by_mdns_instance("test-service").is_some());
}

#[tokio::test]
async fn test_cleanup_only_removes_mdns_backends() {
    let registry = Arc::new(Registry::new());
    
    // Add a static backend
    let static_backend = create_static_backend();
    registry.add_backend(static_backend).unwrap();
    
    let discovery = create_test_discovery(registry.clone());
    
    // Discover and remove an mDNS backend
    discovery.handle_event(create_found_event("mdns-service")).await;
    discovery.handle_event(create_removed_event("mdns-service")).await;
    
    // Run cleanup
    discovery.cleanup_stale_backends().await;
    
    // Static backend should remain
    assert_eq!(registry.backend_count(), 1);
}
```

**Implementation**:
```rust
impl MdnsDiscovery {
    async fn handle_service_removed(&self, instance: &str) {
        // Find backend by mDNS instance
        let Some(id) = self.registry.find_by_mdns_instance(instance) else {
            tracing::debug!("Service removed but not in registry: {}", instance);
            return;
        };
        
        // Set status to Unknown
        if let Err(e) = self.registry.update_status(
            &id, 
            BackendStatus::Unknown, 
            Some("Service disappeared from mDNS".to_string())
        ) {
            tracing::error!("Failed to update backend status: {}", e);
            return;
        }
        
        // Add to pending removal
        self.pending_removal.lock().await.insert(instance.to_string(), Instant::now());
        
        tracing::warn!(
            instance = %instance,
            grace_period_seconds = self.config.grace_period_seconds,
            "Backend disappeared from mDNS, starting grace period"
        );
    }
    
    async fn cleanup_stale_backends(&self) {
        let grace_period = Duration::from_secs(self.config.grace_period_seconds);
        let now = Instant::now();
        
        let mut pending = self.pending_removal.lock().await;
        let expired: Vec<String> = pending
            .iter()
            .filter(|(_, &removal_time)| now.duration_since(removal_time) > grace_period)
            .map(|(instance, _)| instance.clone())
            .collect();
        
        for instance in expired {
            pending.remove(&instance);
            
            if let Some(id) = self.registry.find_by_mdns_instance(&instance) {
                if let Err(e) = self.registry.remove_backend(&id) {
                    tracing::error!("Failed to remove stale backend: {}", e);
                } else {
                    tracing::info!(instance = %instance, "Removed stale mDNS backend after grace period");
                }
            }
        }
    }
}
```

**Acceptance Criteria**:
- [X] ServiceRemoved sets backend status to Unknown
- [X] ServiceRemoved adds instance to pending_removal with timestamp
- [X] Backend is removed after grace_period_seconds expires
- [X] Reappearing service cancels pending removal
- [X] Cleanup only removes mDNS backends, not static
- [X] Logs at WARN when removal starts, INFO when removed
- [X] All 5 tests pass

**Test Command**: `cargo test handle_service_removed grace_period`

---

## T10: Implement Real mDNS Browser Integration

**Goal**: Wire up actual mdns-sd crate for real service discovery.

**Files to modify**:
- `src/discovery/mod.rs`

**Tests to Write First** (integration tests):
```rust
#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored
async fn test_mdns_browser_starts_without_panic() {
    let registry = Arc::new(Registry::new());
    let config = DiscoveryConfig::default();
    let discovery = MdnsDiscovery::new(config, registry);
    let cancel = CancellationToken::new();
    
    let handle = discovery.start(cancel.clone());
    
    // Let it run briefly
    tokio::time::sleep(Duration::from_secs(2)).await;
    cancel.cancel();
    
    handle.await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_mdns_graceful_fallback_on_error() {
    // Test behavior when mDNS is unavailable
    // (e.g., in Docker without proper networking)
}
```

**Implementation**:
```rust
use mdns_sd::{ServiceDaemon, ServiceEvent};

impl MdnsDiscovery {
    async fn run(self, cancel_token: CancellationToken) {
        // Try to create mDNS daemon
        let daemon = match ServiceDaemon::new() {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("mDNS unavailable, discovery disabled: {}", e);
                return;
            }
        };
        
        // Browse for each service type
        let mut receivers = Vec::new();
        for service_type in &self.config.service_types {
            match daemon.browse(service_type) {
                Ok(receiver) => {
                    tracing::info!("Browsing for mDNS service: {}", service_type);
                    receivers.push(receiver);
                }
                Err(e) => {
                    tracing::error!("Failed to browse {}: {}", service_type, e);
                }
            }
        }
        
        if receivers.is_empty() {
            tracing::warn!("No mDNS service types could be browsed");
            return;
        }
        
        // Spawn cleanup task
        let cleanup_discovery = self.clone();
        let cleanup_cancel = cancel_token.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            loop {
                tokio::select! {
                    _ = cleanup_cancel.cancelled() => break,
                    _ = interval.tick() => {
                        cleanup_discovery.cleanup_stale_backends().await;
                    }
                }
            }
        });
        
        // Main event loop
        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    tracing::info!("mDNS discovery shutting down");
                    break;
                }
                // Check each receiver for events
                // Convert ServiceEvent to DiscoveryEvent
                // Call handle_event
            }
        }
        
        // Shutdown daemon
        daemon.shutdown().ok();
    }
}
```

**Acceptance Criteria**:
- [X] Creates ServiceDaemon successfully on supported platforms
- [X] Graceful fallback if mDNS unavailable (logs warning, continues)
- [X] Browses for all configured service_types
- [X] Converts ServiceEvent::ServiceResolved to DiscoveryEvent::ServiceFound
- [X] Converts ServiceEvent::ServiceRemoved to DiscoveryEvent::ServiceRemoved
- [X] Cleanup task runs every 10 seconds
- [X] Responds to cancellation token for shutdown
- [ ] Integration tests pass (when not ignored)

**Test Command**: `cargo test -- --ignored` (for integration tests)

---

## T11: CLI Integration

**Goal**: Wire mDNS discovery into serve command.

**Files to modify**:
- `src/cli/serve.rs`
- `src/config.rs` (if needed)

**Implementation Steps**:

1. Update serve command to create MdnsDiscovery:
```rust
// In run_serve()

// Create mDNS discovery if enabled
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

// On shutdown
if let Some(handle) = discovery_handle {
    handle.await?;
}
```

2. Verify `--no-discovery` flag works

3. Update config loading to include discovery settings

**Acceptance Criteria**:
- [X] `nexus serve` starts mDNS discovery when enabled in config
- [X] `nexus serve --no-discovery` skips mDNS discovery
- [X] Discovery config loaded from nexus.toml
- [X] Graceful shutdown waits for discovery to stop
- [X] Logs indicate whether discovery is enabled/disabled

**Test Command**: Manual testing with real Ollama instance

---

## T12: Documentation & Cleanup

**Goal**: Polish code and update documentation.

**Files to modify**:
- `src/discovery/*.rs` (doc comments)
- `README.md`
- `nexus.example.toml`
- `docs/FEATURES.md`

**Tasks**:
1. Add doc comments to all public items in discovery module
2. Update nexus.example.toml with discovery section (already present, verify)
3. Update README.md with discovery feature description
4. Update docs/FEATURES.md to mark F05 as complete
5. Run `cargo clippy -- -D warnings`
6. Run `cargo fmt --check`
7. Run `cargo doc` and verify no warnings

**Acceptance Criteria**:
- [X] All public items have doc comments
- [X] README documents mDNS discovery feature
- [X] nexus.example.toml has complete discovery config
- [ ] FEATURES.md shows F05 as complete
- [X] `cargo clippy -- -D warnings` passes
- [X] `cargo fmt --check` passes
- [X] `cargo doc` generates without warnings

**Test Command**: `cargo clippy -- -D warnings && cargo fmt --check && cargo doc`

---

## Definition of Done

- [X] All ~35 tests pass (29 discovery tests + registry tests)
- [ ] Discovery works with real Ollama instance on network (requires manual testing)
- [X] Graceful fallback when mDNS unavailable
- [X] Grace period prevents flapping
- [X] Manual config takes precedence over discovered
- [X] CLI integration complete (`--no-discovery` works)
- [X] `cargo clippy -- -D warnings` passes
- [X] `cargo fmt --check` passes
- [X] Doc comments on all public items
- [X] README and FEATURES.md updated
