# Nexus - Feature Specifications

Detailed specifications for each feature in the Nexus LLM Orchestrator.

---

## Feature Index

| ID | Feature | Version | Status | Spec |
|----|---------|---------|--------|------|
| F01 | Core API Gateway | v0.1 | ✅ Complete | [specs/004-api-gateway](../specs/004-api-gateway/) |
| F02 | Backend Registry | v0.1 | ✅ Complete | [specs/001-backend-registry](../specs/001-backend-registry/) |
| F03 | Health Checker | v0.1 | ✅ Complete | [specs/002-health-checker](../specs/002-health-checker/) |
| F04 | CLI and Configuration | v0.1 | ✅ Complete | [specs/003-cli-configuration](../specs/003-cli-configuration/) |
| F05 | mDNS Discovery | v0.1 | ✅ Complete | [specs/005-mdns-discovery](../specs/005-mdns-discovery/) |
| F06 | Intelligent Router | v0.1 | ✅ Complete | [specs/006-intelligent-router](../specs/006-intelligent-router/) |
| F07 | Model Aliases | v0.1 | ✅ Complete | [specs/007-model-aliases](../specs/007-model-aliases/) |
| F08 | Fallback Chains | v0.1 | ✅ Complete | [specs/008-fallback-chains](../specs/008-fallback-chains/) |
| F09 | Request Metrics | v0.2 | ✅ Complete | [specs/009-request-metrics](../specs/009-request-metrics/) |
| F10 | Web Dashboard | v0.2 | ✅ Complete | [specs/010-web-dashboard](../specs/010-web-dashboard/) |
| F11 | Structured Request Logging | v0.2 | ✅ Complete | [specs/011-structured-logging](../specs/011-structured-logging/) |
| F12 | Cloud Backend Support | v0.3 | Planned | - |
| F13 | Privacy Zones & Capability Tiers | v0.3 | Planned | - |
| F14 | Inference Budget Management | v0.3 | Planned | - |
| F15 | Speculative Router | v0.4 | Planned | - |
| F16 | Quality Tracking & Backend Profiling | v0.4 | Planned | - |
| F17 | Embeddings API | v0.4 | Planned | - |
| F18 | Request Queuing & Prioritization | v0.4 | Planned | - |
| F19 | Pre-warming & Fleet Intelligence | v0.5 | Planned | - |
| F20 | Model Lifecycle Management | v0.5 | Planned | - |
| F21 | Multi-Tenant Support | v0.5 | Planned | - |
| F22 | Rate Limiting | v0.5 | Planned | - |
| F23 | Management UI | v0.5 | Planned | - |

### Current Status

- **v0.1 Foundation**: ✅ Released (F01-F08, 8 features)
- **v0.2 Observability**: ✅ Released (F09-F11, 3 features)
- **v0.3 Cloud Hybrid**: 🎯 Next (F12-F14)
- **v0.4 Intelligence**: Planned (F15-F18)
- **v0.5 Orchestration**: Planned (F19-F23)
- **Tests**: 462 passing, 81% coverage

---

## F01: Core API Gateway

### Overview
HTTP server exposing OpenAI-compatible endpoints that proxy requests to backends.

### Endpoints

#### POST /v1/chat/completions

**Request:**
```json
{
  "model": "llama3:70b",
  "messages": [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "Hello!"}
  ],
  "stream": true,
  "temperature": 0.7,
  "max_tokens": 1000
}
```

**Response (non-streaming):**
```json
{
  "id": "chatcmpl-abc123",
  "object": "chat.completion",
  "created": 1700000000,
  "model": "llama3:70b",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Hello! How can I help you today?"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 20,
    "completion_tokens": 10,
    "total_tokens": 30
  }
}
```

**Response (streaming):**
```
data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1700000000,"model":"llama3:70b","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}

data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1700000000,"model":"llama3:70b","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}

data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1700000000,"model":"llama3:70b","choices":[{"index":0,"delta":{"content":"!"},"finish_reason":"stop"}]}

data: [DONE]
```

#### GET /v1/models

