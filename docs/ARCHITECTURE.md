# Nexus - Technical Architecture

## System Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                           NEXUS SERVER                               │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │                        API Layer (Axum)                          │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │ │
│  │  │ /v1/chat/    │  │ /v1/models   │  │ /health              │  │ │
│  │  │ completions  │  │ /v1/stats    │  │ /metrics             │  │ │
│  │  └──────┬───────┘  └──────┬───────┘  └──────────┬───────────┘  │ │
│  │                                                                  │ │
│  │  ┌──────────────────────────────────────────────────────────┐   │ │
│  │  │ / (Dashboard) — embedded HTML/JS/CSS via rust-embed       │   │ │
│  │  │   Real-time updates via WebSocket                         │   │ │
│  │  └──────────────────────────────────────────────────────────┘   │ │
│  └─────────┼─────────────────┼─────────────────────┼───────────────┘ │
│            │                 │                     │                  │
│            ▼                 ▼                     ▼                  │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │                      Router Layer                                │ │
│  │  ┌──────────────────────────────────────────────────────────┐  │ │
│  │  │ Intelligent Router                                        │  │ │
│  │  │ - Capability matching                                     │  │ │
│  │  │ - Load balancing                                          │  │ │
│  │  │ - Failover handling                                       │  │ │
│  │  └──────────────────────────────────────────────────────────┘  │ │
│  └──────────────────────────────┬──────────────────────────────────┘ │
│                                 │                                     │
│                                 ▼                                     │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │                    Backend Registry                              │ │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐                │ │
│  │  │ Backend 1  │  │ Backend 2  │  │ Backend 3  │                │ │
│  │  │ Ollama     │  │ vLLM       │  │ exo        │                │ │
│  │  │ Healthy    │  │ Healthy    │  │ Unhealthy  │                │ │
│  │  └────────────┘  └────────────┘  └────────────┘                │ │
│  └──────────────────────────────────────────────────────────────────┘ │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │                   Background Services                            │ │
│  │  ┌──────────────────┐  ┌──────────────────────────────────┐   │ │
│  │  │ Health Checker   │  │ mDNS Discovery                    │   │ │
│  │  │ (30s interval)   │  │ (continuous)                      │   │ │
│  │  └──────────────────┘  └──────────────────────────────────┘   │ │
│  └──────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
        ┌────────────────────────┼────────────────────────┐
        │                        │                        │
        ▼                        ▼                        ▼
   ┌─────────┐              ┌─────────┐              ┌─────────┐
   │ Ollama  │              │  vLLM   │              │   exo   │
   │ :11434  │              │  :8000  │              │ :52415  │
   └─────────┘              └─────────┘              └─────────┘
```

---

## Component Details

### 1. API Layer

The HTTP interface exposed to clients.

```rust
// src/api/mod.rs
pub mod chat;
pub mod models;
pub mod health;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(chat::completions))
        .route("/v1/models", get(models::list))
        .route("/health", get(health::check))
        .route("/v1/stats", get(stats::handle))
        .route("/metrics", get(metrics::handle))
        .route("/", get(dashboard::handler))
        .with_state(state)
}
```

#### Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/v1/chat/completions` | POST | Chat completion (streaming or not) |
| `/v1/models` | GET | List all available models (per-backend entries with `owned_by` = backend name) |
| `/v1/stats` | GET | JSON stats: uptime, request counts, per-backend metrics |
| `/metrics` | GET | Prometheus metrics (counters, histograms, gauges) |
| `/health` | GET | System and backend health |
| `/` | GET | Embedded web dashboard with real-time WebSocket updates |

#### Request Flow

```
1. Request arrives at /v1/chat/completions
2. Parse and validate ChatCompletionRequest
3. Extract routing requirements:
   - model name
   - estimated context length
   - requires vision? (check for image_url)
   - requires tools? (check for tools array)
4. Call Router.select_backend(requirements)
5. Forward request to selected backend
6. Stream response back to client
7. On error: retry with next backend (if configured)
```

### 2. Router Layer

Intelligent request routing logic.

