# Nexus - Spec-Kit Prompts

This document contains the prompts to use with [GitHub spec-kit](https://github.com/github/spec-kit) for developing the Nexus LLM Orchestrator.

---

## How to Use This Document

1. Install spec-kit: `npm install -g @github/spec-kit` (or use npx)
2. Initialize a new project: `spec-kit init nexus`
3. Run each phase in order using the prompts below
4. The output from each phase feeds into the next

---

## Phase 1: Constitution

The constitution defines the project's core identity, principles, and constraints.

### Command
```bash
spec-kit constitute
```

### Prompt

```
I want to build Nexus, a distributed LLM model serving orchestrator.

## What It Is
A lightweight Rust service that acts as a unified API gateway for multiple LLM inference backends (Ollama, vLLM, llama.cpp, exo, etc.). It auto-discovers backends on the local network, tracks their available models and capabilities, and intelligently routes incoming requests to the best available model.

## Core Principles
1. **Zero Configuration**: Should work out of the box with mDNS discovery. Just run it.
2. **Single Binary**: Rust-based, no runtime dependencies, easy to deploy.
3. **OpenAI-Compatible**: Exposes standard OpenAI Chat Completions API.
4. **Backend Agnostic**: Works with any OpenAI-compatible backend.
5. **Intelligent Routing**: Routes based on model capabilities (context length, vision, tools), not just load.
6. **Resilient**: Automatic failover when backends go down.
7. **Local-First**: Designed for home labs and small teams, not cloud.

## Technical Constraints
- Language: Rust
- HTTP Framework: Axum
- Async Runtime: Tokio
- Discovery: mDNS (mdns-sd crate)
- No database required (in-memory state)
- Target platforms: Linux, macOS, Windows

## What It Is NOT
- Not an inference engine (doesn't run models)
- Not a model manager (doesn't download/convert models)
- Not multi-tenant (single user/team assumed)
- Not a replacement for exo (complements it for routing)

## Target Users
- Home lab enthusiasts with multiple GPU machines
- Small teams pooling GPU resources
- Developers wanting unified access to local models

## Success Criteria
- Install to first request: < 5 minutes
- Routing overhead: < 5ms
- Memory usage: < 50MB
- Works with Claude Code and Continue.dev out of the box
```

---

## Phase 2: Specify (Per Feature)

Run `spec-kit specify` for each major feature. Below are the prompts for each.

---

### Feature 2.1: Core API Gateway

```bash
spec-kit specify "Core API Gateway"
```

#### Prompt

```
Specify the Core API Gateway feature for Nexus.

## Feature Description
Implement an OpenAI-compatible HTTP API that receives chat completion requests and forwards them to configured backends.

## Requirements

### Functional Requirements
1. Expose POST /v1/chat/completions endpoint
2. Accept standard OpenAI ChatCompletion request format
3. Support both streaming (SSE) and non-streaming responses
4. Forward requests to a configured backend
5. Expose GET /v1/models to list available models
6. Expose GET /health for health checks

### Non-Functional Requirements
1. Handle concurrent requests (at least 100 simultaneous)
2. Request timeout configurable (default 5 minutes)
3. Graceful shutdown on SIGTERM
4. Structured logging with tracing

### API Contract

POST /v1/chat/completions
- Request: OpenAI ChatCompletionRequest
- Response: OpenAI ChatCompletionResponse (or SSE stream)
- Headers: Authorization (optional, passed through to backend)

GET /v1/models
- Response: OpenAI ModelsResponse listing all available models

GET /health
- Response: { "status": "ok", "backends": [...] }

### Technical Approach
- Use Axum for HTTP server
- Use reqwest for backend HTTP client
- Use async-stream for SSE forwarding
- Configuration via TOML file or environment variables

### Edge Cases
1. Backend timeout → Return 504 Gateway Timeout
2. Backend unreachable → Return 502 Bad Gateway
3. Invalid request → Return 400 Bad Request with details
4. Model not found → Return 404 with available models hint

### Testing Strategy
1. Unit tests for request/response parsing
2. Integration tests with mock backend
3. Load test with 100 concurrent requests

### Dependencies
- axum, tokio, reqwest, serde, serde_json, tracing, toml
```

---

### Feature 2.2: Backend Registry

```bash
spec-kit specify "Backend Registry"
```

#### Prompt

```
Specify the Backend Registry feature for Nexus.

## Feature Description
An in-memory registry that tracks all known backends, their available models, capabilities, and health status.

## Requirements

### Functional Requirements
1. Store backend information (URL, type, models, status)
2. Store model information (name, context_length, capabilities)
3. Support adding/removing backends at runtime
4. Track backend health status (healthy, unhealthy, unknown)
5. Provide query methods:
   - Get all backends
   - Get backends by model name
   - Get backends by capability (e.g., supports_vision)
   - Get healthy backends only

### Data Structures

Backend {
  id: String (UUID)
  name: String
  url: String
  backend_type: Enum (Ollama, VLLM, LlamaCpp, Exo, OpenAI, Generic)
  status: Enum (Healthy, Unhealthy, Unknown)
  last_health_check: Timestamp
  models: Vec<Model>
  priority: i32 (lower = prefer)
  metadata: HashMap<String, String>
}

Model {
  id: String
  name: String
  context_length: u32
  supports_vision: bool
  supports_tools: bool
  supports_json_mode: bool
  max_tokens: Option<u32>
  backend_id: String
}

ModelCapabilities {
  min_context_length: Option<u32>
  requires_vision: bool
  requires_tools: bool
  requires_json_mode: bool
}

### Thread Safety
- Registry must be thread-safe (Arc<RwLock<...>>)
- Read-heavy workload expected (many reads, few writes)
- Consider using dashmap for concurrent access

### Technical Approach
- Pure Rust data structures, no database
- Serde serialization for config loading
- Clone-on-read for safe iteration

### Testing Strategy
1. Unit tests for all query methods
2. Concurrent access tests
3. Property-based tests for data integrity
```

---

### Feature 2.3: Health Checker

```bash
spec-kit specify "Health Checker"
```

#### Prompt

```
Specify the Health Checker feature for Nexus.

## Feature Description
A background service that periodically checks backend health and updates the registry.

## Requirements

### Functional Requirements
1. Periodically ping each backend (configurable interval, default 30s)
2. Update backend status in registry based on response
3. Fetch available models from each backend
4. Detect new models added to backends
5. Support different health check methods per backend type:
   - Ollama: GET /api/tags
   - vLLM: GET /v1/models
   - llama.cpp: GET /health
   - Generic: GET /v1/models or configurable

### Health Check Logic
1. Send health request with 5s timeout
2. If success:
   - Mark backend Healthy
   - Update model list
   - Reset failure counter
3. If failure:
   - Increment failure counter
   - If failures >= 3: Mark Unhealthy
   - Keep last known models (don't remove immediately)
4. If unhealthy backend recovers:
   - Require 2 consecutive successes to mark Healthy

### Configuration
health_check:
  interval_seconds: 30
  timeout_seconds: 5
  failure_threshold: 3
  recovery_threshold: 2

### Non-Functional Requirements
1. Health checks should not block request routing
2. Stagger checks to avoid thundering herd
3. Log health transitions at INFO level

### Edge Cases
1. Backend returns 200 but invalid response → Treat as unhealthy
2. Backend very slow but responds → Healthy but note latency
3. DNS resolution fails → Unhealthy
4. TLS certificate error → Unhealthy with specific error

### Testing Strategy
1. Unit tests with mock HTTP responses
2. Integration test with actual Ollama instance
3. Test failure/recovery state transitions
```

---

### Feature 2.4: mDNS Discovery

```bash
spec-kit specify "mDNS Discovery"
```

#### Prompt

```
Specify the mDNS Discovery feature for Nexus.

## Feature Description
Automatically discover LLM backends on the local network using mDNS/Bonjour.

## Requirements

### Functional Requirements
1. Browse for services advertising LLM backends
2. Support multiple service types:
   - _ollama._tcp.local (Ollama)
   - _llm._tcp.local (Generic LLM, proposed standard)
3. Extract connection info from TXT records
4. Auto-add discovered backends to registry
5. Auto-remove backends when they disappear
6. Support manual override (don't remove if manually configured)

### mDNS Service Format

Service: _ollama._tcp.local
TXT Records:
  - version=0.1.0
  - models=llama3,mistral,qwen

Service: _llm._tcp.local (proposed)
TXT Records:
  - type=vllm|llamacpp|exo|generic
  - api_path=/v1
  - version=1.0.0

### Discovery Flow
1. On startup, browse for known service types
2. When service found:
   - Parse address/port from SRV record
   - Parse metadata from TXT records
   - Add to registry if not exists
   - Trigger immediate health check
3. When service removed:
   - Mark as Unhealthy
   - Remove after grace period (60s) if not seen again
4. Continuous browsing (not one-shot)

### Configuration
discovery:
  enabled: true
  service_types:
    - "_ollama._tcp.local"
    - "_llm._tcp.local"
  grace_period_seconds: 60

### Technical Approach
- Use mdns-sd crate for cross-platform mDNS
- Run discovery in background task
- Send updates via channel to main registry

### Edge Cases
1. Multiple instances same IP different ports → Treat as separate
2. Service disappears then reappears → Keep model cache
3. mDNS not available (Docker, etc.) → Graceful fallback to static config
4. Conflicting manual and discovered → Manual takes precedence

### Testing Strategy
1. Unit tests for TXT record parsing
2. Integration tests require local mDNS (optional, skip in CI)
3. Mock mDNS responses for deterministic tests
```

---

### Feature 2.5: Intelligent Router

```bash
spec-kit specify "Intelligent Router"
```

#### Prompt

```
Specify the Intelligent Router feature for Nexus.

## Feature Description
Route incoming requests to the best available backend based on model requirements, capabilities, and load.

## Requirements

### Functional Requirements
1. Select backend for incoming request based on:
   - Requested model name (exact match or alias)
   - Required capabilities (derived from request)
   - Backend health status
   - Current load (pending requests)
   - Configured priority
2. Support model aliases (e.g., "gpt-4" → route to best local model)
3. Support fallback chains (try X, then Y, then Z)
4. Support request-based routing rules

### Routing Algorithm

```
1. Parse request to extract:
   - model_name
   - required_capabilities (vision, tools, min_context)
   
2. Find candidate backends:
   - Filter by model availability
   - Filter by health (Healthy only, or include Unknown if configured)
   - Filter by capabilities
   
3. If no candidates:
   - Check model aliases for alternatives
   - Check fallback chain
   - If still none: return 404

4. Score candidates:
   score = priority_weight * (1 / priority)
         + load_weight * (1 / (pending_requests + 1))
         + latency_weight * (1 / avg_latency_ms)
   
5. Select highest scoring candidate

6. If selected backend fails:
   - Remove from candidates
   - Retry with next best (up to max_retries)
```

### Capability Detection from Request
- Vision: messages contain image_url content
- Tools: tools array is present
- JSON mode: response_format.type == "json_object"
- Context: Estimate tokens from messages (rough: chars / 4)

### Configuration
routing:
  strategy: "smart"  # or "round_robin", "random", "priority_only"
  max_retries: 2
  weights:
    priority: 0.5
    load: 0.3
    latency: 0.2
  aliases:
    "gpt-4": "llama3:70b"
    "gpt-3.5-turbo": "mistral:7b"
  fallbacks:
    "llama3:70b": ["qwen2:72b", "mixtral:8x7b"]

### Non-Functional Requirements
1. Routing decision: < 1ms
2. No external calls during routing (use cached data)
3. Thread-safe (multiple concurrent routing decisions)

### Testing Strategy
1. Unit tests for scoring algorithm
2. Unit tests for capability detection
3. Integration tests with multiple mock backends
4. Chaos tests (backends failing mid-request)
```

---

### Feature 2.6: CLI and Configuration

```bash
spec-kit specify "CLI and Configuration"
```

#### Prompt

```
Specify the CLI and Configuration feature for Nexus.

## Feature Description
Command-line interface and configuration system for Nexus.

## Requirements

### CLI Commands

nexus serve [OPTIONS]
  Start the Nexus server
  --config, -c <FILE>     Config file path (default: nexus.toml)
  --port, -p <PORT>       Listen port (default: 8000)
  --host <HOST>           Listen host (default: 0.0.0.0)
  --log-level <LEVEL>     Log level (default: info)
  --no-discovery          Disable mDNS discovery

nexus backends
  List configured and discovered backends

nexus backends add <URL> [OPTIONS]
  Add a backend manually
  --name <NAME>           Display name
  --type <TYPE>           Backend type (ollama, vllm, llamacpp, generic)
  --priority <N>          Routing priority (lower = prefer)

nexus backends remove <ID|URL>
  Remove a backend

nexus models
  List all available models across backends

nexus health
  Show health status of all backends

nexus config init
  Generate example config file

nexus --version
nexus --help

### Configuration File (nexus.toml)

[server]
host = "0.0.0.0"
port = 8000
request_timeout_seconds = 300

[discovery]
enabled = true
service_types = ["_ollama._tcp.local", "_llm._tcp.local"]

[health_check]
interval_seconds = 30
timeout_seconds = 5

[routing]
strategy = "smart"
max_retries = 2

[routing.aliases]
"gpt-4" = "llama3:70b"
"gpt-3.5-turbo" = "mistral:7b"

[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
priority = 1

[[backends]]
name = "gpu-server"
url = "http://192.168.1.100:8000"
type = "vllm"
priority = 2

[logging]
level = "info"
format = "pretty"  # or "json"

### Environment Variables
NEXUS_CONFIG          Config file path
NEXUS_PORT            Listen port
NEXUS_LOG_LEVEL       Log level
NEXUS_DISCOVERY       Enable discovery (true/false)

### Technical Approach
- Use clap for CLI parsing
- Use config crate for layered config (file + env + args)
- Use toml for config serialization
- Pretty-print tables with comfy-table

### Testing Strategy
1. Unit tests for config parsing
2. CLI tests using assert_cmd
3. Test config precedence (env > file > defaults)
```

---

## Phase 3: Plan

Run after all features are specified.

```bash
spec-kit plan
```

### Prompt

```
Create an implementation plan for Nexus based on the specified features.

## Constraints
1. Solo developer
2. Part-time work (10-15 hours/week)
3. Rust experience: intermediate
4. Must have working MVP within 3 weeks

## Feature Priority
P0 (Must Have for MVP):
- Core API Gateway
- Backend Registry
- Health Checker
- CLI and Configuration (basic)

P1 (Required for v1.0):
- mDNS Discovery
- Intelligent Router

P2 (Nice to Have):
- Web dashboard
- Prometheus metrics
- Authentication

## Suggested Phases

### Phase 1: Foundation (Week 1)
- Project setup (cargo, CI, linting)
- Core data structures (Backend, Model, Registry)
- Basic config loading
- Health checker (minimal)

### Phase 2: API Gateway (Week 2)
- OpenAI-compatible endpoints
- Request forwarding
- Streaming support
- Error handling

### Phase 3: MVP Complete (Week 3)
- CLI commands
- Basic routing (round-robin)
- Documentation
- First release

### Phase 4: Discovery (Week 4-5)
- mDNS integration
- Auto-registration
- Service removal

### Phase 5: Smart Routing (Week 6-7)
- Capability detection
- Scoring algorithm
- Fallback chains

### Phase 6: Polish (Week 8-9)
- Performance optimization
- Comprehensive tests
- Documentation site
- Community feedback

## Risk Mitigation
1. mDNS complexity → Fallback to static config
2. Streaming edge cases → Test with real backends early
3. Scope creep → Strict MVP definition
```

---

## Phase 4: Tasks

Run after plan is created.

```bash
spec-kit tasks
```

### Prompt

```
Generate implementation tasks for the Nexus plan.

## Task Format
Each task should be:
- Small enough to complete in 1-4 hours
- Independently testable
- Clear acceptance criteria

## Week 1 Tasks

### Day 1-2: Project Setup
- [ ] Initialize Cargo project with workspace structure
- [ ] Set up GitHub repo with CI (cargo test, clippy, fmt)
- [ ] Add core dependencies (axum, tokio, serde, reqwest, tracing)
- [ ] Create basic project structure (src/lib.rs, src/main.rs, src/api/, src/registry/, etc.)
- [ ] Add LICENSE and README

### Day 3-4: Core Data Structures
- [ ] Define Backend struct with all fields
- [ ] Define Model struct with capabilities
- [ ] Define Registry trait and in-memory implementation
- [ ] Write unit tests for data structures
- [ ] Implement serde serialization

### Day 5-7: Config and Health
- [ ] Define config structs matching TOML schema
- [ ] Implement config loading (file + env)
- [ ] Create health check task
- [ ] Implement Ollama health check
- [ ] Implement generic OpenAI-compatible health check
- [ ] Write integration tests with mock HTTP

## Week 2 Tasks

### Day 1-2: HTTP Server
- [ ] Create Axum router with basic routes
- [ ] Implement GET /health
- [ ] Implement GET /v1/models
- [ ] Add request logging middleware

### Day 3-5: Chat Completions
- [ ] Define request/response types (OpenAI-compatible)
- [ ] Implement POST /v1/chat/completions (non-streaming)
- [ ] Add backend selection (simple: first healthy)
- [ ] Implement streaming with SSE
- [ ] Handle backend errors gracefully

### Day 6-7: Error Handling
- [ ] Define error types
- [ ] Implement error responses (OpenAI format)
- [ ] Add timeout handling
- [ ] Add retry logic

## Week 3 Tasks

### Day 1-2: CLI
- [ ] Add clap for CLI parsing
- [ ] Implement `nexus serve` command
- [ ] Implement `nexus backends` command
- [ ] Implement `nexus models` command
- [ ] Implement `nexus health` command

### Day 3-4: Testing & Docs
- [ ] Write integration tests with real Ollama
- [ ] Write load test (100 concurrent requests)
- [ ] Create README with quick start
- [ ] Create example config file

### Day 5-7: MVP Release
- [ ] Build release binaries (Linux, macOS, Windows)
- [ ] Create GitHub release
- [ ] Write announcement post
- [ ] Collect initial feedback
```

---

## Phase 5: Implement

Run for each task or group of tasks.

```bash
spec-kit implement "Task description"
```

### Example Prompts

#### Implement: Project Setup
```
Implement the initial project setup for Nexus.

Create a new Rust project with:
1. Cargo workspace with two crates:
   - nexus-core (library)
   - nexus-cli (binary)
2. Dependencies:
   - axum = "0.7"
   - tokio = { version = "1", features = ["full"] }
   - serde = { version = "1", features = ["derive"] }
   - serde_json = "1"
   - reqwest = { version = "0.11", features = ["json", "stream"] }
   - tracing = "0.1"
   - tracing-subscriber = { version = "0.3", features = ["env-filter"] }
   - clap = { version = "4", features = ["derive"] }
   - toml = "0.8"
   - thiserror = "1"
   - async-stream = "0.3"
3. Basic project structure:
   - src/lib.rs (re-exports)
   - src/api/mod.rs (HTTP handlers)
   - src/registry/mod.rs (backend registry)
   - src/health/mod.rs (health checker)
   - src/routing/mod.rs (request router)
   - src/config.rs (configuration)
   - src/error.rs (error types)
4. GitHub Actions workflow for CI
5. rustfmt.toml and clippy.toml for consistent style
```

#### Implement: Backend Registry
```
Implement the Backend Registry for Nexus.

Requirements:
1. Thread-safe registry using Arc<RwLock<HashMap>>
2. Backend struct with: id, name, url, backend_type, status, models, priority
3. Model struct with: id, name, context_length, supports_vision, supports_tools
4. Methods:
   - add_backend(&self, backend: Backend) -> Result<()>
   - remove_backend(&self, id: &str) -> Result<()>
   - get_backend(&self, id: &str) -> Option<Backend>
   - get_all_backends(&self) -> Vec<Backend>
   - get_backends_for_model(&self, model: &str) -> Vec<Backend>
   - get_healthy_backends(&self) -> Vec<Backend>
   - update_status(&self, id: &str, status: BackendStatus) -> Result<()>
   - update_models(&self, id: &str, models: Vec<Model>) -> Result<()>
5. Unit tests for all methods
6. Concurrent access tests
```

#### Implement: OpenAI Chat Completions Endpoint
```
Implement the POST /v1/chat/completions endpoint for Nexus.

Requirements:
1. Accept OpenAI ChatCompletionRequest format
2. Select a backend from registry (use get_healthy_backends, pick first)
3. Forward request to backend with reqwest
4. If stream=false: Return ChatCompletionResponse
5. If stream=true: Return SSE stream, forwarding chunks from backend
6. Handle errors:
   - No healthy backends: 503 Service Unavailable
   - Backend timeout: 504 Gateway Timeout
   - Backend error: 502 Bad Gateway
   - Invalid request: 400 Bad Request
7. Log request/response at DEBUG level
8. Log errors at ERROR level

Use these types:
- ChatCompletionRequest (match OpenAI schema)
- ChatCompletionResponse (match OpenAI schema)
- ChatCompletionChunk (for streaming)

Include tests with mock backend.
```

---

## Quick Reference

| Phase | Command | Purpose |
|-------|---------|---------|
| Constitution | `spec-kit constitute` | Define project identity |
| Specify | `spec-kit specify "Feature"` | Detail a feature |
| Plan | `spec-kit plan` | Create implementation plan |
| Tasks | `spec-kit tasks` | Generate task list |
| Implement | `spec-kit implement "Task"` | Generate code for task |

---

## Notes

- Run phases in order; each builds on the previous
- Save outputs to `.spec-kit/` directory for reference
- Review and edit generated specs before proceeding
- Iterate: it's okay to re-run phases with refined prompts