**Response:**
```json
{
  "object": "list",
  "data": [
    {
      "id": "llama3:70b",
      "object": "model",
      "created": 1700000000,
      "owned_by": "nexus",
      "nexus": {
        "backends": ["local-ollama", "gpu-server"],
        "context_length": 8192,
        "supports_vision": false,
        "supports_tools": true
      }
    },
    {
      "id": "mistral:7b",
      "object": "model",
      "created": 1700000000,
      "owned_by": "nexus",
      "nexus": {
        "backends": ["local-ollama"],
        "context_length": 32768,
        "supports_vision": false,
        "supports_tools": false
      }
    }
  ]
}
```

#### GET /health

**Response:**
```json
{
  "status": "healthy",
  "version": "1.0.0",
  "uptime_seconds": 3600,
  "backends": {
    "total": 3,
    "healthy": 2,
    "unhealthy": 1
  },
  "models": {
    "total": 5
  }
}
```

### Acceptance Criteria
- [x] POST /v1/chat/completions works with non-streaming
- [x] POST /v1/chat/completions works with streaming (SSE)
- [x] GET /v1/models lists all models from all backends
- [x] GET /health returns system status
- [x] Handles concurrent requests (100+)
- [x] Proper error responses in OpenAI format

---

## F02: Backend Registry

### Overview
In-memory data store tracking all known backends and their models.

### Data Structures

```rust
struct Backend {
    id: String,              // UUID
    name: String,            // Human-readable name
    url: String,             // Base URL (e.g., "http://localhost:11434")
    backend_type: BackendType,
    status: BackendStatus,
    last_health_check: DateTime<Utc>,
    last_error: Option<String>,
    models: Vec<Model>,
    priority: i32,           // Lower = prefer
    pending_requests: u32,   // Current in-flight requests
    total_requests: u64,     // Lifetime total
    avg_latency_ms: u32,     // Rolling average
    discovery_source: DiscoverySource,
    metadata: HashMap<String, String>,
}

struct Model {
    id: String,              // Model identifier (e.g., "llama3:70b")
    name: String,            // Display name
    context_length: u32,     // Max context window
    supports_vision: bool,
    supports_tools: bool,
    supports_json_mode: bool,
    max_output_tokens: Option<u32>,
}

enum BackendType {
    Ollama,
    VLLM,
    LlamaCpp,
    Exo,
    OpenAI,
    Generic,
}

enum BackendStatus {
    Healthy,
    Unhealthy,
    Unknown,
    Draining,  // Healthy but not accepting new requests
}

enum DiscoverySource {
    Static,   // From config file
    MDNS,     // Auto-discovered via mDNS
    Manual,   // Added via CLI at runtime
}
```

### Operations

| Operation | Description |
|-----------|-------------|
| `add_backend(backend)` | Add new backend to registry |
| `remove_backend(id)` | Remove backend by ID |
| `get_backend(id)` | Get single backend |
| `get_all_backends()` | List all backends |
| `get_healthy_backends()` | Filter to healthy only |
| `get_backends_for_model(model)` | Find backends with model |
| `update_status(id, status)` | Update health status |
| `update_models(id, models)` | Update model list |
| `increment_pending(id)` | Track in-flight request |
| `decrement_pending(id)` | Request completed |

### Acceptance Criteria
- [x] Thread-safe access (DashMap)
- [x] Fast lookup by model name (indexed)
- [x] Survives concurrent read/write
- [x] Serializable to JSON for debugging

---

## F03: Health Checker

### Overview
Background service that periodically checks backend health.

### Health Check Flow

```
1. Every N seconds (default 30):
   for each backend in registry:
     a. Send health check request
     b. Parse response (models list)
     c. Update registry status
     d. Log status changes

2. Health check request varies by backend type:
   - Ollama: GET /api/tags
   - vLLM: GET /v1/models
   - llama.cpp: GET /health
   - Generic: GET /v1/models

3. Status transitions:
   Unknown → Healthy (1 success)
   Unknown → Unhealthy (1 failure)
   Healthy → Unhealthy (3 consecutive failures)
   Unhealthy → Healthy (2 consecutive successes)
```

### Configuration

```toml
[health_check]
enabled = true
interval_seconds = 30
timeout_seconds = 5
failure_threshold = 3
recovery_threshold = 2
```

