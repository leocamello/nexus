# Nexus - Spec-Kit Development Guide

This document explains how to use [GitHub Spec-Kit](https://github.com/github/spec-kit) for developing the Nexus LLM Orchestrator with **GitHub Copilot CLI**.

---

## Overview

Spec-Kit is a Spec-Driven Development toolkit that transforms how you build software with AI. Instead of "vibe coding," you create structured specifications that guide implementation.

**Key Concept:** Spec-Kit uses different interfaces depending on your editor:
- **VS Code:** Slash commands like `/speckit.specify`
- **Copilot CLI:** Custom agents via the task tool (already integrated!)

---

## Installation

### Prerequisites

- [uv](https://docs.astral.sh/uv/) - Python package manager
- [Python 3.11+](https://www.python.org/downloads/)
- [Git](https://git-scm.com/downloads)

### Install Specify CLI

```bash
# Persistent installation (recommended)
uv tool install specify-cli --from git+https://github.com/github/spec-kit.git

# Verify installation
specify check
```

### Initialize in Existing Project

This project is already initialized. To re-initialize or update:

```bash
# Initialize with Copilot CLI support
specify init . --ai copilot --force

# Or use --here flag
specify init --here --ai copilot
```

### Upgrade Specify CLI

```bash
uv tool install specify-cli --force --from git+https://github.com/github/spec-kit.git
```

---

## Using Spec-Kit with Copilot CLI

In **Copilot CLI**, you use the **task tool** to invoke spec-kit agents. The agents are already configured for this project!

### Available Custom Agents

| Agent | Purpose |
|-------|---------|
| `speckit.analyze` | Cross-artifact consistency analysis (after tasks, before implement) |
| `speckit.checklist` | Generate custom quality checklist for current feature |
| `speckit.implement` | Execute all tasks from tasks.md |
| `speckit.taskstoissues` | Convert tasks into GitHub issues |

### Core Commands (Direct Prompts)

For the core spec-kit workflow, use direct prompts in Copilot CLI:

```
# 1. Create project constitution (governing principles)
Create the project constitution for Nexus following the spec-kit methodology.
Focus on code quality, testing standards, and performance requirements.

# 2. Create a feature specification
Create a spec for: <your feature description>

# 3. Create technical implementation plan  
Create a technical plan for the spec. Use Rust with Axum, Tokio, and reqwest.

# 4. Generate implementation tasks
Generate implementation tasks from the plan.

# 5. Create GitHub issues from tasks (for collaborative development)
Create GitHub issues from the tasks for tracking in an open-source workflow.

# 6. Execute implementation (uses custom agent)
Use speckit.implement to execute all tasks.
```

### Example Workflow

```
You: Create a spec for: Add rate limiting to the API gateway with configurable 
     limits per client IP and API key

[Copilot creates spec in specs/<feature>/spec.md]

You: Create a technical plan for this spec

[Copilot creates plan in specs/<feature>/plan.md]

You: Generate tasks from the plan

[Copilot creates tasks in specs/<feature>/tasks.md]

You: Analyze the spec, plan, and tasks for consistency

[Copilot runs speckit.analyze agent]

You: Create GitHub issues from the tasks

[Copilot runs speckit.taskstoissues or uses gh CLI to create issues]

You: Implement all the tasks

[Copilot runs speckit.implement agent]
```

---

## Project Structure

After initialization, spec-kit creates:

```
nexus/
├── .specify/
│   ├── memory/
│   │   └── constitution.md    # Project principles
│   ├── scripts/
│   │   └── bash/              # Helper scripts
│   └── templates/             # Artifact templates
├── .github/
│   └── prompts/               # VS Code prompt files (not used by CLI)
├── specs/                     # Feature specifications
│   └── 001-feature-name/
│       ├── spec.md            # Feature specification
│       ├── plan.md            # Technical implementation plan
│       └── tasks.md           # Implementation tasks
└── docs/
    └── SPEC_KIT_PROMPTS.md    # This file
```

---

## Development Phases

### Phase 1: Constitution

Create governing principles for the project. Run once at project start.

**Prompt:**
```
Create the project constitution for Nexus. Include:

## What It Is
A distributed LLM orchestrator that unifies heterogeneous inference backends 
behind an OpenAI-compatible API gateway.

## Core Principles
1. Zero Configuration - mDNS discovery, just run it
2. Single Binary - Rust-based, no runtime dependencies
3. OpenAI-Compatible - Standard Chat Completions API
4. Backend Agnostic - Works with Ollama, vLLM, llama.cpp, exo
5. Intelligent Routing - Routes by capabilities, not just load
6. Resilient - Automatic failover
7. Local-First - For home labs and small teams

## Technical Constraints
- Language: Rust
- Framework: Axum + Tokio
- Discovery: mDNS (mdns-sd crate)
- State: In-memory only

## Success Criteria
- Routing overhead: < 5ms
- Memory usage: < 50MB
```

### Phase 2: Specify (Per Feature)

Create detailed specifications for each feature.

**Example - Core API Gateway:**
```
Create a spec for: Core API Gateway

Implement an OpenAI-compatible HTTP API with:
- POST /v1/chat/completions (streaming and non-streaming)
- GET /v1/models  
- GET /health

Handle 100+ concurrent requests, 5-minute timeouts, graceful shutdown.
Return proper OpenAI error format for all failures.
```

**Example - Intelligent Router:**
```
Create a spec for: Intelligent Router

Route requests to the best backend based on:
- Model name (exact match or alias)
- Required capabilities (vision, tools, context length)
- Backend health and load
- Configured priority

Support strategies: smart, round_robin, random, priority_only
Routing decision must be < 1ms with no external calls.
```

### Phase 3: Plan

Create technical implementation plan with your tech stack choices.

**Prompt:**
```
Create a technical plan for the spec. I am building with:
- Rust (async with Tokio)
- Axum for HTTP
- reqwest for backend communication
- DashMap for concurrent registry
- tracing for logging
```

### Phase 4: Tasks

Generate actionable implementation tasks.

**Prompt:**
```
Generate implementation tasks from the plan. Each task should be:
- Completable in 1-4 hours
- Independently testable
- Have clear acceptance criteria
```

### Phase 5: Requirements Validation (Quality Gate)

Before creating issues or implementing, validate that your spec is ready.

**Steps:**
```bash
# 1. Copy requirements validation template to your feature folder
cp .specify/templates/requirements-validation.md specs/XXX-your-feature/requirements-validation.md

# 2. Complete the 65-item checklist
# - Mark [x] for items that pass
# - Mark [-] for items not applicable
# - Fix any [ ] items before proceeding

# 3. Verify ready for implementation (should be 0)
grep -c "\- \[ \]" specs/XXX-your-feature/requirements-validation.md
```

**Key Validations:**
- Constitution gates addressed (Simplicity, Anti-Abstraction, Integration-First, Performance)
- Core principles aligned (Zero-Config, Single Binary, OpenAI-Compatible, etc.)
- Requirements are complete, testable, and unambiguous
- Edge cases and error handling documented

### Phase 6: GitHub Issues (For Collaborative Development)

Convert tasks to GitHub issues for open-source collaboration and progress tracking.

**Prerequisites:**
- GitHub CLI (`gh`) installed and authenticated: `gh auth login`
- Repository pushed to GitHub

**Prompt:**
```
Create GitHub issues from the tasks in specs/001-backend-registry/tasks.md.

Each issue should:
- Have a clear title prefixed with [Feature Name]
- Include implementation steps and acceptance criteria
- Be labeled appropriately (P0, enhancement, testing, etc.)
- Reference dependencies on other issues
- Link back to spec and plan files
```

**What Gets Created:**
- One issue per task (T01, T02, etc.)
- Labels: `P0`, `enhancement`, `backend-registry`, `testing`, `documentation`, `good first issue`
- Each issue body includes:
  - Overview and estimated time
  - Dependencies on other issues
  - Tests to write first (TDD)
  - Implementation steps
  - Acceptance criteria
  - Links to spec/plan/tasks

**Example Issue Structure:**
```markdown
## Overview
[Task description]

**Estimated Time**: X hours
**Dependencies**: #N (previous task)

## Tests to Write First
[Test signatures]

## Implementation Steps
1. [Step 1]
2. [Step 2]

## Acceptance Criteria
- [ ] [Criterion 1]
- [ ] [Criterion 2]

## References
- [Spec](specs/NNN-feature/spec.md)
- [Plan](specs/NNN-feature/plan.md)
```

**Viewing Issues:**
```bash
gh issue list                    # List all issues
gh issue view N                  # View issue #N
gh issue close N                 # Close issue after completion
```

### Phase 7: Analyze (Optional but Recommended)

Run before implementation to catch inconsistencies.

**Prompt:**
```
Analyze the spec, plan, and tasks for consistency and coverage issues.
```

Or use the task tool:
```
Use the speckit.analyze agent to analyze the current feature.
```

### Phase 8: Implement

Execute all tasks to build the feature.

**Steps:**
```bash
# 1. Create feature branch
git checkout -b feature/fXX-feature-name

# 2. Copy implementation verification template
cp .specify/templates/implementation-verification.md specs/XXX-your-feature/verification.md

# 3. Run implementation agent
# Prompt: "Use speckit.implement to execute all tasks in tasks.md"

# 4. Check off tasks.md criteria as you verify them
# 5. Complete verification.md after implementation
```

**Prompt:**
```
Use the speckit.implement agent to execute all tasks in tasks.md.
```

### Phase 9: Verification (Quality Gate)

After implementation, verify all quality criteria are met.

**Steps:**
```bash
# 1. Run speckit.analyze for final consistency check
# Prompt: "Use speckit.analyze"

# 2. Complete the 210-item verification checklist
# Mark [x] for verified, [-] for N/A

# 3. Verify no unchecked items remain (all should be 0)
grep -c "\- \[ \]" specs/XXX-your-feature/verification.md
grep -c "\- \[ \]" specs/XXX-your-feature/tasks.md
```

### Phase 10: Walkthrough (Documentation)

After implementation, create a code walkthrough document for onboarding and knowledge sharing.

**Prompt:**
```
Explain the code for [feature] as if I were a junior developer joining the project.
Walk through each file and the key tests. Save this as a walkthrough.md document.
```

**Output:** `specs/NNN-feature/walkthrough.md`

**What It Includes:**
- The big picture (how the feature fits in the system)
- File-by-file explanation with annotated code
- Key Rust concepts used
- Test walkthrough (unit, property, stress tests)
- Common patterns in the codebase

---

## Quick Reference

| Phase | What to Say in Copilot CLI |
|-------|---------------------------|
| 1. Constitution | "Create the project constitution for Nexus..." |
| 2. Specify | "Create a spec for: [feature description]" |
| 3. Plan | "Create a technical plan for the spec" |
| 4. Tasks | "Generate implementation tasks from the plan" |
| 5. Validate | Copy `requirements-validation.md` and complete checklist |
| 6. Issues | "Create GitHub issues from the tasks" |
| 7. Analyze | "Use speckit.analyze to check consistency" |
| 8. Implement | "Use speckit.implement to execute all tasks" |
| 9. Verify | Copy `verification.md` and complete checklist |
| 10. Walkthrough | "Explain the code as if I were a junior developer" |

---

## The Three-Checklist System

Nexus uses a **three-checklist system** for quality assurance:

| Checklist | When | Items | Purpose |
|-----------|------|-------|---------|
| `requirements-validation.md` | BEFORE implementation | 65 | Validate spec is ready |
| `tasks.md` | DURING implementation | varies | Track acceptance criteria |
| `implementation-verification.md` | AFTER implementation | 210 | Verify implementation |

**Template Files:**
- `.specify/templates/requirements-validation.md`
- `.specify/templates/implementation-verification.md`

**Reference Checklist (not copied):**
- `.specify/checklists/requirements-quality.md` (208 items - comprehensive reference)

---

## Tips

1. **Work in feature branches**: Create `feature/fXX-name` branch before implementing
2. **Review generated artifacts**: Edit spec.md, plan.md, tasks.md before implementing
3. **Complete requirements-validation.md**: No unchecked items before creating issues
4. **Create GitHub issues before implementing**: Enables progress tracking and collaboration
5. **Run analyze before implement**: Catches issues early
6. **Use the constitution**: Reference it in prompts for consistency
7. **Iterate**: Re-run phases with refined prompts if needed
8. **Check off tasks.md as you go**: Update acceptance criteria during implementation
9. **Complete verification.md before PR**: Ensures quality gate is passed
10. **Generate walkthroughs after implementing**: Great for onboarding and knowledge sharing

---

## Troubleshooting

### "Custom agents not showing"

For Copilot CLI, custom agents are invoked via the task tool, not slash commands:
```
Use the speckit.implement agent to execute all tasks.
```

### "Where are my specs?"

Check the `specs/` directory. Feature specs are organized by branch or feature number:
```
specs/001-core-api-gateway/
specs/002-backend-registry/
```

### "Environment variable for non-git repos"

Set `SPECIFY_FEATURE` to the feature directory name:
```bash
export SPECIFY_FEATURE="001-core-api-gateway"
```

---

## Nexus Feature Prompts

Below are ready-to-use prompts for all Nexus features, organized by priority.

### Feature Index

| ID | Feature | Version | Prompt Section |
|----|---------|---------|----------------|
| F01 | Core API Gateway | v0.1 ✅ | [Link](#f01-core-api-gateway) |
| F02 | Backend Registry | v0.1 ✅ | [Link](#f02-backend-registry) |
| F03 | Health Checker | v0.1 ✅ | [Link](#f03-health-checker) |
| F04 | CLI and Configuration | v0.1 ✅ | [Link](#f04-cli-and-configuration) |
| F05 | mDNS Discovery | v0.1 ✅ | [Link](#f05-mdns-discovery) |
| F06 | Intelligent Router | v0.1 ✅ | [Link](#f06-intelligent-router) |
| F07 | Model Aliases | v0.1 ✅ | [Link](#f07-model-aliases) |
| F08 | Fallback Chains | v0.1 ✅ | [Link](#f08-fallback-chains) |
| F09 | Request Metrics | v0.2 | [Link](#f09-request-metrics) |
| F10 | Web Dashboard | v0.2 | [Link](#f10-web-dashboard) |
| F11 | Structured Request Logging | v0.2 | [Link](#f11-structured-request-logging) |
| F12 | Cloud Backend Support | v0.3 | [Link](#f12-cloud-backend-support) |
| F13 | Privacy Zones & Capability Tiers | v0.3 | [Link](#f13-privacy-zones--capability-tiers) |
| F14 | Inference Budget Management | v0.3 | [Link](#f14-inference-budget-management) |
| F15 | Speculative Router | v0.4 | [Link](#f15-speculative-router) |
| F16 | Quality Tracking & Backend Profiling | v0.4 | [Link](#f16-quality-tracking--backend-profiling) |
| F17 | Embeddings API | v0.4 | [Link](#f17-embeddings-api) |
| F18 | Request Queuing & Prioritization | v0.4 | [Link](#f18-request-queuing--prioritization) |
| F19 | Pre-warming & Fleet Intelligence | v0.5 | [Link](#f19-pre-warming--fleet-intelligence) |
| F20 | Model Lifecycle Management | v0.5 | [Link](#f20-model-lifecycle-management) |
| F21 | Multi-Tenant Support | v0.5 | [Link](#f21-multi-tenant-support) |
| F22 | Rate Limiting | v0.5 | [Link](#f22-rate-limiting) |

---

## P0 Features (MVP)

### F01: Core API Gateway
```
Create a spec for: Core API Gateway

## Feature Description
HTTP server exposing OpenAI-compatible endpoints that proxy requests to backends.
This is the primary interface for all clients (Claude Code, Continue.dev, etc.).

## Endpoints

### POST /v1/chat/completions
- Accept standard OpenAI ChatCompletionRequest format
- Support both streaming (SSE) and non-streaming responses
- Pass through Authorization headers to backends
- Return proper usage stats (prompt_tokens, completion_tokens)

### GET /v1/models
- List all available models from all healthy backends
- Include Nexus-specific metadata (backends, context_length, capabilities)
- Response matches OpenAI ModelsResponse format

### GET /health
- Return system status (healthy/degraded/unhealthy)
- Include backend counts (total, healthy, unhealthy)
- Include model count and uptime

## Non-Functional Requirements
- Handle 100+ concurrent requests
- Request timeout: configurable (default 5 minutes)
- Graceful shutdown on SIGTERM
- Structured logging with tracing

## Error Handling (OpenAI format)
- Backend timeout: 504 Gateway Timeout
- Backend unreachable: 502 Bad Gateway  
- Invalid request: 400 Bad Request with details
- Model not found: 404 with available models hint
- No healthy backends: 503 Service Unavailable

## Technical Stack
- Axum for HTTP server
- reqwest for backend HTTP client with connection pooling
- async-stream for SSE forwarding
- tokio for async runtime

## Acceptance Criteria
- [ ] POST /v1/chat/completions works with non-streaming
- [ ] POST /v1/chat/completions works with streaming (SSE)
- [ ] GET /v1/models lists all models from all backends
- [ ] GET /health returns system status
- [ ] Handles concurrent requests (100+)
- [ ] Proper error responses in OpenAI format
```

---

### F02: Backend Registry
```
Create a spec for: Backend Registry

## Feature Description
In-memory data store tracking all known backends and their models.
This is the source of truth for all backend state.

## Data Structures

### Backend
- id: String (UUID)
- name: String (human-readable)
- url: String (base URL, e.g., "http://localhost:11434")
- backend_type: Enum (Ollama, VLLM, LlamaCpp, Exo, OpenAI, Generic)
- status: Enum (Healthy, Unhealthy, Unknown, Draining)
- last_health_check: DateTime<Utc>
- last_error: Option<String>
- models: Vec<Model>
- priority: i32 (lower = prefer)
- pending_requests: u32 (current in-flight)
- total_requests: u64 (lifetime total)
- avg_latency_ms: u32 (rolling average)
- discovery_source: Enum (Static, MDNS, Manual)
- metadata: HashMap<String, String>

### Model
- id: String (model identifier, e.g., "llama3:70b")
- name: String (display name)
- context_length: u32 (max context window)
- supports_vision: bool
- supports_tools: bool
- supports_json_mode: bool
- max_output_tokens: Option<u32>

## Operations
| Operation | Description |
|-----------|-------------|
| add_backend(backend) | Add new backend to registry |
| remove_backend(id) | Remove backend by ID |
| get_backend(id) | Get single backend |
| get_all_backends() | List all backends |
| get_healthy_backends() | Filter to healthy only |
| get_backends_for_model(model) | Find backends with model |
| update_status(id, status) | Update health status |
| update_models(id, models) | Update model list |
| increment_pending(id) | Track in-flight request |
| decrement_pending(id) | Request completed |
| update_latency(id, ms) | Update rolling average |

## Thread Safety
- Use DashMap for concurrent access
- Read-heavy workload expected (many reads, few writes)
- Maintain model-to-backend index for fast lookup

## Acceptance Criteria
- [ ] Thread-safe access with DashMap
- [ ] Fast lookup by model name (indexed)
- [ ] Survives concurrent read/write stress test
- [ ] Serializable to JSON for debugging
- [ ] Atomic updates for pending_requests and latency
```

---

### F03: Health Checker
```
Create a spec for: Health Checker

## Feature Description
Background service that periodically checks backend health and updates the registry.
Runs continuously without blocking request routing.

## Health Check Flow
1. Every N seconds (default 30), for each backend:
   a. Send health check request with 5s timeout
   b. Parse response to extract model list
   c. Update registry status and models
   d. Log status transitions at INFO level

2. Backend-specific endpoints:
   - Ollama: GET /api/tags (returns {"models": [...]})
   - vLLM: GET /v1/models (OpenAI format)
   - llama.cpp: GET /health
   - Generic: GET /v1/models

3. Status transitions:
   - Unknown → Healthy: 1 success
   - Unknown → Unhealthy: 1 failure
   - Healthy → Unhealthy: 3 consecutive failures
   - Unhealthy → Healthy: 2 consecutive successes

## Configuration
```toml
[health_check]
enabled = true
interval_seconds = 30
timeout_seconds = 5
failure_threshold = 3
recovery_threshold = 2
```

## Model Parsing
Parse different response formats:
- Ollama: {"models": [{"name": "llama3:70b", "details": {...}}]}
- OpenAI: {"data": [{"id": "llama3-70b", "object": "model"}]}

Extract capabilities where available (context_length, vision support).

## Edge Cases
- Backend returns 200 but invalid response → Treat as unhealthy
- Backend very slow but responds → Healthy, but record latency
- DNS resolution fails → Unhealthy with specific error
- TLS certificate error → Unhealthy with specific error

## Non-Functional Requirements
- Health checks must not block request routing
- Stagger checks to avoid thundering herd
- Graceful shutdown (finish current checks)

## Acceptance Criteria
- [ ] Checks all backends periodically
- [ ] Updates registry on status change
- [ ] Logs health transitions at INFO level
- [ ] Parses model lists from Ollama and OpenAI formats
- [ ] Handles timeouts gracefully
- [ ] Staggered checks prevent thundering herd
```

---

### F04: CLI and Configuration
```
Create a spec for: CLI and Configuration

## Feature Description
Command-line interface and TOML configuration file support.
Provides both interactive commands and daemon mode.

## CLI Commands

### nexus serve [OPTIONS]
Start the Nexus server.
  --config, -c <FILE>     Config file path (default: nexus.toml)
  --port, -p <PORT>       Listen port (default: 8000)
  --host <HOST>           Listen address (default: 0.0.0.0)
  --log-level <LEVEL>     Log level: trace, debug, info, warn, error
  --no-discovery          Disable mDNS discovery

### nexus backends [OPTIONS]
List all backends.
  --json                  Output as JSON
  --status <STATUS>       Filter by status (healthy, unhealthy, unknown)

### nexus backends add <URL> [OPTIONS]
Add backend manually.
  --name <NAME>           Display name
  --type <TYPE>           Backend type (ollama, vllm, llamacpp, exo, generic)
  --priority <N>          Routing priority (lower = prefer)

### nexus backends remove <ID>
Remove a backend by ID or URL.

### nexus models [OPTIONS]
List all available models.
  --json                  Output as JSON
  --backend <ID>          Filter by backend

### nexus health [OPTIONS]
Show health status.
  --json                  Output as JSON

### nexus config init [OPTIONS]
Generate example config file.
  --output, -o <FILE>     Output file (default: nexus.toml)

### nexus --version / --help

## Configuration File (nexus.toml)
```toml
[server]
host = "0.0.0.0"
port = 8000
request_timeout_seconds = 300
max_concurrent_requests = 1000

[discovery]
enabled = true
service_types = ["_ollama._tcp.local", "_llm._tcp.local"]
grace_period_seconds = 60

[health_check]
enabled = true
interval_seconds = 30
timeout_seconds = 5
failure_threshold = 3
recovery_threshold = 2

[routing]
strategy = "smart"
max_retries = 2

[routing.weights]
priority = 50
load = 30
latency = 20

[routing.aliases]
"gpt-4" = "llama3:70b"
"gpt-3.5-turbo" = "mistral:7b"

[routing.fallbacks]
"llama3:70b" = ["qwen2:72b", "mixtral:8x7b"]

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
```

## Environment Variables
- NEXUS_CONFIG: Config file path
- NEXUS_PORT: Listen port
- NEXUS_HOST: Listen address  
- NEXUS_LOG_LEVEL: Log level
- NEXUS_DISCOVERY: Enable discovery (true/false)

## Config Precedence
CLI args > Environment variables > Config file > Defaults

## CLI Output Examples
```bash
$ nexus backends
┌──────────────┬────────────────────────────┬─────────┬──────────┬────────┐
│ Name         │ URL                        │ Type    │ Status   │ Models │
├──────────────┼────────────────────────────┼─────────┼──────────┼────────┤
│ local-ollama │ http://localhost:11434     │ Ollama  │ Healthy  │ 3      │
│ gpu-server   │ http://192.168.1.100:8000  │ vLLM    │ Healthy  │ 1      │
└──────────────┴────────────────────────────┴─────────┴──────────┴────────┘

$ nexus health
Status: Healthy
Uptime: 2h 34m
Backends: 2/3 healthy
Models: 4 available
```

## Technical Stack
- clap for CLI parsing (derive feature)
- config crate for layered configuration
- toml for config serialization
- comfy-table for pretty CLI output

## Acceptance Criteria
- [ ] `nexus serve` starts server with all options
- [ ] `nexus backends` lists backends with table/JSON output
- [ ] `nexus models` lists models from all backends
- [ ] `nexus health` shows system status
- [ ] `nexus config init` generates valid example config
- [ ] Config file loads correctly
- [ ] Environment variables override config
- [ ] CLI args override everything
```

---

## P1 Features (Post-MVP)

### F05: mDNS Discovery
```
Create a spec for: mDNS Discovery

## Feature Description
Automatically discover LLM backends on local network using mDNS/Bonjour.
Zero-configuration for Ollama instances on the same network.

## Supported Service Types
| Service Type | Backend Type | Notes |
|--------------|--------------|-------|
| _ollama._tcp.local | Ollama | Ollama advertises this by default |
| _llm._tcp.local | Generic | Proposed standard for LLM services |
| _http._tcp.local | Generic | With TXT record hints |

## Discovery Flow
1. On startup:
   - Start mDNS browser for each configured service type
   - Register for service events (found/removed)

2. On ServiceResolved:
   - Extract IP address and port from SRV record
   - Extract metadata from TXT records (type, version, api_path)
   - Create Backend struct with DiscoverySource::MDNS
   - Add to registry if not exists
   - Trigger immediate health check

3. On ServiceRemoved:
   - Mark backend status as Unknown
   - Start grace period timer (60s default)
   - If not seen again within grace period, remove from registry

4. Continuous operation:
   - Keep browsing for changes
   - Handle network interface changes gracefully

## TXT Record Parsing
```
# Ollama default
version=0.1.0

# Proposed LLM standard
type=vllm
api_path=/v1
version=1.0.0
models=llama3:70b,mistral:7b
```

## Configuration
```toml
[discovery]
enabled = true
service_types = ["_ollama._tcp.local", "_llm._tcp.local"]
grace_period_seconds = 60
```

## Edge Cases
- Multiple instances same IP, different ports → Treat as separate backends
- Service disappears then reappears → Keep existing backend, update status
- mDNS not available (Docker, WSL) → Graceful fallback to static config only
- Conflicting manual and discovered config → Manual takes precedence
- IPv6 addresses → Support both IPv4 and IPv6

## Technical Stack
- mdns-sd crate for cross-platform mDNS
- Run discovery in background tokio task
- Send updates via channel to main registry

## Acceptance Criteria
- [ ] Discovers Ollama instances automatically
- [ ] Handles service appearing/disappearing
- [ ] Grace period prevents flapping
- [ ] Works on macOS, Linux, Windows
- [ ] Graceful fallback if mDNS unavailable
- [ ] Manual config takes precedence over discovered
```

---

### F06: Intelligent Router
```
Create a spec for: Intelligent Router

## Feature Description
Select the best backend for each request based on model requirements,
capabilities, and current system load.

## Routing Algorithm
```python
def select_backend(request):
    # 1. Extract requirements from request
    requirements = extract_requirements(request)
    # - model_name
    # - estimated_tokens (chars / 4)
    # - needs_vision (has image_url in messages)
    # - needs_tools (has tools array)
    # - needs_json_mode (response_format.type == "json_object")
    
    # 2. Find candidates with matching model
    candidates = registry.get_backends_for_model(requirements.model)
    
    # 3. Filter by health status
    candidates = [b for b in candidates if b.status == Healthy]
    
    # 4. Filter by capabilities
    candidates = [b for b in candidates if meets_requirements(b, requirements)]
    
    # 5. Check aliases if no candidates found
    if not candidates and requirements.model in aliases:
        requirements.model = aliases[requirements.model]
        return select_backend(request)  # Retry with alias
    
    # 6. Check fallback chain
    if not candidates and requirements.model in fallbacks:
        for fallback_model in fallbacks[requirements.model]:
            # Try each fallback in order
            ...
    
    # 7. Score and select best candidate
    scores = [(score(b, requirements), b) for b in candidates]
    return max(scores, key=lambda x: x[0])
```

## Scoring Function
```
score = (100 - priority) * priority_weight
      + (100 - min(pending_requests, 100)) * load_weight  
      + (100 - min(avg_latency_ms / 10, 100)) * latency_weight
```

Default weights: priority=50, load=30, latency=20

## Capability Detection from Request
| Requirement | Detection Method |
|-------------|------------------|
| Vision | messages[*].content[*].type == "image_url" |
| Tools | tools array present and non-empty |
| JSON Mode | response_format.type == "json_object" |
| Context Length | Estimate: sum(len(m.content) for m in messages) / 4 |

## Routing Strategies
| Strategy | Description | Use Case |
|----------|-------------|----------|
| smart | Score by priority + load + latency | Default, recommended |
| round_robin | Rotate through healthy backends | Even distribution |
| priority_only | Always use lowest priority number | Dedicated primary |
| random | Random selection from healthy | Testing |

## Configuration
```toml
[routing]
strategy = "smart"
max_retries = 2

[routing.weights]
priority = 50
load = 30
latency = 20
```

## Non-Functional Requirements
- Routing decision: < 1ms
- No external calls during routing (use cached data only)
- Thread-safe (multiple concurrent routing decisions)

## Acceptance Criteria
- [ ] Matches model by exact name
- [ ] Filters by capabilities (vision, tools, json_mode)
- [ ] Filters by context length requirement
- [ ] Scores by priority, load, latency
- [ ] Falls back to aliases when model not found
- [ ] Returns appropriate error if no backend available
- [ ] All routing strategies work correctly
```

---

### F07: Model Aliases
```
Create a spec for: Model Aliases

## Feature Description
Map common model names (like "gpt-4") to available local models.
Enables drop-in compatibility with tools configured for OpenAI.

## Configuration
```toml
[routing.aliases]
"gpt-4" = "llama3:70b"
"gpt-4-turbo" = "llama3:70b"
"gpt-4o" = "llama3:70b"
"gpt-3.5-turbo" = "mistral:7b"
"claude-3-opus" = "qwen2:72b"
"claude-3-sonnet" = "llama3:70b"
"claude-3-haiku" = "mistral:7b"
```

## Behavior
1. Request comes in for model "gpt-4"
2. Router checks if any backend has "gpt-4" directly
3. If not found, check aliases: "gpt-4" → "llama3:70b"
4. Route to backend with "llama3:70b"
5. Response model field shows "gpt-4" (what client requested)

## Alias Resolution Rules
- Aliases are resolved at routing time, not registration
- If both alias and target exist, prefer direct match
- Aliases can chain: "gpt-4" → "llama-70b" → "llama3:70b" (max 3 levels)
- Circular aliases are detected and rejected at config load

## Logging
- Log alias resolution at DEBUG level
- Include both requested model and resolved model

## Acceptance Criteria
- [ ] Aliases configured in config file
- [ ] Transparent to client (response shows requested model name)
- [ ] Alias resolution logged at DEBUG level
- [ ] Circular alias detection at config load
- [ ] Max 3 levels of chaining
- [ ] Direct matches preferred over aliases
```

---

### F08: Fallback Chains
```
Create a spec for: Fallback Chains

## Feature Description
Automatic fallback to alternative models when primary model is unavailable.
Maintains service availability when preferred models are down.

## Configuration
```toml
[routing.fallbacks]
"llama3:70b" = ["qwen2:72b", "mixtral:8x7b", "llama3:8b"]
"gpt-4" = ["llama3:70b", "qwen2:72b", "mistral:7b"]
"claude-3-opus" = ["llama3:70b", "mixtral:8x7b"]
```

## Fallback Behavior
1. Request for "llama3:70b"
2. All backends with "llama3:70b" are unhealthy or unavailable
3. Check fallback chain: ["qwen2:72b", "mixtral:8x7b", "llama3:8b"]
4. Try "qwen2:72b" → Check if available and healthy
5. If available, route there. If not, try next in chain.
6. If all fallbacks exhausted, return 503 Service Unavailable

## Fallback vs Retry
- Retry: Same model, different backend (automatic on failure)
- Fallback: Different model when primary model completely unavailable

## Logging
- Log fallback usage at WARN level
- Include original model, fallback model, and reason

## Response Handling
- Response model field shows original requested model (not fallback)
- Add X-Nexus-Fallback-Model header with actual model used

## Acceptance Criteria
- [ ] Fallback chains configurable per model
- [ ] Tries each fallback in order
- [ ] Logs fallback usage at WARN level
- [ ] Returns 503 if all fallbacks exhausted
- [ ] X-Nexus-Fallback-Model header indicates actual model
- [ ] Response model field shows requested model
```

---

## P2 Features (Polish)

### F09: Request Metrics
```
Create a spec for: Request Metrics

## Feature Description
Track request statistics for observability and debugging.
Expose metrics in both Prometheus and JSON formats.

## Metrics

### Counters
- nexus_requests_total{model, backend, status}
  Labels: model name, backend name, HTTP status code
- nexus_errors_total{type}
  Labels: error type (timeout, backend_error, no_backend, etc.)
- nexus_fallbacks_total{from_model, to_model}
  Labels: original model, fallback model

### Histograms
- nexus_request_duration_seconds{model, backend}
  Buckets: [0.1, 0.25, 0.5, 1, 2.5, 5, 10, 30, 60, 120, 300]
- nexus_backend_latency_seconds{backend}
  Health check latency per backend
- nexus_tokens_total{model, backend, type}
  Labels: prompt/completion token counts

### Gauges
- nexus_backends_healthy
- nexus_backends_total
- nexus_pending_requests{backend}
- nexus_models_available

## Endpoints

### GET /metrics
Prometheus-compatible text format.
```
# HELP nexus_requests_total Total number of requests
# TYPE nexus_requests_total counter
nexus_requests_total{model="llama3:70b",backend="local-ollama",status="200"} 1523
nexus_requests_total{model="llama3:70b",backend="local-ollama",status="500"} 12
```

### GET /v1/stats
JSON format for debugging.
```json
{
  "uptime_seconds": 3600,
  "requests": {
    "total": 1535,
    "success": 1523,
    "errors": 12
  },
  "backends": {
    "local-ollama": {
      "requests": 1000,
      "avg_latency_ms": 45,
      "pending": 2
    }
  },
  "models": {
    "llama3:70b": {
      "requests": 800,
      "avg_duration_ms": 2500
    }
  }
}
```

## Technical Stack
- metrics crate for metric collection
- metrics-exporter-prometheus for Prometheus format
- In-memory counters with atomic operations

## Acceptance Criteria
- [ ] Prometheus-compatible /metrics endpoint
- [ ] JSON stats at /v1/stats
- [ ] Request duration tracking with histograms
- [ ] Error rate tracking by type
- [ ] Per-backend and per-model breakdowns
- [ ] Minimal performance impact (< 0.1ms overhead)
```

---

### F10: Web Dashboard
```
Create a spec for: Web Dashboard

## Feature Description
Simple web UI for monitoring Nexus status.
Embedded in the binary, no external dependencies.

## Features

### Backend Status Overview
- List of all backends with status indicators (green/red/yellow)
- Last health check time
- Pending requests count
- Average latency

### Model Availability Matrix
- Grid showing which models are available on which backends
- Capability indicators (vision, tools, json_mode)
- Context length display

### Request History
- Last 100 requests (in-memory ring buffer)
- Model, backend, duration, status
- Expandable details for errors

### Real-time Updates
- WebSocket connection for live updates
- Auto-refresh every 5 seconds as fallback
- Status change notifications

## Technology
- Embedded static files using rust-embed
- Vanilla HTML/CSS/JavaScript (no framework)
- Tailwind CSS for styling (precompiled)
- WebSocket via axum for real-time updates

## Routes
- GET / → Dashboard HTML
- GET /assets/* → Static files (JS, CSS)
- WS /ws → WebSocket for live updates
- Existing API routes unchanged

## Graceful Degradation
- Works without JavaScript (static page with refresh button)
- Mobile-responsive design
- Dark mode support (prefers-color-scheme)

## Acceptance Criteria
- [ ] Shows backend status with health indicators
- [ ] Shows model availability across backends
- [ ] Request history with last 100 requests
- [ ] Real-time updates via WebSocket
- [ ] Works without JavaScript (basic functionality)
- [ ] Mobile-responsive layout
- [ ] Embedded in binary (no external files needed)
- [ ] Dashboard accessible at GET /
```

---

## Implementation Order

Follow this order for a smooth development experience:

### v0.1: Foundation ✅ Released
1. **F02: Backend Registry** ✅
2. **F03: Health Checker** ✅
3. **F01: Core API Gateway** ✅
4. **F04: CLI and Configuration** ✅
5. **F05: mDNS Discovery** ✅
6. **F06: Intelligent Router** ✅
7. **F07: Model Aliases** ✅
8. **F08: Fallback Chains** ✅

### v0.2: Observability (Next)
9. **F09: Request Metrics** — Prometheus + JSON stats
10. **F10: Web Dashboard** — Embedded monitoring UI
11. **F11: Structured Request Logging** — Correlation IDs, JSON format

### v0.3: Cloud Hybrid Gateway
12. **F12: Cloud Backend Support** — OpenAI, Anthropic, Google + Nexus-Transparent Protocol
13. **F13: Privacy Zones & Capability Tiers** — Structural privacy, quality guarantees
14. **F14: Inference Budget Management** — Tokenizer registry, cost tracking, graceful degradation

### v0.4: Intelligence
15. **F15: Speculative Router** — Request-content-aware routing (zero ML)
16. **F16: Quality Tracking** — Backend profiling, reliability scoring
17. **F17: Embeddings API** — /v1/embeddings support
18. **F18: Request Queuing** — Bounded queue with priorities

### v0.5: Orchestration
19. **F19: Pre-warming** — Demand prediction, VRAM-aware recommendations
20. **F20: Model Lifecycle** — Load/unload/migrate via API
21. **F21: Multi-Tenant** — API keys, quotas, access control
22. **F22: Rate Limiting** — Token bucket with burst support

---

## v0.2 Feature Prompts

### F11: Structured Request Logging
```
Create a spec for: Structured Request Logging (F11)

## Feature Description
Structured, queryable logs for every request passing through Nexus. Every request
gets a correlation ID that tracks it through retries and failovers.

## Constitution Alignment
- Principle III (OpenAI-Compatible): Logs never contain response body content
- Principle VIII (Stateless): Logs are emitted, not stored in Nexus state
- Principle X (Precise Measurement): Log fields are accurate, not estimated

## Log Fields
- timestamp, request_id, model, backend, backend_type, status
- latency_ms, tokens_prompt, tokens_completion, stream
- route_reason, retry_count, fallback_chain

## Requirements
- JSON and human-readable output formats (via tracing)
- Configurable log level per component
- Request correlation IDs across retry/failover chains
- No sensitive data (message content) in logs by default
- Configurable opt-in for request content logging (debug only)

## Acceptance Criteria
- [ ] Every request produces a structured log entry
- [ ] Request correlation ID tracks retries and failovers
- [ ] Log format is configurable (JSON / pretty)
- [ ] Message content is never logged by default
- [ ] Log output compatible with common log aggregators (ELK, Loki)
```

---

## v0.3 Feature Prompts

### F12: Cloud Backend Support
```
Create a spec for: Cloud Backend Support with Nexus-Transparent Protocol (F12)

## Feature Description
Register cloud LLM APIs (OpenAI, Anthropic, Google) as backends alongside local
inference servers. Introduce the Nexus-Transparent Protocol: X-Nexus-* response
headers that reveal routing decisions without modifying the OpenAI-compatible
JSON response body.

## Constitution Alignment
- Principle III (OpenAI-Compatible): Headers only, never modify response JSON
- Principle IX (Explicit Contracts): Actionable 503s with context object
- Principle X (Precise Measurement): Cost estimation in response headers

## Cloud Backend Configuration
```toml
[[backends]]
name = "openai-gpt4"
url = "https://api.openai.com"
backend_type = "openai"
api_key_env = "OPENAI_API_KEY"
zone = "open"
tier = 4
```

## Nexus-Transparent Protocol Headers
- X-Nexus-Backend: backend name
- X-Nexus-Backend-Type: local | cloud
- X-Nexus-Route-Reason: capability-match | capacity-overflow | privacy-requirement
- X-Nexus-Cost-Estimated: per-request cost (cloud only)
- X-Nexus-Privacy-Zone: restricted | open

## Actionable Error Schema
503 responses include context object: required_tier, available_backends, eta_seconds

## API Translation
- Anthropic API ↔ OpenAI format translation (message format, streaming format)
- Google AI ↔ OpenAI format translation

## Acceptance Criteria
- [ ] Cloud backends registered via TOML config
- [ ] API keys from environment variables (never in config)
- [ ] X-Nexus-* headers on all proxied responses
- [ ] Actionable 503 responses with context
- [ ] Anthropic API translation works (streaming and non-streaming)
- [ ] Cloud backends participate in standard routing and failover
```

---

### F13: Privacy Zones & Capability Tiers
```
Create a spec for: Privacy Zones & Capability Tiers (F13)

## Feature Description
Structural enforcement of privacy boundaries and quality levels. Privacy is a
backend property configured by the admin, NOT a request header that clients
can forget. Capability tiers prevent silent quality downgrades during failover.

## Constitution Alignment
- Principle IX (Explicit Contracts): Privacy is structural, not opt-in
- Principle IX: Never silently downgrade quality
- Principle VIII (Stateless): Zone enforcement per-request, no session tracking

## Privacy Zones
- "restricted" backends: local-only, never receive cloud-overflow traffic
- "open" backends: can receive overflow from any zone
- Cross-zone overflow: fresh context only or block entirely (never forward history)
- Backend affinity (sticky routing) for restricted conversations
- If restricted backend fails → 503 with Retry-After, NOT silent cloud failover

## Capability Tiers
- Backends declare capability scores: reasoning, coding, context_window, vision, tools
- Overflow only to same-tier-or-higher backends
- Client controls:
  - X-Nexus-Strict: true → only the exact requested model
  - X-Nexus-Flexible: true → tier-equivalent alternatives acceptable
  - Default: strict (never surprise the developer)

## Acceptance Criteria
- [ ] Privacy zones enforced at routing layer as backend property
- [ ] Restricted backends never receive cloud-overflow traffic
- [ ] Cross-zone overflow blocks conversation history forwarding
- [ ] Capability tiers prevent silent quality downgrades
- [ ] Client can opt into strict or flexible routing via header
- [ ] 503 responses include tier/zone context for debugging
```

---

### F14: Inference Budget Management
```
Create a spec for: Inference Budget Management (F14)

## Feature Description
Cost-aware routing with graceful degradation. Includes a tokenizer registry
for audit-grade token counting across different providers.

## Constitution Alignment
- Principle X (Precise Measurement): Per-backend tokenizer, not generic estimates
- Principle IX (Explicit Contracts): Budgets degrade gracefully, never hard-cut
- Principle V (Intelligent Routing): Cost is a routing factor

## Tokenizer Registry
- OpenAI models: o200k_base / cl100k_base via tiktoken-rs
- Anthropic models: cl100k_base approximation via tiktoken-rs
- Llama models: SentencePiece via tokenizers crate
- Unknown models: 1.15x conservative multiplier, flagged "estimated" in metrics

## Budget Configuration
```toml
[budget]
monthly_limit = 100.00
soft_limit_percent = 80
hard_limit_action = "local-only"  # "local-only" | "queue" | "reject"
```

## Degradation Behavior
- 0-80%: Normal routing (local-first, cloud overflow)
- 80-100%: Local-preferred (cloud only if no local option), emit warning
- 100%+: hard_limit_action applies (never hard-cut production)

## Acceptance Criteria
- [ ] Per-request cost estimation using backend-specific tokenizer
- [ ] Soft limit shifts to local-preferred routing
- [ ] Hard limit triggers configurable action (never hard-cut)
- [ ] Budget metrics exposed via Prometheus
- [ ] Unknown tokenizers use 1.15x multiplier with "estimated" flag
- [ ] Budget resets monthly with configurable billing cycle
```

---

## v0.4 Feature Prompts

### F15: Speculative Router
```
Create a spec for: Speculative Router (F15)

## Feature Description
Request-content-aware routing using JSON payload inspection only. Zero ML,
sub-millisecond decisions. Extracts routing signals from the request structure
without analyzing prompt content semantics.

## Constitution Alignment
- Principle V (Intelligent Routing): Match capabilities to request requirements
- Constitution Performance Gate: Routing decision < 1ms
- Principle III: Router reads request JSON but never modifies it

## Routing Signals
| Signal | Source | Routing Decision |
|--------|--------|-----------------|
| Token count estimate | messages array length | Context window requirement |
| Image content | content[].type == "image_url" | Vision-capable backend |
| Tool definitions | tools[] present | Tool-use-capable backend |
| Response format | response_format.type == "json_object" | JSON-mode backend |
| Stream flag | stream: true | Prefer efficient streaming |

## Performance
- Payload inspection: < 0.5ms
- No external dependencies, no ML inference
- Token estimation via character count heuristic (not full tokenization)

## Acceptance Criteria
- [ ] Routes based on vision, tools, JSON mode requirements
- [ ] Token count estimation from message array
- [ ] Routing overhead remains < 1ms total
- [ ] No false negatives (never route to incapable backend)
```

---

### F16: Quality Tracking & Backend Profiling
```
Create a spec for: Quality Tracking & Backend Profiling (F16)

## Feature Description
Build performance profiles for each model+backend combination using rolling
window statistics. Profiles feed into the router scoring algorithm.

## Constitution Alignment
- Principle X (Precise Measurement): Track real metrics, not assumptions
- Principle V (Intelligent Routing): Use data to improve routing decisions

## Tracked Metrics
- Response latency: P50, P95, P99 per model+backend
- Tokens per second: throughput per model+backend
- Error rate: errors / total per model+backend
- Time to first token: streaming responsiveness

## Acceptance Criteria
- [ ] Rolling window statistics (1h, 24h) per model+backend
- [ ] Quality scores integrated into router scoring
- [ ] Degraded backends automatically deprioritized
- [ ] Metrics exposed via Prometheus and /v1/stats
```

---

### F17: Embeddings API
```
Create a spec for: Embeddings API (F17)

## Feature Description
Support the OpenAI Embeddings API across backends that offer embedding models.

## Endpoint
POST /v1/embeddings — OpenAI-compatible request/response format

## Acceptance Criteria
- [ ] Route embedding requests to capable backends
- [ ] Support batch embedding requests
- [ ] OpenAI-compatible request/response format
- [ ] Embeddings backends tracked separately in registry
```

---

### F18: Request Queuing & Prioritization
```
Create a spec for: Request Queuing & Prioritization (F18)

## Feature Description
When all backends are busy, queue requests with configurable timeout and priority
instead of immediately returning 503. Priority levels allow critical requests
to preempt best-effort ones.

## Constitution Alignment
- Principle IX (Explicit Contracts): Queued requests get actionable 503 with ETA on timeout
- Principle VI (Resilient): Queuing is a resilience mechanism, not a bottleneck

## Configuration
```toml
[queuing]
enabled = true
max_queue_size = 100
default_timeout_seconds = 30
priority_header = "X-Nexus-Priority"
```

## Behavior
- Queue fills → oldest low-priority requests dropped first
- Timeout exceeded → 503 with eta_seconds
- Tier-equivalent fallback attempted before queuing

## Acceptance Criteria
- [ ] Bounded queue with configurable max size
- [ ] Priority levels via X-Nexus-Priority header
- [ ] Timeout with actionable 503 (includes ETA)
- [ ] Queue depth exposed in Prometheus metrics
```

---

## v0.5 Feature Prompts

### F19: Pre-warming & Fleet Intelligence
```
Create a spec for: Pre-warming & Fleet Intelligence (F19)

## Feature Description
Predict model demand and proactively recommend loading models on idle nodes.
v0.5 is a suggestion system — recommendations require admin/policy approval.

## Constitution Alignment
- Principle X (Precise Measurement): VRAM headroom is tracked, not assumed
- Principle IX (Explicit Contracts): Pre-warming never disrupts active workloads

## Design Constraints
- Never evict a hot model for a prediction
- Only use idle capacity (configurable headroom %)
- Suggestion-first: recommend via API/logs, admin approves
- Track backend VRAM usage via Ollama /api/ps and similar endpoints

## Acceptance Criteria
- [ ] Tracks model request frequency over time
- [ ] Reports pre-warming recommendations via API and logs
- [ ] Respects VRAM headroom budget (configurable %)
- [ ] Never disrupts active model serving
```

---

### F20: Model Lifecycle Management
```
Create a spec for: Model Lifecycle Management (F20)

## Feature Description
Control model loading, unloading, and migration across the fleet via Nexus API.

## Acceptance Criteria
- [ ] API to trigger model load/unload on specific backends
- [ ] Model migration (unload from A, load on B)
- [ ] Status tracking for loading operations
- [ ] Integrates with pre-warming recommendations (F19)
```

---

### F21: Multi-Tenant Support
```
Create a spec for: Multi-Tenant Support (F21)

## Feature Description
API key-based authentication with per-tenant quotas, model access control,
and usage tracking. Authentication is optional and off by default.

## Constitution Alignment
- Principle I (Zero Configuration): Auth is opt-in, works without it
- Principle VII (Local-First): No external auth provider required

## Acceptance Criteria
- [ ] API key authentication (optional, off by default)
- [ ] Per-tenant usage tracking and quotas
- [ ] Model access control lists per tenant
- [ ] Usage reporting via metrics and API
- [ ] Works with existing zero-config setup when auth is disabled
```

---

### F22: Rate Limiting
```
Create a spec for: Rate Limiting (F22)

## Feature Description
Per-backend and per-tenant rate limiting using token bucket algorithm.

## Acceptance Criteria
- [ ] Per-backend request rate limits (configurable)
- [ ] Per-tenant rate limits (when multi-tenant enabled)
- [ ] Token bucket algorithm with burst support
- [ ] 429 Too Many Requests with Retry-After header
- [ ] Rate limit metrics exposed via Prometheus
```