```rust
// src/routing/mod.rs

pub struct Router {
    registry: Arc<Registry>,
    config: RoutingConfig,
}

impl Router {
    pub fn select_backend(&self, req: &RoutingRequest) -> Result<Backend, RoutingError> {
        // 1. Get candidates matching model
        let candidates = self.registry.get_backends_for_model(&req.model)?;
        
        // 2. Filter by health
        let healthy: Vec<_> = candidates
            .into_iter()
            .filter(|b| b.status == BackendStatus::Healthy)
            .collect();
        
        // 3. Filter by capabilities
        let capable: Vec<_> = healthy
            .into_iter()
            .filter(|b| self.meets_requirements(b, req))
            .collect();
        
        // 4. Score and select
        capable
            .into_iter()
            .max_by_key(|b| self.score(b, req))
            .ok_or(RoutingError::NoBackendAvailable)
    }
    
    fn score(&self, backend: &Backend, req: &RoutingRequest) -> i32 {
        let priority_score = 100 - backend.priority;
        let load_score = 100 - backend.pending_requests.min(100) as i32;
        let latency_score = 100 - (backend.avg_latency_ms / 10).min(100) as i32;
        
        (priority_score * self.config.weights.priority as i32 +
         load_score * self.config.weights.load as i32 +
         latency_score * self.config.weights.latency as i32) / 100
    }
}
```

#### Routing Strategies

| Strategy | Description | Use Case |
|----------|-------------|----------|
| `smart` | Score by priority + load + latency | Default, recommended |
| `round_robin` | Rotate through healthy backends | Even distribution |
| `priority_only` | Always use lowest priority number | Dedicated primary |
| `random` | Random selection from healthy | Testing |

### 3. Backend Registry

In-memory storage for backend and model information.

```rust
// src/registry/mod.rs

pub struct Registry {
    backends: Arc<RwLock<HashMap<String, Backend>>>,
    models_index: Arc<RwLock<HashMap<String, Vec<String>>>>, // model -> backend_ids
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Backend {
    pub id: String,
    pub name: String,
    pub url: String,
    pub backend_type: BackendType,
    pub status: BackendStatus,
    pub last_health_check: DateTime<Utc>,
    pub models: Vec<Model>,
    pub priority: i32,
    pub pending_requests: AtomicU32,
    pub avg_latency_ms: AtomicU32,
    pub discovery_source: DiscoverySource,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub context_length: u32,
    pub supports_vision: bool,
    pub supports_tools: bool,
    pub supports_json_mode: bool,
    pub backend_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BackendStatus {
    Healthy,
    Unhealthy,
    Unknown,
}

#[derive(Clone, Debug)]
pub enum BackendType {
    Ollama,
    VLLM,
    LlamaCpp,
    Exo,
    OpenAI,
    Generic,
}

#[derive(Clone, Debug)]
pub enum DiscoverySource {
    Static,      // From config file
    MDNS,        // Auto-discovered
    Manual,      // Added via CLI
}
```

### 4. Health Checker

Background service for monitoring backend health.

```rust
// src/health/mod.rs

pub struct HealthChecker {
    registry: Arc<Registry>,
    config: HealthCheckConfig,
    client: reqwest::Client,
}

impl HealthChecker {
    pub async fn run(&self, mut shutdown: broadcast::Receiver<()>) {
        let mut interval = tokio::time::interval(
            Duration::from_secs(self.config.interval_seconds)
        );
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.check_all_backends().await;
                }
                _ = shutdown.recv() => {
                    tracing::info!("Health checker shutting down");
                    break;
                }
            }
        }
    }
    
    async fn check_backend(&self, backend: &Backend) -> HealthCheckResult {
        let url = match backend.backend_type {
            BackendType::Ollama => format!("{}/api/tags", backend.url),
            _ => format!("{}/v1/models", backend.url),
        };
        
        let start = Instant::now();
        let result = self.client
            .get(&url)
            .timeout(Duration::from_secs(self.config.timeout_seconds))
            .send()
            .await;
        let latency = start.elapsed();
        
        match result {
            Ok(resp) if resp.status().is_success() => {
                let models = self.parse_models(backend.backend_type, resp).await?;
                HealthCheckResult::Healthy { models, latency }
            }
            Ok(resp) => HealthCheckResult::Unhealthy {
                reason: format!("HTTP {}", resp.status()),
            },
            Err(e) => HealthCheckResult::Unhealthy {
                reason: e.to_string(),
            },
        }
    }
}
```

### 5. mDNS Discovery

Automatic backend discovery on local network.