### Model Parsing

**Ollama /api/tags:**
```json
{
  "models": [
    {
      "name": "llama3:70b",
      "size": 40000000000,
      "details": {
        "parameter_size": "70B",
        "quantization_level": "Q4_0"
      }
    }
  ]
}
```

**OpenAI-compatible /v1/models:**
```json
{
  "data": [
    {
      "id": "llama3-70b",
      "object": "model"
    }
  ]
}
```

### Acceptance Criteria
- [x] Checks all backends periodically
- [x] Updates registry on status change
- [x] Logs health transitions
- [x] Parses model lists from different backend types
- [x] Handles timeouts gracefully
- [x] Staggered checks (avoid thundering herd)

---

## F04: CLI and Configuration

### Overview
Command-line interface and configuration file support.

### Commands

```bash
# Start server
nexus serve [OPTIONS]
  --config, -c <FILE>     Config file (default: nexus.toml)
  --port, -p <PORT>       Listen port (default: 8000)
  --host <HOST>           Listen address (default: 0.0.0.0)
  --log-level <LEVEL>     Log level (default: info)
  --no-discovery          Disable mDNS discovery

# List backends
nexus backends [OPTIONS]
  --json                  Output as JSON
  --status <STATUS>       Filter by status

# Add backend manually
nexus backends add <URL> [OPTIONS]
  --name <NAME>           Display name
  --type <TYPE>           Backend type
  --priority <N>          Routing priority

# Remove backend
nexus backends remove <ID>

# List models
nexus models [OPTIONS]
  --json                  Output as JSON
  --backend <ID>          Filter by backend

# Check health
nexus health [OPTIONS]
  --json                  Output as JSON

# Generate config
nexus config init [OPTIONS]
  --output, -o <FILE>     Output file (default: nexus.toml)

# Version
nexus --version
```

### Example Outputs

```bash
$ nexus backends
┌──────────────┬────────────────────────────┬─────────┬──────────┬────────┐
│ Name         │ URL                        │ Type    │ Status   │ Models │
├──────────────┼────────────────────────────┼─────────┼──────────┼────────┤
│ local-ollama │ http://localhost:11434     │ Ollama  │ Healthy  │ 3      │
│ gpu-server   │ http://192.168.1.100:8000  │ vLLM    │ Healthy  │ 1      │
│ pi-cluster   │ http://192.168.1.50:52415  │ Exo     │ Unhealthy│ 0      │
└──────────────┴────────────────────────────┴─────────┴──────────┴────────┘

$ nexus models
┌──────────────┬─────────────────┬─────────┬────────┬───────┐
│ Model        │ Backend         │ Context │ Vision │ Tools │
├──────────────┼─────────────────┼─────────┼────────┼───────┤
│ llama3:70b   │ local-ollama    │ 8192    │ No     │ Yes   │
│ llama3:70b   │ gpu-server      │ 8192    │ No     │ Yes   │
│ mistral:7b   │ local-ollama    │ 32768   │ No     │ No    │
│ qwen2:72b    │ local-ollama    │ 131072  │ No     │ Yes   │
└──────────────┴─────────────────┴─────────┴────────┴───────┘

$ nexus health
Status: Healthy
Uptime: 2h 34m
Backends: 2/3 healthy
Models: 4 available

Backends:
  ✓ local-ollama (3 models, 45ms avg)
  ✓ gpu-server (1 model, 12ms avg)
  ✗ pi-cluster (connection refused)
```

### Acceptance Criteria
- [x] `serve` command starts server
- [x] `backends` lists all backends
- [x] `models` lists all models
- [x] `health` shows status
- [x] Config file loads correctly
- [x] Environment variables override config
- [x] CLI args override everything

---

## F05: mDNS Discovery

### Overview
Automatically discover LLM backends on local network.

### Supported Service Types

| Service Type | Backend Type | Notes |
|--------------|--------------|-------|
| `_ollama._tcp.local` | Ollama | Ollama advertises this |
| `_llm._tcp.local` | Generic | Proposed standard |
| `_http._tcp.local` | Generic | With TXT hints |

### Discovery Flow

