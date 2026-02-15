# Feature Specification: NII Extraction — Nexus Inference Interface

**Feature Branch**: `012-nii-extraction`
**Created**: 2026-02-15
**Status**: Draft
**Input**: User description: "Extract the Nexus Inference Interface (NII) from the existing monolithic codebase. Define the InferenceAgent trait and implement built-in agents for all supported backend types. This is the architectural foundation that enables F12-F14 (v0.3) and all subsequent features."
**RFC Reference**: RFC-001 v2 — "Platform Architecture — From Monolithic Router to Controller/Agent Platform" (approved 2026-02-15)

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Transparent Agent Abstraction (Priority: P1)

Nexus operators currently configure backends by type (Ollama, LM Studio, vLLM, etc.) in TOML and Nexus handles health checking, model discovery, and request forwarding. After the NII extraction, the exact same configuration must produce the exact same behavior — users should not notice any change. Internally, each backend is now represented as an `InferenceAgent` implementation, eliminating scattered `match backend_type {}` branching across health/, api/, and registry/ modules.

**Why this priority**: This is the entire purpose of Phase 1. If existing behavior changes or breaks, the extraction failed. Every subsequent feature (F12 Cloud, F13 Privacy, F14 Budget) depends on this abstraction being correct and invisible.

**Independent Test**: Configure Nexus with multiple backend types (Ollama, LM Studio, generic OpenAI-compatible), start Nexus, verify health checks discover models, `/v1/models` lists them, and `/v1/chat/completions` routes and completes requests — identical to pre-extraction behavior.

**Acceptance Scenarios**:

1. **Given** a TOML config with an Ollama backend, **When** Nexus starts and the health checker runs, **Then** models are discovered via `/api/tags`, enriched via `/api/show`, and registered with correct capabilities (vision, tools, context_length) — identical to current behavior
2. **Given** a TOML config with an LM Studio backend, **When** Nexus starts, **Then** models are discovered via `/v1/models` and registered — identical to current behavior
3. **Given** a TOML config with a generic OpenAI-compatible backend (vLLM, exo, llama.cpp), **When** Nexus starts, **Then** models are discovered and health is checked — identical to current behavior
4. **Given** any configured backend, **When** a chat completion request is sent, **Then** the request is forwarded to the backend and the response (streaming or non-streaming) is returned — identical to current behavior
5. **Given** the existing TOML configuration format, **When** upgrading to the NII version, **Then** zero configuration changes are required

---

### User Story 2 - Agent-Based Health Checking (Priority: P1)

The health checker currently uses `match backend.backend_type {}` to determine which endpoint to call and which response parser to use. After extraction, the health checker calls `agent.health_check()` and `agent.list_models()` uniformly, and each agent implementation encapsulates its backend-specific logic.

**Why this priority**: Health checking is the core discovery loop — it runs every N seconds and populates the registry. If this breaks, all routing breaks. The agent abstraction must handle this correctly for all backend types.

**Independent Test**: Start Nexus with mixed backends, verify health_check_interval triggers uniform `agent.health_check()` calls, each agent returns correct HealthStatus, and `agent.list_models()` returns properly enriched models. Stop a backend, verify it transitions to Unhealthy via the agent.

**Acceptance Scenarios**:

1. **Given** an OllamaAgent, **When** health_check() is called, **Then** it calls `GET /api/tags` and returns HealthStatus::Healthy with model count, or HealthStatus::Unhealthy on failure
2. **Given** an OllamaAgent, **When** list_models() is called, **Then** it calls `GET /api/tags` then `POST /api/show` per model to enrich capabilities (vision, tools, context_length)
3. **Given** an LMStudioAgent or GenericOpenAIAgent, **When** health_check() is called, **Then** it calls `GET /v1/models` and returns HealthStatus::Healthy with model count
4. **Given** any agent, **When** the backend is unreachable, **Then** health_check() returns `Err(AgentError::Network(...))` and the health checker marks the backend Unhealthy
5. **Given** the health checker running, **When** it iterates backends, **Then** it calls `agent.health_check()` and `agent.list_models()` without any `match backend_type {}` branching

---

### User Story 3 - Agent-Based Request Forwarding (Priority: P1)

The completions handler currently builds HTTP requests manually in `proxy_request()` and forwards them to `{backend.url}/v1/chat/completions`. After extraction, it calls `agent.chat_completion()` or `agent.chat_completion_stream()`, and each agent handles its own HTTP construction, URL formation, and response parsing.

**Why this priority**: Request forwarding is the critical data path. Latency overhead must remain < 5ms. The agent abstraction must not add measurable overhead or break streaming.

**Independent Test**: Send both streaming and non-streaming chat completion requests through Nexus with different backend types. Verify responses are identical to pre-extraction, SSE streaming works correctly, and `X-Nexus-*` headers are still present.

**Acceptance Scenarios**:

1. **Given** a non-streaming request routed to any agent, **When** agent.chat_completion() is called, **Then** it returns a ChatCompletionResponse matching the OpenAI format
2. **Given** a streaming request routed to any agent, **When** agent.chat_completion_stream() is called, **Then** it returns a stream of SSE chunks matching the OpenAI streaming format
3. **Given** the Authorization header is present on the incoming request, **When** the agent forwards the request, **Then** the Authorization header is forwarded to the backend
4. **Given** a backend returns an error (timeout, 500, malformed response), **When** agent.chat_completion() is called, **Then** it returns an appropriate AgentError and the completions handler retries/fails over as before
5. **Given** any agent, **When** measuring end-to-end latency, **Then** the agent abstraction adds < 0.1ms overhead compared to the direct HTTP call

---

### User Story 4 - Registry Integration with Dual Storage (Priority: P1)

During the Phase 1 migration, the Registry must store both the existing `Backend` struct (for dashboard, metrics, CLI compatibility) and the new `Arc<dyn InferenceAgent>` (for health checking and request forwarding). This dual storage ensures zero disruption to existing consumers while enabling the new agent-based flow.

**Why this priority**: The Registry is the source of truth. If dual storage breaks existing consumers (dashboard, metrics, CLI), the migration fails. This must coexist cleanly.

**Independent Test**: Add a backend via config, verify the Registry stores both `Backend` and `Arc<dyn InferenceAgent>`, verify the dashboard and metrics still read from `Backend`/`BackendView`, and verify health checker and completions handler use the agent.

**Acceptance Scenarios**:

1. **Given** a backend is registered, **When** querying the Registry, **Then** both `get_backend()` (returns &Backend) and `get_agent()` (returns Arc<dyn InferenceAgent>) work for the same backend ID
2. **Given** the dashboard requests backend data, **When** it reads from Registry, **Then** it receives BackendView as before — no changes required
3. **Given** the metrics system requests stats, **When** it reads from Registry, **Then** it receives the same data as before — no changes required
4. **Given** the health checker updates backend status, **When** it uses the agent, **Then** the Backend struct's status, models, and timestamps are also updated
5. **Given** the agent factory creates an agent, **When** add_backend() is called, **Then** the Registry stores the agent alongside the Backend

---

### User Story 5 - Forward-Looking Trait Methods with Safe Defaults (Priority: P2)

The InferenceAgent trait includes methods for future features (embeddings, load_model, unload_model, count_tokens, resource_usage) with default implementations that return `Unsupported` or safe fallback values. This ensures v0.4/v0.5 features won't require breaking trait changes.

**Why this priority**: While these methods aren't exercised in Phase 1, defining them now prevents a breaking trait change when F14 (Budget/count_tokens), F17 (Embeddings), or F20 (Lifecycle) are implemented. The cost of adding them now is minimal.

**Independent Test**: Call each default method on any agent, verify embeddings returns `Err(AgentError::Unsupported)`, load_model returns `Err(AgentError::Unsupported)`, count_tokens returns `TokenCount::Heuristic(chars/4)`, resource_usage returns empty `ResourceUsage::default()`.

**Acceptance Scenarios**:

1. **Given** any built-in agent, **When** embeddings() is called, **Then** it returns `Err(AgentError::Unsupported("embeddings"))` (overridden in F17)
2. **Given** any built-in agent, **When** load_model() is called, **Then** it returns `Err(AgentError::Unsupported("load_model"))` (overridden for Ollama in F20)
3. **Given** any built-in agent, **When** count_tokens("hello world") is called, **Then** it returns `TokenCount::Heuristic(2)` (11 chars / 4 = 2) — overridden for OpenAI in F14
4. **Given** any built-in agent, **When** resource_usage() is called, **Then** it returns `ResourceUsage::default()` with all None/zero fields (overridden for Ollama in F19)

---

### User Story 6 - Agent Factory from Configuration (Priority: P2)

An agent factory function creates the correct agent implementation from the existing `BackendConfig` TOML structure. The factory maps `BackendType` to the corresponding agent struct, passing the backend URL, HTTP client, and any relevant configuration. Users never interact with agents directly — the factory is internal.

**Why this priority**: The factory is the bridge between configuration and the agent abstraction. Without it, backend registration requires manual agent construction. It must handle all existing BackendType variants.

**Independent Test**: Call `create_agent()` with each BackendType variant (Ollama, LMStudio, VLLM, LlamaCpp, Exo, Generic, OpenAI), verify the correct agent type is returned, and verify the agent's id() and profile() reflect the config.

**Acceptance Scenarios**:

1. **Given** a BackendConfig with `backend_type: Ollama`, **When** create_agent() is called, **Then** an OllamaAgent is returned with the correct URL and ID
2. **Given** a BackendConfig with `backend_type: LMStudio`, **When** create_agent() is called, **Then** an LMStudioAgent is returned
3. **Given** a BackendConfig with `backend_type: VLLM`, `Exo`, `LlamaCpp`, or `Generic`, **When** create_agent() is called, **Then** a GenericOpenAIAgent is returned configured for that backend type
4. **Given** a BackendConfig with `backend_type: OpenAI`, **When** create_agent() is called, **Then** an OpenAIAgent is returned (cloud-capable, API key from config)
5. **Given** any BackendConfig, **When** create_agent() is called, **Then** the returned agent shares the same `reqwest::Client` (connection pooling)