```rust
// src/discovery/mod.rs

pub struct MdnsDiscovery {
    registry: Arc<Registry>,
    config: DiscoveryConfig,
}

impl MdnsDiscovery {
    pub async fn run(&self, mut shutdown: broadcast::Receiver<()>) {
        let daemon = ServiceDaemon::new().expect("Failed to create mDNS daemon");
        
        for service_type in &self.config.service_types {
            let receiver = daemon.browse(service_type).expect("Failed to browse");
            
            tokio::spawn({
                let registry = self.registry.clone();
                async move {
                    while let Ok(event) = receiver.recv_async().await {
                        match event {
                            ServiceEvent::ServiceResolved(info) => {
                                Self::handle_service_found(&registry, info).await;
                            }
                            ServiceEvent::ServiceRemoved(_, name) => {
                                Self::handle_service_removed(&registry, &name).await;
                            }
                            _ => {}
                        }
                    }
                }
            });
        }
        
        shutdown.recv().await.ok();
        tracing::info!("mDNS discovery shutting down");
    }
    
    async fn handle_service_found(registry: &Registry, info: ServiceInfo) {
        let backend = Backend {
            id: Uuid::new_v4().to_string(),
            name: info.get_fullname().to_string(),
            url: format!("http://{}:{}", info.get_addresses().first().unwrap(), info.get_port()),
            backend_type: Self::detect_type(&info),
            status: BackendStatus::Unknown,
            discovery_source: DiscoverySource::MDNS,
            // ...
        };
        
        registry.add_backend(backend).await;
    }
}
```

### 6. Metrics Layer

Exposes operational statistics via two formats.

**Prometheus** (`GET /metrics`): Counters, histograms, and gauges for scraping by Prometheus/Grafana.

**JSON Stats** (`GET /v1/stats`): Human-friendly JSON breakdown:
- Aggregate request counts (total, success, errors)
- Per-backend stats (id, name, requests, average latency, pending)
- Per-model stats (name, requests, average duration)
- Uptime in seconds

### 7. Web Dashboard

Embedded single-page dashboard served at `GET /`.

- **Technology**: Vanilla JS + CSS, embedded via `rust-embed` (no build step)
- **Real-time updates**: WebSocket connection pushes backend status changes, request completions
- **Sections**: Backend status cards, model availability matrix, request history (last 100)
- **Backend cards**: Show name, UUID, type, URL, status badge, and metrics (requests, latency, pending, model count)
- **Request history**: Displays backend name with UUID tooltip for traceability
- **No-JS fallback**: Initial server-rendered data injected via `<script id="initial-data">` tag

### 8. CLI Layer

Command-line interface built with **Clap derive** (`src/cli/`).

| File | Purpose |
|------|---------|
| `mod.rs` | Root `Cli` struct, `Commands` enum (6 subcommands) |
| `serve.rs` | Starts HTTP server, health checker, mDNS, budget reconciliation |
| `backends.rs` | `backends list/add/remove` with auto-type-detection |
| `models.rs` | `models` listing aggregated across backends |
| `health.rs` | `health` status with colored terminal output |
| `config.rs` | `config init` generates example TOML (respects `--force`) |
| `completions.rs` | Shell completion generation (Bash/Zsh/Fish) |
| `output.rs` | Table and JSON formatting via `comfy_table` |

**Startup orchestration** (`serve.rs`): Loads config → validates → inits tracing → creates
registry → registers static backends with agents → starts background tasks (health checker,
mDNS, budget reconciliation) → binds TCP → runs Axum server → handles graceful shutdown via
`CancellationToken`.

### 9. Configuration Layer

TOML-based configuration with environment variable overrides (`src/config/`).

| File | Purpose |
|------|---------|
| `mod.rs` | `NexusConfig` aggregate struct, `load()`, `with_env_overrides()` |
| `server.rs` | `ServerConfig`: host, port, timeout, max concurrent requests |
| `backend.rs` | `BackendConfig`: name, url, type, priority, api_key_env, zone, tier |
| `routing.rs` | `RoutingConfig`: strategy, policies, aliases, budget |
| `discovery.rs` | `DiscoveryConfig`: enabled, service types, grace period |
| `logging.rs` | `LoggingConfig`: level, format, component levels |
| `error.rs` | Configuration validation errors |

**Precedence**: CLI args > env vars (`NEXUS_*`) > config file > defaults.

Supported environment overrides: `NEXUS_PORT`, `NEXUS_HOST`, `NEXUS_LOG_LEVEL`,
`NEXUS_LOG_FORMAT`, `NEXUS_DISCOVERY`, `NEXUS_HEALTH_CHECK`. Invalid values are
silently ignored (no panic).

### 10. Logging Layer

Structured logging via `tracing` (`src/logging/`).