```
1. On startup:
   - Start mDNS browser for each service type
   - Register for service events

2. On ServiceResolved:
   - Extract IP, port from SRV record
   - Extract metadata from TXT records
   - Create Backend struct
   - Add to registry
   - Trigger immediate health check

3. On ServiceRemoved:
   - Mark backend status as Unknown
   - Start grace period timer
   - If not seen again, remove from registry

4. Continuous operation:
   - Keep browsing for changes
   - Handle network changes gracefully
```

### TXT Record Parsing

```
# Ollama
version=0.1.0

# Proposed LLM standard
type=vllm
api_path=/v1
version=1.0.0
models=llama3:70b,mistral:7b
```

### Acceptance Criteria
- [x] Discovers Ollama instances automatically
- [x] Handles service appearing/disappearing
- [x] Grace period before removal
- [x] Works on macOS, Linux, Windows
- [x] Graceful fallback if mDNS unavailable

---

## F06: Intelligent Router

### Overview
Select the best backend for each request based on requirements.

### Routing Algorithm

```python
def select_backend(request):
    # 1. Extract requirements
    requirements = extract_requirements(request)
    # - model name
    # - estimated tokens
    # - needs_vision (has image_url in messages)
    # - needs_tools (has tools array)
    # - needs_json_mode (response_format.type == "json_object")
    
    # 2. Find candidates
    candidates = registry.get_backends_for_model(requirements.model)
    
    # 3. Filter by health
    candidates = [b for b in candidates if b.status == Healthy]
    
    # 4. Filter by capabilities
    candidates = [b for b in candidates if meets_requirements(b, requirements)]
    
    # 5. Check aliases if no candidates
    if not candidates and requirements.model in aliases:
        requirements.model = aliases[requirements.model]
        return select_backend(request)  # Retry with alias
    
    # 6. Score and select
    scores = [(score(b, requirements), b) for b in candidates]
    return max(scores, key=lambda x: x[0])[1]

def score(backend, requirements):
    # Base: priority (lower is better, so invert)
    score = 100 - backend.priority
    
    # Load factor (fewer pending requests is better)
    load_penalty = min(backend.pending_requests * 5, 50)
    score -= load_penalty
    
    # Latency factor (lower latency is better)
    latency_penalty = min(backend.avg_latency_ms / 20, 30)
    score -= latency_penalty
    
    return score
```

### Capability Detection

| Requirement | Detection Method |
|-------------|------------------|
| Vision | `messages[*].content[*].type == "image_url"` |
| Tools | `tools` array present and non-empty |
| JSON Mode | `response_format.type == "json_object"` |
| Context Length | Estimate: `sum(len(m.content) for m in messages) / 4` |

### Acceptance Criteria
- [x] Matches model by name
- [x] Filters by capabilities
- [x] Scores by priority, load, latency
- [x] Falls back to aliases
- [x] Returns error if no backend available

---

## F07: Model Aliases

### Overview
Map common model names to available local models.

### Configuration

```toml
[routing.aliases]
"gpt-4" = "llama3:70b"
"gpt-4-turbo" = "llama3:70b"
"gpt-3.5-turbo" = "mistral:7b"
"claude-3-opus" = "qwen2:72b"
"claude-3-sonnet" = "llama3:70b"
```

### Behavior

1. Request comes in for model "gpt-4"
2. Check if any backend has "gpt-4" → No
3. Check aliases: "gpt-4" → "llama3:70b"
4. Route to backend with "llama3:70b"
5. Response model field shows "gpt-4" (what client requested)

### Acceptance Criteria
- [x] Aliases configured in config file
- [x] Transparent to client (sees requested model name)
- [x] Logged at DEBUG level
- [x] Circular alias detection at config load
- [x] Max 3 levels of chaining
- [x] Direct matches preferred over aliases

---

## F08: Fallback Chains

### Overview
Automatic fallback to alternative models when primary fails.

### Configuration

```toml
[routing.fallbacks]
"llama3:70b" = ["qwen2:72b", "mixtral:8x7b", "llama3:8b"]
"gpt-4" = ["llama3:70b", "mistral:7b"]
```

### Behavior

