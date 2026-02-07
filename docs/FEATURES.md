# Nexus - Feature Specifications

Detailed specifications for each feature in the Nexus LLM Orchestrator.

---

## Feature Index

| ID | Feature | Priority | Status | Spec |
|----|---------|----------|--------|------|
| F01 | Core API Gateway | P0 | ✅ Complete | [specs/004-api-gateway](../specs/004-api-gateway/) |
| F02 | Backend Registry | P0 | ✅ Complete | [specs/001-backend-registry](../specs/001-backend-registry/) |
| F03 | Health Checker | P0 | ✅ Complete | [specs/002-health-checker](../specs/002-health-checker/) |
| F04 | CLI and Configuration | P0 | ✅ Complete | [specs/003-cli-configuration](../specs/003-cli-configuration/) |
| F05 | mDNS Discovery | P1 | ✅ Complete | [specs/005-mdns-discovery](../specs/005-mdns-discovery/) |
| F06 | Intelligent Router | P1 | Planned | - |
| F07 | Model Aliases | P1 | Planned | - |
| F08 | Fallback Chains | P1 | Planned | - |
| F09 | Request Metrics | P2 | Planned | - |
| F10 | Web Dashboard | P2 | Planned | - |

### Current Status

- **MVP (P0)**: ✅ Complete (4/4 features)
- **Phase 2 (P1)**: 🚧 In Progress (1/4 features complete)
- **Tests**: 258 passing

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
- [ ] Discovers Ollama instances automatically
- [ ] Handles service appearing/disappearing
- [ ] Grace period before removal
- [ ] Works on macOS, Linux, Windows
- [ ] Graceful fallback if mDNS unavailable

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
- [ ] Matches model by name
- [ ] Filters by capabilities
- [ ] Scores by priority, load, latency
- [ ] Falls back to aliases
- [ ] Returns error if no backend available

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
- [ ] Aliases configured in config file
- [ ] Transparent to client (sees requested model name)
- [ ] Logged at DEBUG level

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
- [ ] Fallback chains in config
- [ ] Tries each fallback in order
- [ ] Logs when fallback used
- [ ] Returns 503 if all fallbacks exhausted

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

### Phase 1: MVP ✅ Complete
1. F02: Backend Registry ✅
2. F03: Health Checker ✅
3. F01: Core API Gateway ✅
4. F04: CLI and Configuration ✅

### Phase 2: Discovery (Next)
5. F05: mDNS Discovery

### Phase 3: Intelligence
6. F06: Intelligent Router
7. F07: Model Aliases
8. F08: Fallback Chains

### Phase 4: Polish
9. F04: CLI and Configuration (complete)
10. F09: Request Metrics
11. F10: Web Dashboard