| File | Purpose |
|------|---------|
| `mod.rs` | `build_filter_directives()` — constructs `EnvFilter` from config |
| `middleware.rs` | `generate_request_id()` — UUID v4 correlation IDs |
| `fields.rs` | `extract_tokens()`, `extract_status()`, `truncate_prompt()` |

**Setup** (in `serve.rs`): `tracing_subscriber::registry()` with `EnvFilter` +
`fmt::layer()` (Pretty or JSON). Component-specific levels supported
(e.g., `nexus::routing=debug`).

**Per-request fields** captured via `#[instrument]`: `request_id`, `model`,
`backend`, `backend_type`, `status`, `status_code`, `latency_ms`, `tokens_prompt`,
`tokens_completion`, `tokens_total`, `stream`, `route_reason`, `retry_count`.

---

## Data Flow Diagrams

### Chat Completion Request (Non-Streaming)

```
Client                  Nexus                   Backend
  │                       │                       │
  │ POST /v1/chat/...     │                       │
  │──────────────────────>│                       │
  │                       │                       │
  │                       │ select_backend()      │
  │                       │─────────┐             │
  │                       │         │             │
  │                       │<────────┘             │
  │                       │                       │
  │                       │ POST /v1/chat/...     │
  │                       │──────────────────────>│
  │                       │                       │
  │                       │ ChatCompletionResponse│
  │                       │<──────────────────────│
  │                       │                       │
  │ ChatCompletionResponse│                       │
  │<──────────────────────│                       │
  │                       │                       │
```

### Chat Completion Request (Streaming)

```
Client                  Nexus                   Backend
  │                       │                       │
  │ POST /v1/chat/...     │                       │
  │ stream=true           │                       │
  │──────────────────────>│                       │
  │                       │                       │
  │                       │ POST /v1/chat/...     │
  │                       │ stream=true           │
  │                       │──────────────────────>│
  │                       │                       │
  │ SSE: data: {...}      │ SSE: data: {...}      │
  │<──────────────────────│<──────────────────────│
  │                       │                       │
  │ SSE: data: {...}      │ SSE: data: {...}      │
  │<──────────────────────│<──────────────────────│
  │                       │                       │
  │ SSE: data: [DONE]     │ SSE: data: [DONE]     │
  │<──────────────────────│<──────────────────────│
  │                       │                       │
```

### Backend Failover

```
Client                  Nexus                 Backend A    Backend B
  │                       │                       │            │
  │ POST /v1/chat/...     │                       │            │
  │──────────────────────>│                       │            │
  │                       │                       │            │
  │                       │ POST (to A)           │            │
  │                       │──────────────────────>│            │
  │                       │                       │            │
  │                       │ Error/Timeout         │            │
  │                       │<──────────────────────│            │
  │                       │                       │            │
  │                       │ mark A unhealthy      │            │
  │                       │                       │            │
  │                       │ POST (to B)                        │
  │                       │───────────────────────────────────>│
  │                       │                                    │
  │                       │ ChatCompletionResponse             │
  │                       │<───────────────────────────────────│
  │                       │                       │            │
  │ ChatCompletionResponse│                       │            │
  │<──────────────────────│                       │            │
```

---

## Configuration Schema

```toml
# nexus.toml

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
strategy = "smart"  # smart | round_robin | priority_only | random
max_retries = 2

[routing.weights]
priority = 50
load = 30
latency = 20

[routing.aliases]
"gpt-4" = "llama3:70b"
"gpt-4-turbo" = "llama3:70b"
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

[[backends]]
name = "cloud-fallback"
url = "https://api.openai.com"
type = "openai"
priority = 100
api_key_env = "OPENAI_API_KEY"

[logging]
level = "info"
format = "pretty"  # pretty | json
```

---

## Error Handling

### Error Types

```rust
// src/error.rs

#[derive(Debug, thiserror::Error)]
pub enum NexusError {
    #[error("No backend available for model '{model}'")]
    NoBackendAvailable { model: String },
    
    #[error("All backends failed: {reasons:?}")]
    AllBackendsFailed { reasons: Vec<String> },
    
    #[error("Backend timeout after {timeout_seconds}s")]
    BackendTimeout { timeout_seconds: u64 },
    
    #[error("Backend error: {message}")]
    BackendError { status: u16, message: String },
    
    #[error("Invalid request: {message}")]
    InvalidRequest { message: String },
    
    #[error("Model not found: {model}")]
    ModelNotFound { model: String },
    
    #[error("Configuration error: {message}")]
    ConfigError { message: String },
}
```