1. Request for "llama3:70b"
2. All backends with "llama3:70b" are unhealthy
3. Check fallback chain: ["qwen2:72b", "mixtral:8x7b", "llama3:8b"]
4. Try "qwen2:72b" → Available! Route there
5. Log fallback at WARN level

### Acceptance Criteria
- [x] Fallback chains configurable per model
- [x] Tries each fallback in order
- [x] Logs fallback usage at WARN level
- [x] Returns 503 if all fallbacks exhausted
- [x] X-Nexus-Fallback-Model header indicates actual model
- [x] Response model field shows requested model

---

## F09: Request Metrics (P2)

### Overview
Track request statistics for observability.

### Metrics

```
# Counters
nexus_requests_total{model, backend, status}
nexus_errors_total{type}

# Histograms
nexus_request_duration_seconds{model, backend}
nexus_backend_latency_seconds{backend}

# Gauges
nexus_backends_healthy
nexus_backends_total
nexus_pending_requests{backend}
```

### Endpoints

```
GET /metrics          # Prometheus format
GET /v1/stats         # JSON format
```

### Acceptance Criteria
- [ ] Prometheus-compatible /metrics endpoint
- [ ] JSON stats at /v1/stats
- [ ] Request duration tracking
- [ ] Error rate tracking

---

## F10: Web Dashboard (P2)

### Overview
Simple web UI for monitoring.

### Features

- Backend status overview
- Model availability matrix
- Request history (last 100)
- Real-time updates (WebSocket)

### Technology

- Embedded static files (rust-embed)
- Vanilla JS (no framework dependencies)
- Tailwind CSS for styling

### Acceptance Criteria
- [ ] Shows backend status
- [ ] Shows model list
- [ ] Auto-refreshes
- [ ] Works without JavaScript (graceful degradation)

---

## Implementation Order

### v0.1: Foundation ✅ Released
1. F02: Backend Registry ✅
2. F03: Health Checker ✅
3. F01: Core API Gateway ✅
4. F04: CLI and Configuration ✅
5. F05: mDNS Discovery ✅
6. F06: Intelligent Router ✅
7. F07: Model Aliases ✅
8. F08: Fallback Chains ✅

### v0.2: Observability ✅ Released
9. F09: Request Metrics ✅
10. F10: Web Dashboard ✅
11. F11: Structured Request Logging ✅

### v0.3: Cloud Hybrid Gateway (Next)
12. F12: Cloud Backend Support
13. F13: Privacy Zones & Capability Tiers
14. F14: Inference Budget Management

### v0.4: Intelligence
15. F15: Speculative Router
16. F16: Quality Tracking & Backend Profiling
17. F17: Embeddings API
18. F18: Request Queuing & Prioritization

### v0.5: Orchestration
19. F19: Pre-warming & Fleet Intelligence
20. F20: Model Lifecycle Management
21. F21: Multi-Tenant Support
22. F22: Rate Limiting

---

## v0.2 Features

---

## F11: Structured Request Logging (v0.2)

### Overview
Structured, queryable logs for every request passing through Nexus.

### Log Fields

```json
{
  "timestamp": "2026-02-11T12:00:00Z",
  "request_id": "req_abc123",
  "model": "llama3:70b",
  "backend": "gpu-node-1",
  "backend_type": "ollama",
  "status": 200,
  "latency_ms": 1234,
  "tokens_prompt": 150,
  "tokens_completion": 200,
  "stream": true,
  "route_reason": "capability-match"
}
```

### Requirements
- JSON and human-readable output formats (via `tracing`)
- Configurable log level per component
- Request correlation IDs across retry/failover chains
- No sensitive data (message content) in logs by default

### Acceptance Criteria
- [ ] Every request produces a structured log entry
- [ ] Request correlation ID tracks retries and failovers
- [ ] Log format is configurable (JSON / pretty)
- [ ] Message content is never logged by default

---

## v0.3 Features

---

## F12: Cloud Backend Support (v0.3)

### Overview
Register cloud LLM APIs (OpenAI, Anthropic, Google) as backends alongside local inference servers. Includes the Nexus-Transparent Protocol for routing observability.

### Cloud Backend Configuration

