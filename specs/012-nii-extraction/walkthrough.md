# NII Extraction - Code Walkthrough

**Feature**: F12 - NII Agent Abstraction (RFC-001 Phase 1)  
**Audience**: Junior developers joining the project  
**Last Updated**: 2025-07-22

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: mod.rs - The InferenceAgent Trait](#file-1-modrs---the-inferenceagent-trait)
4. [File 2: types.rs - Supporting Types](#file-2-typesrs---supporting-types)
5. [File 3: error.rs - What Can Go Wrong](#file-3-errorrs---what-can-go-wrong)
6. [File 4: factory.rs - Agent Creation](#file-4-factoryrs---agent-creation)
7. [File 5: ollama.rs - Ollama Agent](#file-5-ollamars---ollama-agent)
8. [File 6: openai.rs - OpenAI Agent](#file-6-openairs---openai-agent)
9. [File 7: lmstudio.rs - LM Studio Agent](#file-7-lmstudiors---lm-studio-agent)
10. [File 8: generic.rs - Generic OpenAI Agent](#file-8-genericrs---generic-openai-agent)
11. [File 9: registry/mod.rs - Dual Storage](#file-9-registrymodrs---dual-storage)
12. [File 10: health/mod.rs - Agent-Based Health Checking](#file-10-healthmodrs---agent-based-health-checking)
13. [File 11: api/completions.rs - Agent-Based Request Forwarding](#file-11-apicompletionsrs---agent-based-request-forwarding)
14. [Understanding the Tests](#understanding-the-tests)
15. [Key Rust Concepts](#key-rust-concepts)
16. [Common Patterns in This Codebase](#common-patterns-in-this-codebase)
17. [Next Steps](#next-steps)

---

## The Big Picture

Think of the NII (Nexus Inference Interface) as a **universal power adapter for AI backends**. Before NII, every time Nexus needed to talk to a different backend — Ollama, OpenAI, vLLM, LM Studio — it had to know each backend's specific quirks: different health check URLs, different model listing formats, different streaming protocols. This led to `match backend_type { ... }` branches scattered throughout the codebase.

NII replaces all of that with a single `InferenceAgent` trait. Each backend gets its own implementation, but the rest of Nexus just calls `agent.health_check()` or `agent.chat_completion()` without caring what's behind it.

### What Problem Does This Solve?

Before NII, adding a new backend type meant modifying code in **three separate places**:

1. Health checker — add a new match arm for the health check URL
2. Completions handler — add a new match arm for request forwarding
3. Model listing — add backend-specific parsing logic

With NII, adding a new backend means creating **one file** — a new agent implementation — and adding one line to the factory.

### How Agents Fit Into Nexus

```
┌────────────────────────────────────────────────────────────────────────┐
│                             Nexus                                      │
│                                                                        │
│  ┌──────────┐     ┌──────────┐     ┌──────────────────────────────┐   │
│  │   API    │────▶│  Router  │────▶│  Registry (Dual Storage)     │   │
│  │ Gateway  │     │          │     │                              │   │
│  └──────────┘     └──────────┘     │  backends: DashMap<Backend>  │   │
│       │                            │  agents:   DashMap<Agent>    │   │
│       │                            └──────────────┬───────────────┘   │
│       │                                           │                   │
│       │  ┌────────────────────────────────────────┘                   │
│       │  │                                                            │
│       ▼  ▼                                                            │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │              InferenceAgent Trait (you are here!)                │  │
│  │                                                                 │  │
│  │  ┌──────────┐  ┌──────────┐  ┌───────────┐  ┌──────────────┐  │  │
│  │  │ Ollama   │  │ OpenAI   │  │ LM Studio │  │ Generic      │  │  │
│  │  │ Agent    │  │ Agent    │  │ Agent     │  │ (vLLM, exo,  │  │  │
│  │  │          │  │          │  │           │  │  llama.cpp)  │  │  │
│  │  └──────────┘  └──────────┘  └───────────┘  └──────────────┘  │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                           │                                           │
│                           ▼                                           │
│                  ┌──────────────────┐                                  │
│                  │  LLM Backends    │                                  │
│                  │  (actual servers) │                                  │
│                  └──────────────────┘                                  │
└────────────────────────────────────────────────────────────────────────┘
```

### Key Design Decisions

| Decision | Why |
|----------|-----|
| Rust trait, not gRPC | Zero overhead, compile-time safety, single binary |
| `Arc<dyn InferenceAgent>` | Object-safe trait object shared across threads |
| Optional methods with defaults | Future capabilities (embeddings, lifecycle) without breaking existing agents |
| Heuristic token counting default | Every agent can count tokens — exact implementations override the chars/4 fallback |
| Factory function, not constructor | Centralizes creation logic, handles API key extraction |
| Agent + Backend dual storage | Dashboard/metrics read Backend; health checks and completions use Agent |

### Request Lifecycle With Agents

Here's what happens when a chat request arrives:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                   Request Lifecycle (with NII)                           │
│                                                                         │
│  Client                                                                 │
│    │                                                                    │
│    │  POST /v1/chat/completions { model: "llama3:8b" }                  │
│    ▼                                                                    │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │  api/completions.rs :: handle()                                    │ │
│  │                                                                    │ │
│  │  ① Router selects backend for "llama3:8b"                          │ │
│  │  │                                                                 │ │
│  │  ② registry.get_agent(backend.id)                                  │ │
│  │  │  └─ Returns Arc<dyn InferenceAgent>                             │ │
│  │  │                                                                 │ │
│  │  ③ agent.chat_completion(request, headers)                         │ │
│  │  │  └─ Agent handles URL construction, auth, error mapping         │ │
│  │  │                                                                 │ │
│  │  ├─ Non-streaming: Returns ChatCompletionResponse directly         │ │
│  │  └─ Streaming: Returns BoxStream<StreamChunk>                      │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                         │
│  Meanwhile, in the background:                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │  health/mod.rs :: check_backend()                                  │ │
│  │                                                                    │ │
│  │  ① registry.get_agent(backend.id)                                  │ │
│  │  ② agent.health_check() → HealthStatus::Healthy { model_count }    │ │
│  │  ③ agent.list_models() → Vec<ModelCapability>                      │ │
│  │  ④ Convert ModelCapability → Model, update registry                │ │
│  └────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## File Structure

```
src/
├── agent/                       ← NEW MODULE (this feature)
│   ├── mod.rs                   # InferenceAgent trait definition
│   ├── types.rs                 # AgentProfile, HealthStatus, ModelCapability, etc.
│   ├── error.rs                 # AgentError enum
│   ├── factory.rs               # create_agent() function
│   ├── ollama.rs                # OllamaAgent implementation
│   ├── openai.rs                # OpenAIAgent implementation
│   ├── lmstudio.rs              # LMStudioAgent implementation
│   └── generic.rs               # GenericOpenAIAgent (vLLM, llama.cpp, exo)
├── registry/
│   └── mod.rs                   # MODIFIED: added agents DashMap (dual storage)
├── health/
│   └── mod.rs                   # MODIFIED: agent-based health checking
├── api/
│   └── completions.rs           # MODIFIED: agent-based request forwarding
└── lib.rs                       # MODIFIED: added `pub mod agent;`

tests/
└── agent_integration.rs         # NEW: 8 integration tests for dual storage
```

---

## File 1: mod.rs - The InferenceAgent Trait

This is the heart of NII. It defines the contract that every backend agent must fulfill.

### The Trait Definition

```rust
#[async_trait]
pub trait InferenceAgent: Send + Sync + 'static {
    // ── Identity (synchronous) ──────────────────
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn profile(&self) -> AgentProfile;

    // ── Discovery & Health (required) ───────────
    async fn health_check(&self) -> Result<HealthStatus, AgentError>;
    async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError>;

    // ── Inference (required) ────────────────────
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
        headers: Option<&HeaderMap>,
    ) -> Result<ChatCompletionResponse, AgentError>;

    async fn chat_completion_stream(
        &self,
        request: ChatCompletionRequest,
        headers: Option<&HeaderMap>,
    ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError>;

    // ── Optional Capabilities (with defaults) ───
    async fn embeddings(&self, _input: Vec<String>) -> Result<Vec<Vec<f32>>, AgentError> {
        Err(AgentError::Unsupported("embeddings"))
    }
    async fn load_model(&self, _model_id: &str) -> Result<(), AgentError> {
        Err(AgentError::Unsupported("load_model"))
    }
    async fn unload_model(&self, _model_id: &str) -> Result<(), AgentError> {
        Err(AgentError::Unsupported("unload_model"))
    }
    async fn count_tokens(&self, _model_id: &str, text: &str) -> TokenCount {
        TokenCount::Heuristic((text.len() / 4) as u32)
    }
    async fn resource_usage(&self) -> ResourceUsage {
        ResourceUsage::default()
    }
}
```

**What's happening here:**

- `Send + Sync + 'static` — Required because agents are stored in `Arc` and shared across tokio tasks on different threads.
- `#[async_trait]` — The `async_trait` crate enables async methods in trait objects. Without it, Rust can't use `async fn` in `dyn Trait`.
- **Required methods** (no body) — Every agent must implement identity, health, and inference.
- **Optional methods** (with default body) — Future features like embeddings (v0.4) and model lifecycle (v0.5) return `Unsupported` by default. Agents override these when they gain the capability.
- **Heuristic token counting** — The `count_tokens` default divides character count by 4. This is surprisingly accurate for English text and provides a fallback when no tokenizer is available.

### Method Categories

| Category | Methods | When Called |
|----------|---------|-------------|
| Identity | `id()`, `name()`, `profile()` | Logging, metrics labels, UI display |
| Discovery | `health_check()`, `list_models()` | Background health check loop (every 10s) |
| Inference | `chat_completion()`, `chat_completion_stream()` | Per-request, from completions handler |
| Future | `embeddings()`, `load_model()`, `unload_model()`, `count_tokens()`, `resource_usage()` | v0.3+ features |

---

## File 2: types.rs - Supporting Types

This file defines the data structures that agents return and the metadata that describes them.

### AgentProfile — Who Is This Agent?

```rust
pub struct AgentProfile {
    pub backend_type: String,          // "ollama", "openai", "lmstudio", "vllm", etc.
    pub version: Option<String>,       // "0.1.29" (from backend), or None
    pub privacy_zone: PrivacyZone,     // Restricted (local) or Open (cloud)
    pub capabilities: AgentCapabilities, // Feature flags
}
```

Think of `AgentProfile` as the agent's **business card**. It tells the rest of Nexus what type of backend this is, what privacy zone it belongs to, and what optional features it supports.

### PrivacyZone — Local vs Cloud

```rust
pub enum PrivacyZone {
    Restricted,  // Local-only: Ollama, LM Studio, vLLM, llama.cpp, exo
    Open,        // Cloud: OpenAI (can receive overflow from local)
}
```

This enum is the foundation for F13 (Privacy Zones, v0.3). Local backends are `Restricted` — their data never flows to the cloud. Cloud backends are `Open` — they can receive overflow when local backends are at capacity.

### HealthStatus — What State Is the Backend In?

```rust
pub enum HealthStatus {
    Healthy { model_count: usize },     // Ready, with N models available
    Unhealthy,                          // Failed health check
    Loading { model_id: String,         // Loading a model (F20, v0.5)
              percent: u8,
              eta_ms: Option<u64> },
    Draining,                           // Accepting no new requests
}
```

```
                    ┌──────────┐
                    │ Unknown  │ (initial state)
                    └────┬─────┘
                         │ health_check()
                         ▼
              ┌──────────────────────┐
              │                      │
              ▼                      ▼
        ┌───────────┐         ┌───────────┐
        │  Healthy  │◀───────▶│ Unhealthy │
        │ {count: 3}│         │           │
        └─────┬─────┘         └───────────┘
              │
              ▼
        ┌───────────┐         ┌───────────┐
        │  Loading  │         │ Draining  │
        │ {50%, 2s} │         │           │
        └───────────┘         └───────────┘
```

### ModelCapability — What Can a Model Do?

```rust
pub struct ModelCapability {
    pub id: String,                    // "llama3:70b"
    pub name: String,                  // "Llama 3 70B"
    pub context_length: u32,           // 8192
    pub supports_vision: bool,         // Can process images?
    pub supports_tools: bool,          // Can use function calling?
    pub supports_json_mode: bool,      // Can force JSON output?
    pub max_output_tokens: Option<u32>,// Output limit (if any)
    pub capability_tier: Option<u8>,   // Phase 2: For tiered routing (F13)
}
```

`ModelCapability` extends the existing `Model` struct with a `capability_tier` field for future F13 routing. Bidirectional conversion (`From<Model>` and `Into<Model>`) keeps the two types compatible.

### TokenCount — How Accurate Is Our Count?

```rust
pub enum TokenCount {
    Exact(u32),      // From real tokenizer (tiktoken, HuggingFace)
    Heuristic(u32),  // From chars/4 estimate
}
```

This is a **tiered accuracy** pattern. The caller can check `token_count.is_exact()` to decide how much to trust the number. Budget reconcilers (F14, v0.3) will use this to decide cost estimates.

### StreamChunk — Streaming Data Wrapper

```rust
pub struct StreamChunk {
    pub data: String,  // Raw SSE data (JSON or "[DONE]")
}
```

A thin wrapper around the raw SSE chunk data. Each chunk is either a JSON-encoded partial completion response or the literal `[DONE]` sentinel.

---

## File 3: error.rs - What Can Go Wrong

```rust
pub enum AgentError {
    Network(String),                    // DNS, connection refused
    Timeout(u64),                       // Deadline exceeded (ms)
    Upstream { status: u16,             // Backend 4xx/5xx
               message: String },
    Unsupported(&'static str),          // Method not available
    InvalidResponse(String),            // Response format mismatch
    Configuration(String),              // Config errors (e.g., missing API key)
}
```

**Why not just use `reqwest::Error`?** Because `AgentError` normalizes errors across all backends. Whether Ollama returns a connection timeout or OpenAI returns a 429 rate limit, the completions handler sees the same error type and maps it to the appropriate HTTP response.

### Error Mapping to HTTP Responses

| AgentError | HTTP Status | When |
|-----------|-------------|------|
| `Network(...)` | 502 Bad Gateway | Backend unreachable |
| `Timeout(ms)` | 504 Gateway Timeout | Request deadline exceeded |
| `Upstream { status: 429, ... }` | 502 Bad Gateway | Backend rate-limited |
| `Upstream { status: 500, ... }` | 502 Bad Gateway | Backend internal error |
| `Unsupported(method)` | 501 Not Implemented | Feature not available |
| `InvalidResponse(...)` | 502 Bad Gateway | Response parse failure |
| `Configuration(...)` | 500 Internal Server Error | Misconfigured backend |

---

## File 4: factory.rs - Agent Creation

The factory is a single function that creates the right agent implementation based on `BackendType`:

```rust
pub fn create_agent(
    id: String,
    name: String,
    url: String,
    backend_type: BackendType,
    client: Arc<Client>,
    metadata: HashMap<String, String>,
) -> Result<Arc<dyn InferenceAgent>, AgentError> {
    match backend_type {
        BackendType::Ollama   => Ok(Arc::new(OllamaAgent::new(id, name, url, client))),
        BackendType::OpenAI   => {
            let api_key = /* extract from metadata or env var */;
            Ok(Arc::new(OpenAIAgent::new(id, name, url, api_key, client)))
        }
        BackendType::LMStudio => Ok(Arc::new(LMStudioAgent::new(id, name, url, client))),
        BackendType::VLLM | BackendType::LlamaCpp
        | BackendType::Exo | BackendType::Generic => {
            Ok(Arc::new(GenericOpenAIAgent::new(id, name, backend_type, url, client)))
        }
    }
}
```

**What's happening here:**

- Every agent gets the same shared `Arc<Client>` — connection pooling across all agents.
- **OpenAI is special** — it requires an API key. The factory checks `metadata["api_key"]` first, then `metadata["api_key_env"]` for an environment variable lookup. Missing key → `AgentError::Configuration`.
- **Generic catches four types** — vLLM, llama.cpp, exo, and generic all speak the same OpenAI-compatible protocol. One implementation handles them all, using `backend_type` to set the correct profile string.

### BackendType → Agent Mapping

| BackendType | Agent | Privacy Zone | Special Handling |
|-------------|-------|-------------|------------------|
| `Ollama` | `OllamaAgent` | Restricted | Native `/api/tags` + `/api/show` enrichment |
| `OpenAI` | `OpenAIAgent` | Open | Bearer token auth, API key required |
| `LMStudio` | `LMStudioAgent` | Restricted | OpenAI-compatible, no auth required |
| `VLLM` | `GenericOpenAIAgent` | Restricted | OpenAI-compatible |
| `LlamaCpp` | `GenericOpenAIAgent` | Restricted | OpenAI-compatible |
| `Exo` | `GenericOpenAIAgent` | Restricted | OpenAI-compatible |
| `Generic` | `GenericOpenAIAgent` | Restricted | OpenAI-compatible |

---

## File 5: ollama.rs - Ollama Agent

The Ollama agent is the most complex implementation because Ollama has its own native API alongside an OpenAI-compatible layer.

### Struct

```rust
pub struct OllamaAgent {
    id: String,
    name: String,
    base_url: String,       // "http://localhost:11434"
    client: Arc<Client>,
}
```

### How It Works

```
┌─────────────────────────────────────────────────────────────────────┐
│                     OllamaAgent Operations                          │
│                                                                     │
│  health_check()                list_models()                        │
│  ═══════════                   ═════════════                        │
│  GET /api/tags                 GET /api/tags → list of model names  │
│  → count models                │                                    │
│  → Healthy { count }           ▼ for each model:                    │
│                                POST /api/show { name: "llama3" }    │
│                                → template, parameters, size         │
│                                → Detect vision, tools, context len  │
│                                                                     │
│  chat_completion()             chat_completion_stream()              │
│  ════════════════              ════════════════════════              │
│  POST /v1/chat/completions     POST /v1/chat/completions            │
│  (Ollama's OpenAI compat)      + stream: true                       │
│  → JSON response               → byte stream → StreamChunk         │
└─────────────────────────────────────────────────────────────────────┘
```

### Model Enrichment

Ollama's `/api/tags` only returns model names and sizes. To discover capabilities (vision, tools, context length), the agent calls `/api/show` per model and applies both **response parsing** and **name heuristics**:

| Detection Method | What It Finds | Example |
|-----------------|---------------|---------|
| `/api/show` template parsing | Vision support | Template contains `image` |
| `/api/show` parameters | Context length | `num_ctx` parameter |
| Name contains "llava", "vision" | Vision support | `llava:13b` |
| Name contains "32k", "128k" | Context length | `mixtral-8x7b-32k` |

### Timeouts

| Operation | Timeout |
|-----------|---------|
| `health_check()` | 5 seconds |
| `list_models()` | 5 seconds |
| `chat_completion()` | 120 seconds |
| `chat_completion_stream()` | 120 seconds |

---

## File 6: openai.rs - OpenAI Agent

The OpenAI agent handles cloud API access with Bearer token authentication.

### Struct

```rust
pub struct OpenAIAgent {
    id: String,
    name: String,
    base_url: String,       // "https://api.openai.com"
    api_key: String,        // "sk-..."
    client: Arc<Client>,
}
```

### Key Differences From Other Agents

| Feature | OpenAI | Others |
|---------|--------|--------|
| Authentication | Bearer token (always) | Header forwarding (optional) |
| Privacy Zone | **Open** (cloud) | Restricted (local) |
| Health Check | `GET /v1/models` with auth | `GET /api/tags` or `/v1/models` |
| Capabilities | Name-based heuristics | Name heuristics + API enrichment |

### OpenAI Model Heuristics

```
Model Name → Capabilities
─────────────────────────────────
gpt-4o, gpt-4-vision   → vision: true
gpt-4, gpt-3.5-turbo   → tools: true
gpt-4-turbo, gpt-4o    → context: 128k
gpt-4-32k              → context: 32k
gpt-4 (base)           → context: 8192
gpt-3.5-turbo          → context: 16384
```

---

## File 7: lmstudio.rs - LM Studio Agent

LM Studio speaks the OpenAI API natively, making this a simpler implementation.

### Struct

```rust
pub struct LMStudioAgent {
    id: String,
    name: String,
    base_url: String,       // "http://localhost:1234"
    client: Arc<Client>,
}
```

**Why not use GenericOpenAIAgent?** LM Studio has specific model naming conventions and loads models differently. Having a dedicated agent allows LM Studio-specific heuristics (e.g., recognizing `TheBloke/` prefixed models).

---

## File 8: generic.rs - Generic OpenAI Agent

This is the **catch-all** agent for any backend that speaks the OpenAI-compatible API: vLLM, llama.cpp, exo, or any unknown backend.

### Struct

```rust
pub struct GenericOpenAIAgent {
    id: String,
    name: String,
    backend_type: BackendType,  // Preserved for profile reporting
    base_url: String,
    client: Arc<Client>,
}
```

The `backend_type` field is stored so the agent profile correctly reports "vllm", "llamacpp", "exo", or "generic" — even though the HTTP protocol is identical.

### Profile Mapping

```rust
fn profile(&self) -> AgentProfile {
    AgentProfile {
        backend_type: match self.backend_type {
            BackendType::VLLM     => "vllm",
            BackendType::LlamaCpp => "llamacpp",
            BackendType::Exo      => "exo",
            _                     => "generic",
        }.to_string(),
        privacy_zone: PrivacyZone::Restricted,
        // ...
    }
}
```

---

## File 9: registry/mod.rs - Dual Storage

The Registry now stores **two things** for each backend: the `Backend` struct (for metrics, dashboard, routing) and the `InferenceAgent` (for actual communication).

### Why Dual Storage?

```
┌───────────────────────────────────────────────────────┐
│                  Registry (Dual Storage)               │
│                                                        │
│  backends: DashMap<String, Backend>                     │
│  ├─ Read by: Dashboard, Metrics, Router, BackendView   │
│  └─ Contains: status, models, latency, pending count   │
│                                                        │
│  agents: DashMap<String, Arc<dyn InferenceAgent>>      │
│  ├─ Read by: Health Checker, Completions Handler       │
│  └─ Contains: HTTP client, URLs, auth, protocol logic  │
│                                                        │
│  Why separate? Because Backend is serializable          │
│  (BackendView), but agents contain Arc<Client>,         │
│  network connections — they can't be serialized.        │
└───────────────────────────────────────────────────────┘
```

### Key Methods

```rust
impl Registry {
    /// Store both backend data and agent in one atomic operation
    pub fn add_backend_with_agent(
        &self,
        backend: Backend,
        agent: Arc<dyn InferenceAgent>,
    ) -> Result<(), RegistryError>;

    /// Get agent for health checks or request forwarding
    pub fn get_agent(&self, id: &str) -> Option<Arc<dyn InferenceAgent>>;

    /// Get all agents for batch health checking
    pub fn get_all_agents(&self) -> Vec<Arc<dyn InferenceAgent>>;

    /// Remove cleans up BOTH backend and agent
    pub fn remove_backend(&self, id: &str) -> Result<(), RegistryError>;
}
```

The `add_backend_with_agent` method checks for duplicates, updates the model index, then inserts both the Backend and agent under the same ID. The `remove_backend` method cleans up both — no orphaned agents.

---

## File 10: health/mod.rs - Agent-Based Health Checking

The health checker was modified to use agents instead of direct HTTP calls, with a legacy fallback for backwards compatibility.

### The Agent-First Pattern

```rust
pub async fn check_backend(&self, backend: &Backend) -> HealthCheckResult {
    let start = Instant::now();

    // Try agent first (NII path)
    if let Some(agent) = self.registry.get_agent(&backend.id) {
        match agent.health_check().await {
            Ok(HealthStatus::Healthy { .. }) => {
                // Agent is healthy — now list models
                match agent.list_models().await {
                    Ok(model_capabilities) => {
                        let models = model_capabilities
                            .into_iter()
                            .map(Model::from)    // ModelCapability → Model
                            .collect();
                        HealthCheckResult::Success { latency_ms, models }
                    }
                    Err(e) => HealthCheckResult::SuccessWithParseError { ... }
                }
            }
            Ok(HealthStatus::Unhealthy) => HealthCheckResult::Failure { ... },
            Ok(HealthStatus::Loading { .. }) => { /* temporarily unhealthy */ },
            Ok(HealthStatus::Draining) => { /* draining status */ },
            Err(e) => HealthCheckResult::Failure { ... },
        }
    } else {
        // Legacy fallback: direct HTTP (no agent registered)
        // ... existing HTTP-based health check code ...
    }
}
```

**Why the fallback?** During migration, some backends might be registered without agents (e.g., from old config paths). The fallback ensures zero breakage during the transition.

### Health Check Flow

```
check_backend(&backend)
       │
       ▼
  get_agent(id) ──── None ───▶ Legacy HTTP fallback
       │
     Some(agent)
       │
       ▼
  agent.health_check()
       │
       ├── Healthy ──▶ agent.list_models()
       │                    │
       │                    ├── Ok(caps) ──▶ Convert to Model ──▶ Success
       │                    └── Err(e)  ──▶ SuccessWithParseError
       │
       ├── Unhealthy ──▶ Failure
       ├── Loading   ──▶ Failure (temporary)
       └── Draining  ──▶ Failure (draining)
```

---

## File 11: api/completions.rs - Agent-Based Request Forwarding

The completions handler uses agents for both non-streaming and streaming requests.

### Non-Streaming (proxy_request)

```rust
async fn proxy_request(
    state: &Arc<AppState>,
    backend: &Backend,
    headers: &HeaderMap,
    request: &ChatCompletionRequest,
) -> Result<ChatCompletionResponse, ApiError> {
    // Try agent (NII path)
    if let Some(agent) = state.registry.get_agent(&backend.id) {
        let response = agent
            .chat_completion(request.clone(), Some(headers))
            .await
            .map_err(ApiError::from_agent_error)?;
        Ok(response)
    } else {
        // Legacy fallback: direct HTTP
        // ... build reqwest::Request manually ...
    }
}
```

### Streaming (create_sse_stream)

```rust
fn create_sse_stream(
    state: Arc<AppState>,
    backend: Arc<Backend>,
    headers: HeaderMap,
    request: ChatCompletionRequest,
) -> impl Stream<Item = Result<Event, Infallible>> {
    async_stream::stream! {
        if let Some(agent) = state.registry.get_agent(&backend_id) {
            match agent.chat_completion_stream(request, Some(&headers)).await {
                Ok(mut stream) => {
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(chunk) if chunk.data == "[DONE]" => {
                                yield Ok(Event::default().data("[DONE]"));
                                break;
                            }
                            Ok(chunk) => yield Ok(Event::default().data(chunk.data)),
                            Err(e) => { /* log error, break */ }
                        }
                    }
                }
                Err(e) => { /* yield error event */ }
            }
        }
    }
}
```

**Key detail**: Both paths forward the original request headers. This is critical for OpenAI backends where the `Authorization` header must pass through from the client to the upstream API.

---

## Understanding the Tests

### Test Distribution

| Category | Count | Location |
|----------|-------|----------|
| Agent factory tests | 16 | `src/agent/factory.rs` |
| Agent implementation tests | 21 | `src/agent/{ollama,openai,lmstudio,generic}.rs` |
| Integration tests | 8 | `tests/agent_integration.rs` |
| **Total agent-related** | **45** | |

### Integration Test Categories

The 8 integration tests in `tests/agent_integration.rs` focus on **dual storage correctness**:

```rust
// 1. Dual storage — both Backend and agent stored
#[tokio::test]
async fn test_dual_storage_stores_both() {
    let registry = Registry::new();
    let backend = test_backend("b1", BackendType::Ollama);
    let agent = create_agent("b1".into(), /* ... */);

    registry.add_backend_with_agent(backend, agent).unwrap();

    assert!(registry.get_backend("b1").is_some());  // ← Backend stored
    assert!(registry.get_agent("b1").is_some());     // ← Agent stored
}

// 2. BackendView unaffected — serialization works without agent data
// 3. Agent identity — correct id() and name() returned
// 4. Mixed backends — Ollama + LMStudio + vLLM coexist
// 5. Cleanup — remove_backend cleans up agent too
// 6. Batch retrieval — get_all_agents returns all
// 7. Duplicate prevention — second add with same ID rejected
// 8. Profile mapping — agent.profile().backend_type matches
```

### Factory Test Coverage

The factory tests cover all backend types and edge cases:

```rust
// Every BackendType creates the right agent
test_create_ollama_agent      → profile.backend_type == "ollama"
test_create_openai_agent      → profile.backend_type == "openai"
test_create_lmstudio_agent    → profile.backend_type == "lmstudio"
test_create_vllm_agent        → profile.backend_type == "vllm"
test_create_llamacpp_agent    → profile.backend_type == "llamacpp"
test_create_exo_agent         → profile.backend_type == "exo"
test_create_generic_agent     → profile.backend_type == "generic"

// OpenAI key handling
test_create_openai_agent_with_direct_key → api_key in metadata
test_create_openai_agent_with_env_key    → api_key_env in metadata
test_create_openai_agent_missing_key     → AgentError::Configuration

// Shared client
test_shared_client → two agents share same Arc<Client>
```

---

## Key Rust Concepts

| Concept | What It Means | Example in This Code |
|---------|---------------|----------------------|
| `#[async_trait]` | Enables async methods in trait objects | `InferenceAgent` trait definition |
| `Arc<dyn Trait>` | Shared ownership of a trait object | `Arc<dyn InferenceAgent>` in registry |
| `BoxStream<'static, T>` | Heap-allocated async stream | Return type of `chat_completion_stream()` |
| `Send + Sync + 'static` | Thread-safe trait bounds | Required for cross-tokio-task sharing |
| `From<A> for B` | Type conversion | `ModelCapability` ↔ `Model` |
| `DashMap<K, V>` | Thread-safe HashMap | `agents` map in Registry |
| `async_stream::stream!` | Macro for creating async streams | SSE streaming in completions |
| `thiserror::Error` | Derive macro for error types | `AgentError` enum |
| Default trait methods | Methods with bodies in traits | `embeddings()`, `count_tokens()` defaults |
| `&'static str` | String literal with infinite lifetime | `AgentError::Unsupported("embeddings")` |

### Object Safety — Why It Matters

```rust
// This works because InferenceAgent is object-safe:
let agent: Arc<dyn InferenceAgent> = Arc::new(OllamaAgent::new(...));

// Object safety requires:
// ✅ No generic methods (all concrete types)
// ✅ No Self in return types (uses concrete types instead)
// ✅ No associated types with complex bounds
// ✅ &self or &mut self receiver (not self by value)
```

If `InferenceAgent` used generics (e.g., `fn process<T: Serialize>(&self, input: T)`), it could **not** be used as `dyn InferenceAgent` — the compiler wouldn't know which monomorphization to call.

### The async_trait Transformation

```rust
// What you write:
#[async_trait]
pub trait InferenceAgent: Send + Sync {
    async fn health_check(&self) -> Result<HealthStatus, AgentError>;
}

// What the compiler sees (simplified):
pub trait InferenceAgent: Send + Sync {
    fn health_check(&self) -> Pin<Box<dyn Future<Output = Result<HealthStatus, AgentError>> + Send>>;
}
```

`async_trait` boxes the future so it can be used in trait objects. This adds one heap allocation per call — negligible compared to the network round-trip.

---

## Common Patterns in This Codebase

### Pattern 1: Agent-First With Legacy Fallback

```rust
// Used in health/mod.rs and api/completions.rs
if let Some(agent) = registry.get_agent(&backend.id) {
    // NII path — use agent
    agent.health_check().await?;
} else {
    // Legacy path — direct HTTP (backwards compatibility)
    client.get(&url).send().await?;
}
```

This pattern ensures **zero breakage** during migration. Old code paths remain as fallbacks while new agent-based code is the preferred path.

### Pattern 2: Factory + Trait Object

```rust
// Creation: specific type → trait object
let agent: Arc<dyn InferenceAgent> = create_agent(
    id, name, url, BackendType::Ollama, client, metadata
)?;

// Usage: callers only see the trait
agent.health_check().await?;
agent.chat_completion(request, headers).await?;

// The caller never knows (or needs to know) it's an OllamaAgent
```

This is the **Strategy pattern** — the factory picks the right strategy based on `BackendType`, and all consumers work through the uniform interface.

### Pattern 3: Type Conversion Bridge

```rust
// Agent returns ModelCapability (richer type with capability_tier)
let capabilities: Vec<ModelCapability> = agent.list_models().await?;

// Health checker needs Model (existing registry type)
let models: Vec<Model> = capabilities
    .into_iter()
    .map(Model::from)    // From<ModelCapability> for Model
    .collect();

// Both directions work:
let cap: ModelCapability = model.into();  // From<Model> for ModelCapability
```

This pattern bridges between the new agent types and the existing registry types without modifying either.

### Pattern 4: Shared Client (Connection Pooling)

```rust
// One reqwest::Client for the entire process
let client = Arc::new(Client::new());

// Every agent gets a clone of the Arc (not a new Client)
let ollama = create_agent("a1", ..., Arc::clone(&client), ...)?;
let openai = create_agent("a2", ..., Arc::clone(&client), ...)?;
let vllm   = create_agent("a3", ..., Arc::clone(&client), ...)?;

// All three agents share TCP connection pools, TLS sessions, etc.
```

### Pattern 5: Heuristic With Accuracy Tag

```rust
// Default: heuristic estimate
let count = agent.count_tokens("model", "Hello world").await;
// → TokenCount::Heuristic(2)  (11 chars / 4 ≈ 2)

// Override in OpenAI agent: exact count (future)
let count = openai_agent.count_tokens("gpt-4", text).await;
// → TokenCount::Exact(157)  (from tiktoken)

// Callers can check accuracy:
if count.is_exact() {
    // Confident in budget calculation
} else {
    // Add safety margin
}
```

---

## Next Steps

Now that you understand the NII agent abstraction, explore:

1. **Backend Registry** (`src/registry/mod.rs`) — Where `Backend` structs live alongside agents in dual storage
2. **Health Checker** (`src/health/mod.rs`) — The background loop that calls `agent.health_check()` + `agent.list_models()`
3. **Completions Handler** (`src/api/completions.rs`) — Where `agent.chat_completion()` replaces direct HTTP
4. **Intelligent Router** (`src/routing/`) — How backends are selected before the agent is invoked

### Try It Yourself

1. Look at the factory tests to see every backend type created:
   ```bash
   cargo test agent::factory -- --nocapture
   ```

2. Run the integration tests:
   ```bash
   cargo test --test agent_integration -- --nocapture
   ```

3. Read an agent implementation from top to bottom — `ollama.rs` is the most complete:
   ```bash
   cat src/agent/ollama.rs
   ```

4. Search for the agent-first pattern in the codebase:
   ```bash
   grep -n "get_agent" src/health/mod.rs src/api/completions.rs
   ```

5. Check how `ModelCapability` converts to `Model`:
   ```bash
   grep -A 10 "impl From<ModelCapability>" src/agent/types.rs
   ```