### HTTP Error Responses

| Error | HTTP Status | Response |
|-------|-------------|----------|
| NoBackendAvailable | 503 | `{"error": {"type": "server_error", "message": "..."}}` |
| BackendTimeout | 504 | `{"error": {"type": "timeout", "message": "..."}}` |
| BackendError | 502 | `{"error": {"type": "backend_error", "message": "..."}}` |
| InvalidRequest | 400 | `{"error": {"type": "invalid_request", "message": "..."}}` |
| ModelNotFound | 404 | `{"error": {"type": "not_found", "message": "..."}}` |

---

## Performance Considerations

### Concurrency Model

```rust
// Application state shared across handlers
pub struct AppState {
    pub registry: Arc<Registry>,
    pub router: Arc<Router>,
    pub http_client: reqwest::Client,
    pub config: Arc<Config>,
}

// HTTP client with connection pooling
let http_client = reqwest::Client::builder()
    .pool_max_idle_per_host(10)
    .timeout(Duration::from_secs(300))
    .build()?;
```

### Memory Usage

| Component | Estimated Size |
|-----------|----------------|
| Registry (100 backends) | ~500 KB |
| HTTP client pool | ~10 MB |
| Request buffers | ~1 KB per request |
| Total baseline | ~15 MB |

### Latency Budget

| Operation | Target | Max |
|-----------|--------|-----|
| Request parsing | 0.1ms | 1ms |
| Backend selection | 0.5ms | 2ms |
| Proxy overhead | 1ms | 5ms |
| **Total overhead** | **< 2ms** | **< 10ms** |

---

## Testing Strategy

### Unit Tests
```
src/
├── registry/
│   └── tests.rs      # Registry operations
├── routing/
│   └── tests.rs      # Routing logic, scoring
├── health/
│   └── tests.rs      # Health check parsing
└── api/
    └── tests.rs      # Request/response handling
```

### Integration Tests
```
tests/
├── api_test.rs           # Full API tests with mock backend
├── discovery_test.rs     # mDNS tests (optional, requires network)
├── failover_test.rs      # Backend failure scenarios
└── load_test.rs          # Concurrent request handling
```

### Test Backends

```rust
// tests/mock_backend.rs

pub struct MockBackend {
    port: u16,
    models: Vec<String>,
    response_delay: Duration,
    fail_rate: f32,
}

impl MockBackend {
    pub async fn start(&self) -> JoinHandle<()> {
        // Start Axum server with controllable behavior
    }
}
```

---

## Deployment

### Single Binary

```bash
# Build release
cargo build --release

# Binary size target: < 20 MB
ls -lh target/release/nexus

# Run
./nexus serve --config nexus.toml
```

### Docker

```dockerfile
FROM rust:latest AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/nexus /usr/local/bin/
EXPOSE 8000
CMD ["nexus", "serve"]
```

### Systemd

```ini
[Unit]
Description=Nexus LLM Orchestrator
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/nexus serve --config /etc/nexus/nexus.toml
Restart=always

[Install]
WantedBy=multi-user.target
```

---

## Security Considerations

### v0.1 (Current)
- No authentication (trusted network assumed)
- API key passthrough to backends
- No TLS termination (use reverse proxy)

### v0.3+ (Planned)
- Privacy zones: structural enforcement of data boundaries
- Cloud API keys loaded from environment variables, never in config files
- Nexus-Transparent Protocol headers reveal routing decisions (not sensitive data)

### v0.5+ (Planned)
- Optional API key authentication (multi-tenant)
- Per-tenant rate limiting
- Request/response logging (opt-in, no message content by default)
- TLS support

---

## Future Architecture (v0.3-v0.5)

> **Reference:** RFC-001 v2 ("Nexus Platform Architecture — From Monolithic Router to Controller/Agent Platform") defines the full architectural evolution. This section summarizes the key elements.

The v0.3+ architecture adopts the **Kubernetes Controller pattern** — decoupling the Control Plane (policy decisions) from the Data Plane (backend communication) using a standardized interface — while preserving the "Zero Config" and "Single Binary" constraints.

### Nexus Inference Interface (NII) — v0.3

Every backend implements the `InferenceAgent` Rust trait, eliminating type-specific branching:

