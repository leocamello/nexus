# Implementation Plan: mDNS Discovery

**Spec**: [spec.md](./spec.md)  
**Status**: Ready for Implementation  
**Estimated Complexity**: Medium-High

## Constitution Compliance

### Simplicity Gate
- [x] Using ≤3 main modules: `discovery/` with 4 submodules (config, events, parser, mod)
- [x] No speculative features: focused on Ollama + generic `_llm._tcp.local` services
- [x] No premature optimization: straightforward event handling
- [x] Simplest approach: single background task with direct registry calls

### Anti-Abstraction Gate
- [x] Using mdns-sd/Tokio directly (no wrapper layers)
- [x] Single representation: DiscoveryEvent enum maps directly to Backend
- [x] No framework-on-framework patterns
- [x] Minimal abstractions: parse → convert → register

### Integration-First Gate
- [x] API contracts defined: DiscoveryEvent, Registry extensions specified
- [x] Integration tests planned: T10 tests real mDNS with `#[ignore]`
- [x] End-to-end testable: mock mDNS events in T05-T09

### Performance Gate
- [x] Discovery latency < 5s (non-critical async path)
- [x] Memory overhead < 10MB (within 50MB baseline)
- [x] Does NOT impact routing decision time (mDNS runs in background task)
- [x] No impact on the < 1ms routing budget (discovery is async)

## Approach

Implement mDNS discovery using the `mdns-sd` crate, running as a background tokio task. Follow strict TDD: write failing tests first using mock service events, then implement to make them pass.

### Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| mDNS library | `mdns-sd` | Cross-platform, async-ready, well-maintained |
| Event handling | `mpsc` channel | Decouple discovery from registry updates |
| Grace period tracking | `HashMap<String, Instant>` | Track when services were last seen |
| Cleanup task | Periodic (every 10s) | Check for expired grace periods |
| Service identification | mDNS instance name | Unique per service instance |

### File Structure

```
src/
├── discovery/
│   ├── mod.rs              # MdnsDiscovery struct and main logic
│   ├── config.rs           # DiscoveryConfig struct
│   ├── events.rs           # DiscoveryEvent enum
│   ├── parser.rs           # TXT record parsing utilities
│   └── tests.rs            # Unit tests (mocked mDNS)
├── lib.rs                  # Add pub mod discovery
└── cli/
    └── serve.rs            # Wire up discovery on startup
```

### Dependencies

**New dependencies needed**:
```toml
[dependencies]
mdns-sd = "0.11"  # mDNS service discovery
```

## Implementation Phases

### Phase 1: Configuration & Types (Tests First)

**Goal**: Define configuration and event types.

**Tests to write first**:
1. `test_discovery_config_defaults` - Default config has sensible values
2. `test_discovery_config_from_toml` - Config parses from TOML correctly
3. `test_discovery_event_serialization` - Events can be logged/debugged
4. `test_service_types_validation` - Invalid service types rejected

**Implementation**:
1. Create `src/discovery/mod.rs` with module declarations
2. Create `src/discovery/config.rs`:
   ```rust
   pub struct DiscoveryConfig {
       pub enabled: bool,
       pub service_types: Vec<String>,
       pub grace_period_seconds: u64,
   }
   ```