```toml
[[backends]]
name = "openai-gpt4"
url = "https://api.openai.com"
backend_type = "openai"
api_key_env = "OPENAI_API_KEY"
zone = "cloud"
tier = 4

[[backends]]
name = "anthropic-claude"
url = "https://api.anthropic.com"
backend_type = "anthropic"
api_key_env = "ANTHROPIC_API_KEY"
zone = "cloud"
tier = 4
```

### Nexus-Transparent Protocol

Response headers added to every proxied response:

| Header | Values | Description |
|--------|--------|-------------|
| `X-Nexus-Backend` | Backend name | Which backend served the request |
| `X-Nexus-Backend-Type` | `local` \| `cloud` | Backend location |
| `X-Nexus-Route-Reason` | `capability-match` \| `capacity-overflow` \| `privacy-requirement` | Why this backend was chosen |
| `X-Nexus-Cost-Estimated` | `$0.0042` | Estimated cost (cloud backends only) |
| `X-Nexus-Privacy-Zone` | `restricted` \| `open` | Privacy zone of the backend used |

### Actionable Error Responses

```json
{
  "error": {
    "type": "nexus_routing_error",
    "code": "no_capable_backend",
    "message": "No healthy backend available for model 'gpt-4o'",
    "context": {
      "required_tier": 4,
      "available_backends": ["gpu-node-2 (busy)", "gpu-node-3 (loading)"],
      "eta_seconds": 15
    }
  }
}
```

### Acceptance Criteria
- [ ] Cloud backends can be registered via TOML config
- [ ] API keys loaded from environment variables (never in config files)
- [ ] X-Nexus-* response headers on all proxied responses
- [ ] Actionable error responses with context object
- [ ] Cloud backends participate in standard routing and failover
- [ ] Anthropic API translated to/from OpenAI format

---

## F13: Privacy Zones & Capability Tiers (v0.3)

### Overview
Structural enforcement of privacy boundaries and quality levels for routing decisions. Privacy is a backend property (admin-configured), not a request header.

### Privacy Zones

```toml
[[backends]]
name = "gpu-node-1"
zone = "restricted"    # Local-only, never receives cloud-overflow traffic

[[backends]]
name = "openai-gpt4"
zone = "open"          # Cloud, can receive overflow from open-zone conversations
```

**Zone enforcement rules:**
1. `restricted` backends never receive requests that could overflow to `open` backends
2. If a conversation started on a `restricted` backend and that backend fails → 503 with `Retry-After`, not silent failover to cloud
3. Cross-zone overflow: fresh context only (no conversation history forwarded) or block entirely
4. Backend affinity (sticky routing) for `restricted` conversations

### Capability Tiers

```toml
[[backends]]
name = "gpu-node-1"
tier = 2
capabilities = { reasoning = 3, coding = 2, context_window = 32768, vision = false }

[[backends]]
name = "openai-gpt4"
tier = 4
capabilities = { reasoning = 5, coding = 4, context_window = 128000, vision = true }
```

**Tier enforcement rules:**
1. Overflow only to same-tier-or-higher backends (never silently downgrade)
2. Client control via request headers:
   - `X-Nexus-Strict: true` — only the exact requested model
   - `X-Nexus-Flexible: true` — tier-equivalent alternatives acceptable
3. If no suitable backend available → 503 with tier requirement info

### Acceptance Criteria
- [ ] Privacy zones enforced at routing layer (backend property)
- [ ] Restricted backends never receive cloud-overflow traffic
- [ ] Capability tiers prevent silent quality downgrades
- [ ] Client can opt into strict or flexible routing
- [ ] 503 responses include tier/zone context

---

## F14: Inference Budget Management (v0.3)

### Overview
Cost-aware routing with graceful degradation. Includes a tokenizer registry for audit-grade token counting.

### Budget Configuration

```toml
[budget]
monthly_limit = 100.00        # USD
soft_limit_percent = 80       # Shift to local-preferred at 80%
hard_limit_action = "local-only"  # Options: "local-only", "queue", "reject"

[budget.pricing]
openai-gpt4 = { prompt = 0.03, completion = 0.06 }  # per 1K tokens
anthropic-claude = { prompt = 0.015, completion = 0.075 }
```

### Tokenizer Registry