```rust
#[async_trait]
pub trait InferenceAgent: Send + Sync + 'static {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn profile(&self) -> AgentProfile;

    async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError>;
    async fn health_check(&self) -> Result<HealthStatus, AgentError>;
    async fn chat_completion(&self, request: &ChatCompletionRequest)
        -> Result<ChatCompletionResponse, AgentError>;
    async fn chat_completion_stream(&self, request: &ChatCompletionRequest)
        -> Result<BoxStream<'_, Result<StreamChunk, AgentError>>, AgentError>;

    // Default implementations for optional capabilities
    async fn embeddings(&self, _request: &EmbeddingsRequest) -> Result<...> { Err(Unsupported) }
    async fn load_model(&self, _model_id: &str) -> Result<(), AgentError> { Err(Unsupported) }
    async fn count_tokens(&self, _model_id: &str, text: &str) -> TokenCount {
        TokenCount::Heuristic((text.len() / 4) as u32) // Binary-size-safe default
    }
}
```

**Built-in agents:** OllamaAgent, OpenAIAgent, LMStudioAgent, VLLMAgent, GenericOpenAIAgent.

### Control Plane — Reconciler Pipeline (v0.3)

Replaces the imperative `Router::select_backend()` with independent reconcilers that annotate a shared `RoutingIntent`. Each reconciler only **adds** constraints — never removes them — ensuring order-independence and composability.

#### Pipeline Architecture

The pipeline follows a trait-based design where each reconciler implements `Reconciler`:

```rust
pub trait Reconciler: Send + Sync {
    fn name(&self) -> &'static str;
    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError>;
}
```

`RoutingIntent` is the shared state object carrying request identity, model resolution, requirements, policy constraints, budget state, and the candidate/excluded agent lists. Reconcilers read from any field but only write to their own domain fields and the exclusion list.

`RoutingDecision` is the pipeline output — one of three variants:
- **Route**: agent selected, model resolved, cost estimated
- **Queue**: all agents busy, estimated wait time (future)
- **Reject**: no viable agents, with accumulated rejection reasons

#### Execution Order

```
Request
    │
    ▼
┌──────────────────────────────┐
│  1. RequestAnalyzer          │  Resolve aliases (max 3 levels), populate candidates
│     Budget: < 0.5ms         │  from registry by model availability
└──────────┬───────────────────┘
           ▼
┌──────────────────────────────┐
│  2. PrivacyReconciler        │  Match model against traffic policies,
│                              │  exclude agents violating privacy zone
└──────────┬───────────────────┘
           ▼
┌──────────────────────────────┐
│  3. BudgetReconciler         │  Estimate cost, calculate budget status,
│                              │  exclude cloud agents at hard limit
└──────────┬───────────────────┘
           ▼
┌──────────────────────────────┐
│  4. TierReconciler           │  Enforce minimum capability tier from
│                              │  policy (strict or flexible mode)
└──────────┬───────────────────┘
           ▼
┌──────────────────────────────┐
│  5. QualityReconciler        │  Reserved for future quality metrics
│     (pass-through Phase 1)   │  (latency, accuracy, user ratings)
└──────────┬───────────────────┘
           ▼
┌──────────────────────────────┐
│  6. SchedulerReconciler      │  Filter by health + capabilities,
│                              │  score candidates → select best agent
└──────────────────────────────┘
    │
    ▼
RoutingDecision (Route | Queue | Reject)
```

**Order matters because:**
1. RequestAnalyzer must run first to resolve aliases and populate candidates
2. Privacy and Budget filter policy-violating agents before scoring
3. TierReconciler runs after Budget to avoid tier-filtering already-budget-excluded agents
4. SchedulerReconciler must run last — it selects the single best agent from remaining candidates

**Order-independence of constraint reconcilers:** Privacy, Budget, and Tier each only **add** constraints and exclusions. They never remove rejection reasons or re-add excluded agents. This means reordering the middle three (Privacy, Budget, Tier) would produce the same final candidate set.

#### Data Flow

```
RoutingIntent fields written by each reconciler:

RequestAnalyzer:    resolved_model, candidate_agents
PrivacyReconciler:  privacy_constraint, excluded_agents, rejection_reasons
BudgetReconciler:   cost_estimate, budget_status, excluded_agents, rejection_reasons
TierReconciler:     min_capability_tier, excluded_agents, rejection_reasons
QualityReconciler:  (none — pass-through)
SchedulerReconciler: candidate_agents (→ single winner), route_reason
```

#### Observability

The pipeline emits metrics and trace logs at each stage:

