# Quickstart: mDNS Discovery

**Feature**: F05 mDNS Discovery  
**Status**: ✅ Implemented  
**Prerequisites**: Rust 1.87+, Nexus codebase cloned, network with multicast support

---

## Overview

mDNS (multicast DNS) discovery allows Nexus to automatically find LLM backends on the local network without manual configuration. When enabled, Nexus browses for advertised services (e.g., Ollama instances) and registers them in the backend registry. This guide covers how to configure, run, and test mDNS discovery.

---

## Project Structure

```
nexus/
├── src/
│   ├── discovery/
│   │   ├── mod.rs          # MdnsDiscovery service, event loop, grace period cleanup
│   │   ├── events.rs       # DiscoveryEvent enum (ServiceFound, ServiceRemoved)
│   │   └── parser.rs       # TXT record parsing, backend type inference
│   ├── config/
│   │   └── discovery.rs    # DiscoveryConfig (enabled, service_types, grace_period)
│   └── registry/
│       └── mod.rs          # Backend registry (source of truth)
├── nexus.example.toml      # Example config with discovery section
└── tests/
    └── integration/        # Integration tests
```

---

## Configuration

### Enable/Disable mDNS Discovery

In `nexus.toml`:

```toml
[discovery]
# Toggle mDNS discovery on/off
enabled = true

# Service types to browse for (trailing dot is optional — added automatically)
service_types = ["_ollama._tcp.local", "_llm._tcp.local"]

# Grace period before removing disappeared backends (seconds)
grace_period_seconds = 60
```

### Minimal Config (Discovery Only)

```toml
[server]
host = "0.0.0.0"
port = 8000

[discovery]
enabled = true
service_types = ["_ollama._tcp.local"]
grace_period_seconds = 30

[health_check]
enabled = true
interval_seconds = 15
```

### Disable Discovery (Static-Only Mode)

```toml
[discovery]
enabled = false

[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
priority = 1
```

---

## Usage

### 1. Start Nexus with Discovery Enabled

```bash
# With debug logging to see discovery events
RUST_LOG=debug cargo run -- serve
```

Expected log output when discovery starts:

```
INFO  nexus::discovery: mDNS service daemon started
INFO  nexus::discovery: Browsing for mDNS service  service_type="_ollama._tcp.local"
INFO  nexus::discovery: Browsing for mDNS service  service_type="_llm._tcp.local"
```

### 2. Observe Backend Discovery

When a backend is found on the network, you'll see:

```
INFO  nexus::discovery: Discovered backend via mDNS  instance="ollama-server._ollama._tcp.local" url="http://192.168.1.10:11434"
```

### 3. Verify Discovered Backends

```bash
# List all backends (discovered + static)
cargo run -- backends list
```

```bash
# Check via API
curl -s http://localhost:8000/v1/models | jq '.data[].id'
```

### 4. Start Nexus with Discovery Disabled

```bash
# Override config via environment variable
NEXUS_DISCOVERY_ENABLED=false cargo run -- serve
```

Expected:

```
INFO  nexus::discovery: mDNS discovery disabled
```

---

## Manual Testing

### Test 1: Verify Discovery Starts

```bash
# Start Nexus with debug logging
RUST_LOG=nexus::discovery=debug cargo run -- serve
```

**Expected output**:
```
INFO  nexus::discovery: mDNS service daemon started
INFO  nexus::discovery: Browsing for mDNS service  service_type="_ollama._tcp.local"
INFO  nexus::discovery: Browsing for mDNS service  service_type="_llm._tcp.local"
```

✅ Pass if: Both service types are browsed  
❌ Fail if: "mDNS unavailable" warning appears (check network multicast support)

### Test 2: Verify Discovery Disabled Path

```bash
NEXUS_DISCOVERY_ENABLED=false RUST_LOG=nexus::discovery=info cargo run -- serve
```

**Expected output**:
```
INFO  nexus::discovery: mDNS discovery disabled
```

✅ Pass if: Discovery exits immediately, no browsing messages  
❌ Fail if: Browsing messages still appear

### Test 3: Discover a Local Ollama Instance

1. Start Ollama (it advertises via mDNS by default):
   ```bash
   ollama serve
   ```

2. Start Nexus:
   ```bash
   RUST_LOG=nexus::discovery=debug cargo run -- serve
   ```

3. Wait a few seconds and verify:
   ```bash
   curl -s http://localhost:8000/v1/models | jq .
   ```

**Expected**: Models from the Ollama instance appear in the response.

✅ Pass if: Ollama models are listed  
❌ Fail if: Empty model list (check that Ollama advertises `_ollama._tcp.local`)