---

### Edge Cases

- What happens when an agent's health_check() times out? → Returns `Err(AgentError::Timeout(...))`, health checker marks backend Unhealthy, identical to current timeout handling.
- What happens when Ollama's `/api/show` enrichment fails for one model but succeeds for others? → That model is registered with heuristic capabilities (name-based detection), same as current behavior.
- What happens when a backend URL is misconfigured (unreachable)? → Agent creation succeeds (it's just config), but health_check() fails, marking it Unhealthy.
- What happens when streaming is interrupted mid-response (client disconnects)? → The agent's future is dropped; agents must be cancellation-safe (abort in-flight HTTP requests).
- What happens when the same backend is registered from both static config and mDNS? → Same as current: deduplication by ID in Registry, agent is created once.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST define an `InferenceAgent` trait in `src/agent/mod.rs` with all methods specified in RFC-001 Section 1
- **FR-002**: System MUST implement `OllamaAgent` that encapsulates Ollama-specific health checking (`/api/tags`), model enrichment (`/api/show`), and chat completion
- **FR-003**: System MUST implement `OpenAIAgent` that supports API key authentication via Bearer token, targeting cloud OpenAI-compatible endpoints
- **FR-004**: System MUST implement `LMStudioAgent` that handles LM Studio's OpenAI-compatible API with its specific model discovery behavior
- **FR-005**: System MUST implement `GenericOpenAIAgent` that handles vLLM, exo, llama.cpp, and any OpenAI-compatible backend
- **FR-006**: System MUST provide a `create_agent()` factory function that maps `BackendType` to the correct agent implementation
- **FR-007**: System MUST store `Arc<dyn InferenceAgent>` in the Registry alongside the existing `Backend` struct (dual storage)
- **FR-008**: Health checker MUST delegate to `agent.health_check()` and `agent.list_models()` instead of type-specific match branching
- **FR-009**: Completions handler MUST delegate to `agent.chat_completion()` / `agent.chat_completion_stream()` instead of direct HTTP calls
- **FR-010**: All default trait methods MUST return safe fallback values (`Unsupported`, `Heuristic`, `default()`)
- **FR-011**: Existing TOML configuration format MUST NOT change
- **FR-012**: All 468+ existing tests MUST pass without modification
- **FR-013**: The `Authorization` header MUST be forwarded to backends when present on the incoming request
- **FR-014**: Agent methods MUST be cancellation-safe (dropped futures clean up resources)

### Key Entities

- **InferenceAgent**: Trait defining the contract between Nexus core and any LLM backend. Methods cover discovery (health, models), inference (chat, stream, embeddings), lifecycle (load/unload), and metering (tokens, resources).
- **AgentProfile**: Metadata about an agent — its type string, optional version, privacy zone, and capability flags.
- **AgentError**: Error type for all agent operations — Network, Timeout, Upstream (backend returned error), Unsupported (method not implemented), InvalidResponse (parse failure).
- **HealthStatus**: Agent health with Loading state for model lifecycle (Healthy, Unhealthy, Loading { percent, eta_ms }, Draining).
- **TokenCount**: Tiered result — Exact(u32) from a real tokenizer, or Heuristic(u32) from chars/4 estimation.
- **ResourceUsage**: VRAM, pending requests, latency, loaded models — for fleet intelligence (F19).
- **PrivacyZone**: Restricted (local-only) or Open (can receive cloud overflow) — for privacy zones (F13).
- **ModelCapability**: Extends existing `Model` struct with capability_tier field for F13 tier routing.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All 468+ existing tests pass without modification after the extraction
- **SC-002**: Zero `match backend_type {}` branching remains in `src/health/mod.rs` for endpoint selection and response parsing
- **SC-003**: Zero direct HTTP request construction remains in `src/api/completions.rs` `proxy_request()` — delegated to agents
- **SC-004**: Agent creation overhead is < 1ms per backend (verified via unit test)
- **SC-005**: Request forwarding overhead via agent abstraction is < 0.1ms compared to direct HTTP (verified via benchmark or tracing)
- **SC-006**: Memory overhead per agent is < 5KB beyond existing Backend struct
- **SC-007**: Binary size increase is < 500KB (no heavy new dependencies in Phase 1)
- **SC-008**: Each agent module has a `mod tests` block with at least 5 unit tests using mock HTTP backends
- **SC-009**: Dashboard, metrics, CLI, mDNS discovery all function identically after extraction (no regressions)
- **SC-010**: The InferenceAgent trait compiles and is object-safe (`Arc<dyn InferenceAgent>` works)