- `nexus_reconciler_duration_seconds{reconciler}` — Per-reconciler latency histogram
- `nexus_reconciler_exclusions_total{reconciler}` — Agents excluded per reconciler
- `nexus_pipeline_duration_seconds` — Total pipeline execution latency
- `trace!`-level logs for each reconciler with timing and candidate counts

#### Adding a New Reconciler

1. Create `src/routing/reconciler/your_reconciler.rs`
2. Implement the `Reconciler` trait (add constraints, never remove)
3. Add `pub mod your_reconciler;` to `src/routing/reconciler/mod.rs`
4. Insert into the pipeline construction in `Router::select_backend()` (in `src/routing/mod.rs`)
5. Add metrics and tracing (automatic via pipeline executor)

```rust
pub struct YourReconciler { /* config */ }

impl Reconciler for YourReconciler {
    fn name(&self) -> &'static str { "YourReconciler" }

    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        // Read intent.requirements, intent.resolved_model, etc.
        // Add constraints or exclude agents via intent.exclude_agent()
        // NEVER remove from intent.rejection_reasons or intent.excluded_agents
        Ok(())
    }
}
```

### Architecture Topology (v0.3+)

```
┌─────────────────────────────────────────────────────────────────────┐
│                      NEXUS BINARY (single process)                  │
│                                                                     │
│  ┌──────────────── CONTROL PLANE ─────────────────────────────────┐ │
│  │                                                                │ │
│  │  RequestAnalyzer → Reconciler Pipeline → RoutingDecision       │ │
│  │                                                                │ │
│  │  Agent Registry (DashMap)                                      │ │
│  │    AgentSchedulingProfiles + ModelIndex + Quality Metrics       │ │
│  │                                                                │ │
│  │  Background Loops: Health | Budget | Quality | mDNS Discovery  │ │
│  │                                                                │ │
│  │  Request Queue (F18): Bounded queue with timeout + re-route    │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌──────────────── DATA PLANE ────────────────────────────────────┐ │
│  │                                                                │ │
│  │  OllamaAgent │ OpenAIAgent │ LMStudioAgent │ GenericAgent │... │ │
│  │  (NII impl)  │ (NII impl)  │ (NII impl)    │ (NII impl)      │ │
│  └──────┬──────────────┬──────────────┬──────────────┬───────────┘ │
└─────────┼──────────────┼──────────────┼──────────────┼─────────────┘
          ▼              ▼              ▼              ▼
      Ollama:11434   api.openai.com  LM Studio    vLLM/exo/llama.cpp
```

### Nexus-Transparent Protocol (v0.3)

Every proxied response includes `X-Nexus-*` HTTP headers for routing observability. Headers are additive — they never modify the OpenAI-compatible JSON response body.

```
HTTP/1.1 200 OK
X-Nexus-Backend: gpu-node-1
X-Nexus-Backend-Type: local
X-Nexus-Route-Reason: capability-match
X-Nexus-Privacy-Zone: restricted
Content-Type: application/json

{"id":"chatcmpl-...","choices":[...]}
```

Error responses extend the OpenAI error envelope with an optional `context` object:

```json
{
  "error": {
    "type": "nexus_routing_error",
    "code": "privacy_violation_on_failover",
    "message": "Local backend 'gpu-node-1' is offline. Cannot failover to cloud.",
    "context": {
      "required_tier": 2,
      "available_backends": ["gpu-node-2 (busy)", "gpu-node-3 (loading)"],
      "eta_seconds": 15
    }
  }
}
```

### Privacy Zone Enforcement (v0.3)

```
┌──────────────────────────────────────────────────────────┐
│                        Router                             │
│                                                           │
│  Request arrives → Check backend zone compatibility       │
│                                                           │
│  ┌─────────────────┐        ┌─────────────────┐         │
│  │ Restricted Zone  │        │   Open Zone      │         │
│  │ (local-only)     │───X───→│ (cloud-ok)       │         │
│  │                  │ Never  │                  │         │
│  │  gpu-node-1      │forwards│  openai-gpt4     │         │
│  │  gpu-node-2      │context │  anthropic-claude │         │
│  └─────────────────┘        └─────────────────┘         │
│                                                           │
│  If restricted backend fails → 503 + Retry-After          │
│  If open backend overflows  → fresh context or block      │
└──────────────────────────────────────────────────────────┘
```

### Tokenizer Registry (v0.3)

Per-backend tokenizer for audit-grade token counting and budget management:

```
┌─────────────────────────────────────────────────────────┐
│                  Tokenizer Registry                      │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ OpenAI       │  │ Anthropic    │  │ Llama        │  │
│  │ o200k_base   │  │ cl100k_base  │  │ SentencePiece│  │
│  │ tiktoken-rs  │  │ tiktoken-rs  │  │ tokenizers   │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
│                                                          │
│  Unknown models → 1.15x conservative multiplier          │
│                   (flagged "estimated" in metrics)        │
└─────────────────────────────────────────────────────────┘
```

### Speculative Router (v0.4)

Zero-ML request-content routing via JSON payload inspection:

```
Request JSON ──→ Extract signals (sub-ms)
                  │
                  ├── messages[].content[].type == "image_url" → vision required
                  ├── tools[] present → tool-use required
                  ├── response_format.type == "json_object" → JSON mode required
                  ├── token count estimate → context window requirement
                  └── stream: true → prefer efficient streaming backends
                  │
                  ▼
             Capability filter → Tier filter → Load balance → Route
```

### Fleet Intelligence (v0.5)

Suggestion-based model pre-warming with VRAM awareness:

```
Request History ──→ Demand Prediction ──→ Recommendation
                                              │
Backend VRAM ─────→ Headroom Check ──────────→│
                                              │
                                              ▼
                                    "Load CodeLlama on node-3"
                                    "(4GB VRAM free, >30% headroom)"
                                              │
                                              ▼
                                    Admin/Policy Approval
```

**Design constraints:**
- Never evict a hot model for a prediction
- Only use idle capacity (configurable headroom %)
- Suggestion system, not autonomous actor

---

## TokenizerRegistry (F14: Budget Management)

**Purpose**: Provider-specific token counting for accurate cost estimation

The TokenizerRegistry provides a fallback hierarchy for token counting across different model providers:

```
TokenizerRegistry
│
├── Tier 0: Exact (provider's official tokenizer)
│   ├── gpt-4-turbo*, gpt-4o* → o200k_base (tiktoken-rs)
│   └── gpt-{3.5,4}* → cl100k_base (tiktoken-rs)
│
├── Tier 1: Approximation (similar tokenizer)
│   └── claude-* → cl100k_base (close enough for cost estimates)
│
└── Tier 2: Heuristic (conservative estimate)
    └── unknown models → character_count / 4 * 1.15

```

### Usage

```rust
use nexus::agent::tokenizer::TokenizerRegistry;

let registry = TokenizerRegistry::new()?;

// Automatic tier selection
let token_count = registry.count_tokens("gpt-4-turbo", "Hello, world!")?;

// Check which tier was used
let tokenizer = registry.get_tokenizer("gpt-4-turbo");
match tokenizer.tier() {
    0 => println!("Using exact tokenizer"),
    1 => println!("Using approximation"),
    2 => println!("Using heuristic"),
    _ => unreachable!(),
}
```

### Metrics

Token counting operations emit Prometheus metrics:

- `nexus_token_count_duration_seconds{tier,model}` - Tokenization latency
- `nexus_token_count_tier_total{tier,model}` - Count by tier (exact/approximation/heuristic)
- `nexus_cost_per_request_usd{model,tier}` - Estimated cost per request

### Error Handling

TokenizerRegistry gracefully degrades:

1. **Initialization failure**: Returns `Err(TokenizerError)` at startup (fail-fast)
2. **Unknown model**: Returns `HeuristicTokenizer` (tier 2, never fails)
3. **Tokenization error**: Propagates `Result<u32, TokenizerError>` for caller handling

**Pattern**: All tokenization is non-blocking and fails open (estimates $0.00 if pricing data missing).

### Extension

To add a new provider:

```rust
// 1. Add tokenizer patterns in TokenizerRegistry::new()
let custom_patterns = vec!["provider-model-*"];
for pattern in custom_patterns {
    let glob = Glob::new(pattern)?;
    matchers.push((
        glob.compile_matcher(),
        Arc::new(CustomTokenizer::new()?),
    ));
}

// 2. Implement the Tokenizer trait
impl Tokenizer for CustomTokenizer {
    fn count_tokens(&self, text: &str) -> Result<u32, TokenizerError> {
        // Your tokenization logic
    }
    
    fn tier(&self) -> u8 { 0 } // 0=exact, 1=approx, 2=heuristic
    fn name(&self) -> &str { "custom" }
}
```

**Recommendation**: Use tier 1 (approximation) for new providers with similar tokenization to existing ones (e.g., GPT-like models can use cl100k_base).