| Backend/Model | Tokenizer | Source |
|---------------|-----------|--------|
| OpenAI (GPT-4, GPT-4o) | `o200k_base` | `tiktoken-rs` |
| OpenAI (GPT-3.5) | `cl100k_base` | `tiktoken-rs` |
| Anthropic (Claude) | `cl100k_base` (approximate) | `tiktoken-rs` |
| Llama models | SentencePiece | `tokenizers` crate |
| Unknown | 1.15x conservative multiplier | Flagged "estimated" |

### Degradation Behavior

| Budget Level | Routing Behavior | Metrics |
|-------------|-----------------|---------|
| 0-80% | Normal (local-first, cloud overflow) | `budget_usage_percent` gauge |
| 80-100% | Local-preferred (cloud only if no local option) | Warning emitted |
| 100%+ | `hard_limit_action` applies | Alert emitted |

### Acceptance Criteria
- [ ] Per-request cost estimation using tokenizer registry
- [ ] Soft limit triggers local-preferred routing
- [ ] Hard limit triggers configurable action (never hard-cut production)
- [ ] Budget metrics exposed via Prometheus
- [ ] Token counts are per-backend-tokenizer, not generic estimates
- [ ] Unknown tokenizers use conservative multiplier with "estimated" flag

---

## v0.4 Features

---

## F15: Speculative Router (v0.4)

### Overview
Request-content-aware routing using JSON payload inspection. Zero ML — sub-millisecond decisions based on request structure.

### Routing Signals (extracted from request JSON)

| Signal | Source | Routing Decision |
|--------|--------|-----------------|
| Prompt token count | `messages` array | Route to backend with sufficient context window |
| Image content | `messages[].content[].type == "image_url"` | Route to vision-capable backend |
| Tool definitions | `tools` array present | Route to tool-use-capable backend |
| Response format | `response_format.type == "json_object"` | Route to JSON-mode-capable backend |
| Stream flag | `stream: true` | Prefer backends with efficient streaming |

### Performance Target
- Payload inspection: < 0.5ms
- No ML inference, no embedding computation
- Consider classifier only for future "auto-tier" feature, and only for prompts > 10K tokens

### Acceptance Criteria
- [ ] Routes based on detected request capabilities (vision, tools, JSON mode)
- [ ] Token count estimation from message array (pre-tokenization fast path)
- [ ] Routing overhead remains < 1ms total
- [ ] No external dependencies for routing decisions

---

## F16: Quality Tracking & Backend Profiling (v0.4)

### Overview
Build performance profiles for each model+backend combination to inform routing decisions over time.

### Tracked Metrics

| Metric | Granularity | Purpose |
|--------|-------------|---------|
| Response latency (P50, P95, P99) | Per model+backend | Latency-aware routing |
| Tokens per second | Per model+backend | Throughput-aware routing |
| Error rate | Per model+backend | Reliability scoring |
| Time to first token (streaming) | Per model+backend | Streaming quality |

### Acceptance Criteria
- [ ] Rolling window statistics (last 1h, 24h) per model+backend
- [ ] Quality scores feed into router scoring algorithm
- [ ] Degraded backends automatically deprioritized
- [ ] Metrics exposed via Prometheus and /v1/stats

---

## F17: Embeddings API (v0.4)

### Overview
Support the OpenAI Embeddings API across backends.

### Endpoint

```
POST /v1/embeddings
```

### Acceptance Criteria
- [ ] Route embedding requests to capable backends
- [ ] Support batch embedding requests
- [ ] OpenAI-compatible request/response format

---

## F18: Request Queuing & Prioritization (v0.4)

### Overview
When all backends are busy, queue requests with configurable timeout and priority levels rather than immediately returning 503.

### Configuration

```toml
[queuing]
enabled = true
max_queue_size = 100
default_timeout_seconds = 30
priority_header = "X-Nexus-Priority"  # "high", "normal", "low"
```

### Behavior
- Queue fills → oldest low-priority requests dropped first
- Timeout exceeded → 503 with `eta_seconds` if backend is loading
- Tier-equivalent fallback attempted before queuing

