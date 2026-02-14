# mDNS Discovery Protocol Contract

This document defines the mDNS service discovery protocol used by Nexus to automatically discover LLM backends on the local network.

## Protocol

**Transport**: mDNS (Multicast DNS) via UDP port 5353  
**Library**: `mdns-sd` crate (`ServiceDaemon`)  
**Direction**: Nexus browses (passive); backends advertise

---

## Service Types Browsed

Nexus browses for two mDNS service types by default:

| Service Type | Inferred Backend Type | Default API Path |
|---|---|---|
| `_ollama._tcp.local` | `Ollama` | `` (empty — root) |
| `_llm._tcp.local` | `Generic` | `/v1` |

Additional service types can be configured via `[discovery].service_types` in `nexus.toml`. The service type string is normalized with a trailing dot before browsing (required by `mdns-sd`).

---

## TXT Record Format

TXT records provide optional metadata about the service. All keys are optional.

### Recognized Keys

| Key | Type | Description | Example |
|---|---|---|---|
| `type` | string | Backend type override (case-insensitive) | `vllm`, `ollama`, `llamacpp`, `exo`, `openai` |
| `api_path` | string | API base path appended to URL | `/v1` |
| `version` | string | Backend software version | `0.1.45` |

### Type Resolution

1. If TXT record contains `type` key → parse as backend type
2. Otherwise → infer from service type name:
   - `_ollama._tcp.local` → `Ollama`
   - All others → `Generic`

### Recognized `type` Values

| TXT Value | BackendType | Notes |
|---|---|---|
| `ollama` | `Ollama` | Case-insensitive |
| `vllm` | `VLLM` | |
| `llamacpp` or `llama.cpp` | `LlamaCpp` | Both forms accepted |
| `exo` | `Exo` | |
| `openai` | `OpenAI` | |
| _(anything else)_ | `Generic` | Fallback |

### API Path Defaults

If `api_path` is not present in TXT records:
- **Ollama** backends: empty string (API at root)
- **All other types**: `/v1`

---

## Service Registration Fields

When a service is resolved, Nexus extracts the following fields to construct a `Backend`:

| Field | Source | Description |
|---|---|---|
| `id` | Generated | UUID v4, unique per discovery event |
| `name` | Instance name | First segment of mDNS instance, underscores replaced with spaces |
| `url` | IP + port + api_path | `http://{ip}:{port}{api_path}` |
| `backend_type` | TXT `type` or service type | See type resolution above |
| `models` | Empty | Populated later via health check |
| `discovery_source` | Constant | Always `MDNS` |
| `metadata` | TXT + instance | `mdns_instance` and optional `version` |

### URL Construction

```
http://{selected_ip}:{port}{api_path}
```

- **IPv4**: `http://192.168.1.10:11434`
- **IPv6**: `http://[::1]:11434`
- **With path**: `http://192.168.1.10:8000/v1`

### IP Address Selection

When multiple addresses are available, IPv4 is preferred over IPv6. If only IPv6 addresses exist, the address is wrapped in brackets (`[::1]`).

### Name Extraction

The instance name (e.g., `My_Ollama_Server._ollama._tcp.local`) is split on `.` and only the first segment is kept. Underscores are replaced with spaces.

**Example**: `My_Ollama_Server._ollama._tcp.local` → `My Ollama Server`

---

## Example mDNS Records

### Ollama Backend (Minimal)

```
Instance: ollama-desktop._ollama._tcp.local
Port:     11434
A:        192.168.1.10
TXT:      (empty)
```

**Result**: Backend registered as `http://192.168.1.10:11434`, type `Ollama`, name `ollama-desktop`.

### vLLM Backend (With TXT Records)

```
Instance: gpu-server._llm._tcp.local
Port:     8000
A:        192.168.1.50
TXT:      type=vllm api_path=/v1 version=0.4.1
```

**Result**: Backend registered as `http://192.168.1.50:8000/v1`, type `VLLM`, name `gpu-server`.

### llama.cpp Backend (IPv6 Only)

```
Instance: edge-node._llm._tcp.local
Port:     8080
AAAA:     fe80::1
TXT:      type=llamacpp
```

**Result**: Backend registered as `http://[fe80::1]:8080/v1`, type `LlamaCpp`, name `edge-node`.

---

## Discovery Event Lifecycle

```
  mDNS Network                    Nexus Discovery                   Registry
  ────────────                    ───────────────                   ────────
       │                                │                              │
       │  ServiceResolved               │                              │
       ├───────────────────────────────►│                              │
       │                                │  parse TXT records           │
       │                                │  select best IP (prefer v4)  │
       │                                │  build URL                   │
       │                                │  check URL not duplicate     │
       │                                │                              │
       │                                │  add_backend()               │
       │                                ├─────────────────────────────►│
       │                                │                              │  Status: Unknown
       │                                │                              │  Models: []
       │                                │                              │
       │                          (health checker runs separately)     │
       │                                │                              │  Status: Healthy
       │                                │                              │  Models: [discovered]
       │                                │                              │
       │  ServiceRemoved                │                              │
       ├───────────────────────────────►│                              │
       │                                │  update_status(Unknown)      │
       │                                ├─────────────────────────────►│
       │                                │  add to pending_removal      │
       │                                │  start grace period          │
       │                                │                              │
       │  (grace period expires)        │                              │
       │                                │  remove_backend()            │
       │                                ├─────────────────────────────►│
       │                                │                              │  (removed)
```

### States

1. **ServiceFound** → Backend added to registry with status `Unknown`, empty models list
2. **Health-checked** → Health checker (separate module) probes the backend URL, updates status to `Healthy`/`Unhealthy`, and populates the models list
3. **ServiceRemoved** → Status set to `Unknown`, grace period timer starts
4. **Grace period expires** → Backend removed from registry (cleanup runs every 10 seconds)
5. **Service reappears** → Removed from pending_removal, grace period cancelled

### Duplicate Detection

Before adding a backend, Nexus checks `registry.has_backend_url()`. If a backend with the same URL already exists (e.g., added via static config), the mDNS discovery event is silently skipped.

---

## Configuration

```toml
[discovery]
enabled = true                                        # Default: true
service_types = ["_ollama._tcp.local", "_llm._tcp.local"]  # Default shown
grace_period_seconds = 60                             # Default: 60
```

| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | bool | `true` | Enable/disable mDNS discovery |
| `service_types` | string[] | `["_ollama._tcp.local", "_llm._tcp.local"]` | Service types to browse |
| `grace_period_seconds` | u64 | `60` | Seconds to wait before removing a disappeared service |

---

## Error Handling

| Condition | Behavior |
|---|---|
| mDNS daemon fails to start | Log warning, discovery disabled (Nexus continues without it) |
| Service type browse fails | Log error, skip that service type, continue others |
| No addresses in resolved service | Skip (backend not created) |
| Duplicate URL detected | Skip silently (log at debug level) |
| Backend add fails | Log error, continue |

---

## Implementation Notes

- **Polling interval**: mDNS events are polled every 100ms in the main loop
- **Cleanup interval**: Stale backend cleanup runs every 10 seconds
- **Thread safety**: `pending_removal` map is protected by `tokio::sync::Mutex`
- **Shutdown**: Responds to `CancellationToken`; daemon is shut down gracefully
- **No re-registration**: Nexus is a browser only — it does not advertise itself via mDNS
