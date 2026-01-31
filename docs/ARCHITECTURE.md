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
│  │  │ completions  │  │              │  │                      │  │ │
│  │  └──────┬───────┘  └──────┬───────┘  └──────────┬───────────┘  │ │
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
        .with_state(state)
}
```

#### Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/v1/chat/completions` | POST | Chat completion (streaming or not) |
| `/v1/models` | GET | List all available models |
| `/health` | GET | System and backend health |

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
FROM rust:1.75 AS builder
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

### v1.0 (Basic)
- No authentication (trusted network assumed)
- API key passthrough to backends
- No TLS termination (use reverse proxy)

### Future
- Optional API key authentication
- Rate limiting
- Request/response logging (opt-in)
- TLS support
