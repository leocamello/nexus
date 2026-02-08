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

| ID | Feature | Priority | Prompt Section |
|----|---------|----------|----------------|
| F01 | Core API Gateway | P0 | [Link](#f01-core-api-gateway) |
| F02 | Backend Registry | P0 | [Link](#f02-backend-registry) |
| F03 | Health Checker | P0 | [Link](#f03-health-checker) |
| F04 | CLI and Configuration | P0 | [Link](#f04-cli-and-configuration) |
| F05 | mDNS Discovery | P1 | [Link](#f05-mdns-discovery) |
| F06 | Intelligent Router | P1 | [Link](#f06-intelligent-router) |
| F07 | Model Aliases | P1 | [Link](#f07-model-aliases) |
| F08 | Fallback Chains | P1 | [Link](#f08-fallback-chains) |
| F09 | Request Metrics | P2 | [Link](#f09-request-metrics) |
| F10 | Web Dashboard | P2 | [Link](#f10-web-dashboard) |

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

### Phase 1: MVP (Weeks 1-3)
1. **F02: Backend Registry** - Foundation for all other features
2. **F03: Health Checker** - Keeps registry up to date
3. **F01: Core API Gateway** - The main user-facing interface
4. **F04: CLI and Configuration** (basic) - Start server, list backends

### Phase 2: Discovery (Weeks 4-5)
5. **F05: mDNS Discovery** - Zero-config experience

### Phase 3: Intelligence (Weeks 6-7)
6. **F06: Intelligent Router** - Smart backend selection
7. **F07: Model Aliases** - OpenAI compatibility
8. **F08: Fallback Chains** - Resilience

### Phase 4: Polish (Weeks 8-9)
9. **F04: CLI and Configuration** (complete) - All commands
10. **F09: Request Metrics** - Observability
11. **F10: Web Dashboard** - Visual monitoring