### Acceptance Criteria
- [ ] Bounded queue with configurable max size
- [ ] Priority levels via request header
- [ ] Timeout with actionable 503 (includes ETA)
- [ ] Queue depth exposed in metrics

---

## v0.5 Features

---

## F19: Pre-warming & Fleet Intelligence (v0.5)

### Overview
Predict model demand and proactively load models on idle nodes before capacity is exhausted. Suggestion-first: Nexus recommends, admin/policy approves.

### Design Constraints (from Constitution)
- Never evict a hot model for a prediction
- Use idle capacity only
- VRAM headroom awareness: only pre-warm if > configurable % headroom
- Recommendation system, not autonomous actor (v0.5)

### Data Sources
- Request history patterns (time of day, model popularity)
- Backend VRAM usage (from Ollama `/api/ps` and similar endpoints)
- Current model loading state

### Acceptance Criteria
- [ ] Tracks model request frequency over time
- [ ] Reports pre-warming recommendations via API/logs
- [ ] Respects VRAM headroom budget
- [ ] Never disrupts active model serving

---

## F20: Model Lifecycle Management (v0.5)

### Overview
Control model loading, unloading, and migration across the fleet via Nexus API.

### Acceptance Criteria
- [ ] API to trigger model load/unload on specific backends
- [ ] Model migration (unload from A, load on B)
- [ ] Status tracking for loading operations
- [ ] Integrates with pre-warming recommendations (F19)

---

## F21: Multi-Tenant Support (v0.5)

### Overview
API key-based authentication with per-tenant quotas, model access control, and usage tracking.

### Acceptance Criteria
- [ ] API key authentication (optional, off by default)
- [ ] Per-tenant usage tracking and quotas
- [ ] Model access control lists per tenant
- [ ] Usage reporting via metrics and API

---

## F22: Rate Limiting (v0.5)

### Overview
Per-backend and per-tenant rate limiting to prevent resource exhaustion.

### Acceptance Criteria
- [ ] Per-backend request rate limits
- [ ] Per-tenant rate limits
- [ ] Token bucket algorithm with burst support
- [ ] 429 responses with `Retry-After` header

---

## F23: Management UI (v0.5)

### Overview
A full-featured web-based management interface that provides everything the CLI offers through an interactive UI. Evolves the existing monitoring dashboard (F10) into a complete control plane with backend management, model lifecycle, configuration editing, and routing controls — all embedded in the single Nexus binary.

### Architecture
- **Hybrid same-repo approach**: Frontend source in `ui/` with its own `package.json` and build pipeline
- **Framework**: Modern JS framework (React, Vue, or Svelte — TBD during spec phase)
- **Embedding**: CI builds frontend to static assets, `rust-embed` bundles into the binary
- **Development**: `npm run dev` with hot reload, proxying API calls to a running Nexus instance
- **Distribution**: Single binary via crates.io, GitHub releases, and Docker — zero extra setup

### Capabilities
| Area | Features |
|------|----------|
| **Monitoring** (migrate from F10) | System summary, backend status cards, model matrix, request history, real-time WebSocket updates |
| **Backend Management** | Add/remove/edit backends, health check controls, priority adjustment, drain/undrain |
| **Model Management** | Browse models across backends, load/unload models (F20), view capabilities |
| **Configuration** | View/edit TOML config, alias and fallback chain management, routing strategy selection |
| **Routing** | Visual routing strategy selector, alias editor, fallback chain builder |
| **Observability** | Metrics charts (latency, throughput), log viewer with filtering, Prometheus integration status |

### Acceptance Criteria
- [ ] All CLI capabilities accessible through the UI
- [ ] Existing F10 dashboard migrated into monitoring tab
- [ ] Backend CRUD operations (add, remove, edit, drain)
- [ ] Model alias and fallback chain management
- [ ] Routing strategy configuration
- [ ] Real-time updates via WebSocket (existing F10 infrastructure)
- [ ] Responsive design (desktop and tablet)
- [ ] Embedded in single binary via `rust-embed` (zero-config)
- [ ] Works across all distribution channels (crates.io, Docker, GitHub releases)
- [ ] Frontend dev workflow with hot reload (`npm run dev` with API proxy)
- [ ] No-JS fallback for basic monitoring (existing F10 behavior)