3. Create `src/discovery/events.rs`:
   ```rust
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

**Acceptance**: All 4 tests pass.

---

### Phase 2: TXT Record Parser (Tests First)

**Goal**: Parse TXT records into backend metadata.

**Tests to write first**:
1. `test_parse_txt_empty` - Empty TXT records return defaults
2. `test_parse_txt_type_ollama` - `type=ollama` parsed correctly
3. `test_parse_txt_type_vllm` - `type=vllm` parsed correctly
4. `test_parse_txt_api_path` - `api_path=/v1` extracted
5. `test_parse_txt_version` - `version=1.0.0` extracted
6. `test_parse_txt_unknown_keys_ignored` - Unknown keys don't fail
7. `test_infer_type_from_service_type` - `_ollama._tcp.local` → Ollama

**Implementation**:
1. Create `src/discovery/parser.rs`:
   ```rust
   pub struct ParsedService {
       pub backend_type: BackendType,
       pub api_path: String,
       pub version: Option<String>,
   }
   
   pub fn parse_txt_records(
       txt: &HashMap<String, String>,
       service_type: &str,
   ) -> ParsedService;
   ```

**Acceptance**: All 7 tests pass.

---

### Phase 3: Service Event to Backend Conversion (Tests First)

**Goal**: Convert discovery events to Backend structs.

**Tests to write first**:
1. `test_service_to_backend_basic` - Creates backend from minimal service info
2. `test_service_to_backend_with_api_path` - URL includes api_path
3. `test_service_to_backend_prefers_ipv4` - IPv4 chosen over IPv6 when both present
4. `test_service_to_backend_ipv6_only` - Works with IPv6-only service
5. `test_service_to_backend_discovery_source` - Source is MDNS
6. `test_service_to_backend_generates_name` - Human-readable name from instance

**Implementation**:
1. Add to `src/discovery/mod.rs`:
   ```rust
   fn service_event_to_backend(event: &DiscoveryEvent) -> Option<Backend>;
   ```
2. Handle IP address selection (prefer IPv4)
3. Build URL with api_path
4. Set DiscoverySource::MDNS

**Acceptance**: All 6 tests pass.

---

### Phase 4: MdnsDiscovery Core Structure (Tests First)

**Goal**: Implement the main MdnsDiscovery struct with lifecycle management.

**Tests to write first**:
1. `test_mdns_discovery_new` - Constructor creates valid instance
2. `test_mdns_discovery_disabled` - Returns immediately if not enabled
3. `test_mdns_discovery_shutdown` - Responds to cancellation token
4. `test_mdns_discovery_tracks_grace_periods` - Internal state tracks removals

**Implementation**:
1. Create main struct in `src/discovery/mod.rs`:
   ```rust
   pub struct MdnsDiscovery {
       config: DiscoveryConfig,
       registry: Arc<Registry>,
       // Track services pending removal: instance_name -> removal_time
       pending_removal: Arc<Mutex<HashMap<String, Instant>>>,
   }
   
   impl MdnsDiscovery {
       pub fn new(config: DiscoveryConfig, registry: Arc<Registry>) -> Self;
       pub fn start(self, cancel_token: CancellationToken) -> JoinHandle<()>;
   }
   ```

**Acceptance**: All 4 tests pass.

---

### Phase 5: Discovery Event Handling (Tests First)

**Goal**: Process service found/removed events with registry integration.

**Tests to write first**:
1. `test_handle_service_found_adds_backend` - New service added to registry
2. `test_handle_service_found_skips_existing_url` - Static backend URL not duplicated
3. `test_handle_service_found_triggers_health_check` - Health check signal sent
4. `test_handle_service_removed_sets_unknown` - Status set to Unknown
5. `test_handle_service_removed_starts_grace_period` - Timer started
6. `test_handle_service_reappears_cancels_removal` - Grace period cancelled
7. `test_grace_period_expiry_removes_backend` - Backend removed after timeout

**Implementation**:
1. Implement `handle_service_found()`:
   - Convert to backend
   - Check for existing URL
   - Add to registry
   - Trigger health check
2. Implement `handle_service_removed()`:
   - Set status to Unknown
   - Add to pending_removal with timestamp
3. Implement cleanup task:
   - Every 10s, check pending_removal
   - Remove backends past grace period

**Acceptance**: All 7 tests pass.

---

### Phase 6: Registry Extensions (Tests First)

**Goal**: Add registry methods needed for discovery.

**Tests to write first**:
1. `test_registry_has_backend_url_true` - Returns true when URL exists
2. `test_registry_has_backend_url_false` - Returns false when not found
3. `test_registry_find_by_mdns_instance` - Finds by instance name
4. `test_registry_cleanup_stale_returns_removed` - Returns IDs of removed backends

**Implementation**:
1. Add to `src/registry/mod.rs`:
   ```rust
   pub fn has_backend_url(&self, url: &str) -> bool;
   pub fn find_by_mdns_instance(&self, instance: &str) -> Option<String>;
   pub fn get_mdns_backends_older_than(&self, duration: Duration) -> Vec<String>;
   ```

**Acceptance**: All 4 tests pass.

---

### Phase 7: Real mDNS Integration (Integration Tests)

**Goal**: Test with actual mdns-sd crate (mocked daemon where possible).

**Tests to write**:
1. `test_mdns_browser_starts` - Browser initializes without panic
2. `test_mdns_browser_handles_network_error` - Graceful fallback on error
3. `test_mdns_service_types_browsed` - All configured types are browsed

**Implementation**:
1. Wire up `mdns_sd::ServiceDaemon`
2. Register browsers for each service type
3. Event loop receiving `ServiceEvent`
4. Map to internal `DiscoveryEvent`
5. Handle errors gracefully

**Acceptance**: All 3 tests pass on local machine.

---

### Phase 8: CLI Integration

**Goal**: Wire discovery into serve command.

**Tasks**:
1. Update `src/cli/serve.rs`:
   - Create MdnsDiscovery if enabled
   - Start discovery task
   - Pass cancel token for shutdown
2. Update config loading to include discovery settings
3. Add `--no-discovery` flag (already exists, verify works)

**Acceptance**: 
- `nexus serve` starts discovery when enabled
- `nexus serve --no-discovery` skips it
- Graceful shutdown stops discovery

---

### Phase 9: Documentation & Cleanup

**Goal**: Polish and document.

**Tasks**:
1. Add doc comments to all public items
2. Update README with discovery section
3. Run `cargo clippy` and `cargo fmt`
4. Add tracing spans for discovery events
5. Update nexus.example.toml with discovery options

**Acceptance**:
- `cargo doc` generates docs
- `cargo clippy -- -D warnings` passes
- `cargo fmt --check` passes

## Task Summary

| Phase | Focus | Tests | Priority |
|-------|-------|-------|----------|
| 1. Config & Types | Data structures | 4 tests | P1 |
| 2. TXT Parser | Metadata extraction | 7 tests | P1 |
| 3. Event Conversion | Backend creation | 6 tests | P1 |
| 4. Core Structure | Lifecycle | 4 tests | P1 |
| 5. Event Handling | Registry integration | 7 tests | P1 |
| 6. Registry Extensions | New methods | 4 tests | P1 |
| 7. Real mDNS | Integration | 3 tests | P2 |
| 8. CLI Integration | Wiring | manual | P1 |
| 9. Documentation | Cleanup | - | P2 |

**Total**: ~35 tests, 9 phases

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| mDNS unavailable in CI | Mock the mDNS layer, test event handling independently |
| Platform differences | Test on Linux (CI), document macOS/Windows behavior |
| mdns-sd API changes | Pin version, wrap in internal abstraction |
| Race conditions | Use proper synchronization, test with concurrent events |

## Definition of Done

- [ ] All ~35 tests pass
- [ ] Discovery works with real Ollama instance
- [ ] Graceful fallback when mDNS unavailable
- [ ] Grace period prevents flapping
- [ ] Manual config takes precedence
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo fmt --check` passes  
- [ ] Doc comments on public items
- [ ] Integration with health checker verified