### Test 4: Simulate Service Disappearance (Grace Period)

1. Start Nexus with a short grace period:
   ```toml
   [discovery]
   enabled = true
   grace_period_seconds = 10
   ```

2. Start an Ollama backend, wait for discovery.

3. Stop the Ollama backend.

**Expected log sequence**:
```
WARN  nexus::discovery: Backend disappeared from mDNS, starting grace period  instance="..." grace_period_seconds=10
```

4. Wait 10+ seconds:

```
INFO  nexus::discovery: Removed stale mDNS backend after grace period  instance="..."
```

✅ Pass if: Backend removed after grace period expires  
❌ Fail if: Backend removed immediately (grace period not working)

### Test 5: Service Reappearance Cancels Removal

1. Start Nexus with `grace_period_seconds = 60`.
2. Discover a backend, then stop it.
3. Restart the backend within 60 seconds.

**Expected log**:
```
WARN  nexus::discovery: Backend disappeared from mDNS, starting grace period ...
DEBUG nexus::discovery: Resolved ... (service reappeared)
```

✅ Pass if: Backend remains in registry, no removal occurs  
❌ Fail if: Backend removed despite reappearing

### Test 6: Static Backend Not Duplicated by mDNS

1. Configure a static backend at the same URL that mDNS will discover:
   ```toml
   [[backends]]
   name = "local-ollama"
   url = "http://192.168.1.10:11434"
   type = "ollama"
   ```

2. Start Nexus with discovery enabled.

**Expected log**:
```
DEBUG nexus::discovery: Backend URL already exists, skipping  url="http://192.168.1.10:11434"
```

✅ Pass if: Only one backend registered for that URL  
❌ Fail if: Duplicate backends appear

### Test 7: Run Unit Tests

```bash
# All discovery tests
cargo test discovery::

# Specific test modules
cargo test discovery::tests::
cargo test discovery::events::tests::
cargo test discovery::parser::tests::
```

**Expected**: All tests pass.

```
test discovery::tests::test_service_to_backend_basic ... ok
test discovery::tests::test_service_to_backend_prefers_ipv4 ... ok
test discovery::tests::test_handle_service_found_adds_backend ... ok
test discovery::tests::test_handle_service_found_skips_existing_url ... ok
test discovery::tests::test_handle_service_removed_starts_grace_period ... ok
test discovery::tests::test_grace_period_expiry_removes_backend ... ok
test discovery::tests::test_service_reappears_cancels_removal ... ok
test discovery::tests::test_cleanup_only_removes_mdns_backends ... ok
...
```

---

## Service Type Reference

| Service Type | Backend Type Inferred | Default API Path |
|---|---|---|
| `_ollama._tcp.local` | Ollama | (none) |
| `_llm._tcp.local` | Generic | `/v1` |

### TXT Record Fields

Backends can advertise metadata via mDNS TXT records:

| Key | Example | Description |
|---|---|---|
| `type` | `ollama`, `vllm`, `llamacpp` | Override backend type inference |
| `api_path` | `/v1` | Override default API path |
| `version` | `0.1.24` | Backend software version |

---

## Debugging Tips

### mDNS Unavailable

```
WARN  nexus::discovery: mDNS unavailable, discovery disabled: ...
```

**Fix**: Ensure multicast DNS is supported on your network. On Linux:
```bash
# Check if avahi-daemon is running
systemctl status avahi-daemon

# Check multicast route
ip route show | grep multicast
```

### No Services Found

1. Verify the backend advertises the correct service type:
   ```bash
   # Use avahi-browse to check mDNS services on the network
   avahi-browse -t _ollama._tcp
   ```

2. Check firewall rules allow mDNS (UDP port 5353):
   ```bash
   sudo ufw allow 5353/udp
   ```

3. Verify service types match in config:
   ```toml
   service_types = ["_ollama._tcp.local"]
   ```

### Discovery Works But Models Empty

mDNS only discovers the backend URL. Models are populated by the **health checker**:
```toml
[health_check]
enabled = true
interval_seconds = 15
```

Ensure health checking is enabled — it fetches model lists from discovered backends.

---

## References

- **Feature Spec**: `specs/005-mdns-discovery/spec.md`
- **Data Model**: `specs/005-mdns-discovery/data-model.md`
- **Implementation Walkthrough**: `specs/005-mdns-discovery/walkthrough.md`
- **mdns-sd Docs**: https://docs.rs/mdns-sd/latest/mdns_sd/
- **Avahi mDNS**: https://avahi.org/
