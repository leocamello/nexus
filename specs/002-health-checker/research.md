# Research: Health Checker (F02)

**Date**: 2026-02-03
**Phase**: Implemented (v0.1)

This document captures the technical decisions made during implementation of the Health Checker — the background service that monitors backend health, discovers models, and drives status transitions in the registry.

## Research Questions & Findings

### 1. Backend-Specific Health Endpoints

**Question**: How should we determine the correct health check endpoint for each backend type?

**Decision**: Use a static dispatch function that maps `BackendType` to a known endpoint path.

**Rationale**:
- Each backend type exposes health/model information at different endpoints
- Ollama uses `/api/tags` (returns model list), llama.cpp uses `/health` (returns status JSON), and OpenAI-compatible backends use `/v1/models`
- Static dispatch avoids runtime configuration — the endpoint is derived from the backend type, not user input
- Health checks double as model discovery: the same response that confirms a backend is alive also reports available models

**Alternatives Considered**:
- User-configurable health endpoint per backend: Rejected because it shifts the burden of knowing internal API details to the user. The whole point of `BackendType` is to encapsulate this knowledge.
- Universal `/health` probe: Rejected because not all backends implement `/health`. Ollama has no `/health` endpoint — `/api/tags` is the closest equivalent. Forcing a single endpoint would require backends to be modified.
- Multiple probe endpoints (try `/health`, fall back to `/v1/models`): Rejected because it doubles the network traffic per health check and introduces ambiguity about which response to trust.

**Implementation**:
```rust
pub fn get_health_endpoint(backend_type: BackendType) -> &'static str {
    match backend_type {
        BackendType::Ollama => "/api/tags",
        BackendType::LlamaCpp => "/health",
        BackendType::VLLM
        | BackendType::Exo
        | BackendType::OpenAI
        | BackendType::LMStudio
        | BackendType::Generic => "/v1/models",
    }
}
```

---

### 2. Model Discovery via Health Check Responses

**Question**: How should we extract model information from health check responses?

**Decision**: Type-specific parsers that extract models from the same response used for health checking.

**Rationale**:
- Ollama `/api/tags` returns `{"models": [{"name": "llama3:70b"}]}` — model list is the health signal
- OpenAI-compatible `/v1/models` returns `{"data": [{"id": "gpt-4"}]}` — same pattern
- LlamaCpp `/health` returns `{"status": "ok"}` with no model info — models preserved from previous checks
- Combining health check and model discovery into one request halves the network traffic
- Parse errors on a 200 response are treated as "healthy but models unknown" (`SuccessWithParseError`) — the backend is responding, so it shouldn't be marked unhealthy

**Alternatives Considered**:
- Separate model discovery endpoint: Rejected because it doubles network requests. Health check already hits an endpoint that returns model information for most backends.
- Strict parsing (fail on unexpected JSON): Rejected because backends evolve their APIs. A new field in Ollama's response shouldn't cause a health check failure. Lenient parsing with `SuccessWithParseError` provides resilience.
- Generic model discovery via probing: Rejected because there's no universal model listing protocol. Each backend type needs specific parsing logic.

**Implementation**:
```rust
pub enum HealthCheckResult {
    Success { latency_ms: u32, models: Vec<Model> },
    SuccessWithParseError { latency_ms: u32, parse_error: String },
    Failure { error: HealthCheckError },
}
```

---

### 3. Failure and Recovery Thresholds

**Question**: How many consecutive failures/successes should trigger a status transition?

**Decision**: Configurable thresholds — default 3 failures to mark unhealthy, 2 successes to recover.

**Rationale**:
- A single network blip shouldn't take a backend offline — 3 consecutive failures (at 30s intervals = 90s) provides reasonable debounce
- Recovery is faster (2 successes = 60s) because the cost of a false positive (sending a request to a recovered backend) is low — it either works or fails and retries
- Asymmetric thresholds (3 fail, 2 recover) prevent flapping: a backend that alternates success/failure stays in its current state
- Both values are configurable via `HealthCheckConfig` for operators with different tolerance levels

**Alternatives Considered**:
- Immediate transition (threshold=1): Rejected because transient network issues would cause constant status flapping. A single dropped packet would mark a healthy backend as unhealthy.
- Percentage-based (e.g., "3 of last 5 checks"): Rejected as more complex to implement (requires a sliding window buffer) without meaningful benefit over consecutive counts.
- Exponential backoff on failure: Considered for reducing probe traffic to unhealthy backends. Deferred to a future version — the current 30s fixed interval is simple and sufficient for small deployments.

**Implementation**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    pub enabled: bool,
    pub interval_seconds: u64,       // Default: 30
    pub timeout_seconds: u64,        // Default: 5
    pub failure_threshold: u32,      // Default: 3
    pub recovery_threshold: u32,     // Default: 2
}
```

---

### 4. Per-Backend State Tracking with DashMap

**Question**: How should we track per-backend health check state (consecutive counts, last models)?

**Decision**: Use `DashMap<String, BackendHealthState>` in the `HealthChecker` struct, separate from the registry.

**Rationale**:
- Health check state (consecutive failure count, last known models) is internal to the health checker — it shouldn't pollute the `Backend` struct in the registry
- `DashMap` provides concurrent access since health checks for different backends could overlap in future versions
- `BackendHealthState` tracks `last_models` to preserve model information when a backend returns 200 but invalid JSON, or when LlamaCpp (which doesn't report models) is healthy
- State is lazily initialized via `entry().or_default()` — no need to pre-populate

**Alternatives Considered**:
- Store health state directly in `Backend`: Rejected because it couples registry data with health checker internals. The registry should only contain data meaningful to routing and API responses.
- `HashMap` protected by `RwLock`: Rejected for the same reasons as the registry (see F01 research). DashMap provides better concurrent access.
- Per-backend `Mutex<BackendHealthState>`: Rejected because it requires pre-creating a mutex for each backend and managing lifecycle. DashMap handles this implicitly.

**Implementation**:
```rust
pub struct BackendHealthState {
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub last_check_time: Option<DateTime<Utc>>,
    pub last_status: BackendStatus,
    pub last_models: Vec<Model>,
}

impl BackendHealthState {
    pub fn apply_result(&mut self, result: &HealthCheckResult, config: &HealthCheckConfig)
        -> Option<BackendStatus>
    {
        match result {
            HealthCheckResult::Success { .. } | HealthCheckResult::SuccessWithParseError { .. } => {
                self.consecutive_failures = 0;
                self.consecutive_successes += 1;
                match self.last_status {
                    BackendStatus::Unknown => Some(BackendStatus::Healthy),
                    BackendStatus::Unhealthy
                        if self.consecutive_successes >= config.recovery_threshold =>
                    {
                        Some(BackendStatus::Healthy)
                    }
                    _ => None,
                }
            }
            // ... failure handling
        }
    }
}
```

---

### 5. Graceful Shutdown with CancellationToken

**Question**: How should we stop the health checker background task cleanly?

**Decision**: Use `CancellationToken` from `tokio_util` with `tokio::select!` in the main loop.

**Rationale**:
- The health checker runs as a `tokio::spawn` background task — it needs a cooperative shutdown signal
- `CancellationToken` is cloneable and can be shared with multiple tasks (health checker, mDNS discovery, server)
- `tokio::select!` allows the loop to respond to either the next tick or cancellation, whichever comes first
- `MissedTickBehavior::Skip` ensures that if a health check cycle takes longer than the interval, we skip missed ticks rather than bursting

**Alternatives Considered**:
- `tokio::sync::watch<bool>`: Rejected because it requires polling. `CancellationToken` provides a future-based API that integrates cleanly with `select!`.
- `tokio::sync::oneshot`: Rejected because it's single-use. If we later add multiple shutdown phases (drain, then stop), oneshot can't express this.
- `std::sync::atomic::AtomicBool` checked each loop: Rejected because it doesn't integrate with async. The loop would need to add a sleep between checks, and there's no way to wake it early.
- Dropping the `JoinHandle`: Rejected because dropping a JoinHandle detaches the task — it keeps running. Cancellation requires cooperative signaling.

**Implementation**:
```rust
pub fn start(self, cancel_token: CancellationToken) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(
            Duration::from_secs(self.config.interval_seconds)
        );
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    tracing::info!("Health checker shutting down");
                    break;
                }
                _ = interval.tick() => {
                    self.check_all_backends().await;
                }
            }
        }
    })
}
```

**References**:
- LEARNINGS.md: "Graceful Shutdown Pattern — Using CancellationToken from tokio_util provides clean shutdown"
- https://docs.rs/tokio-util/0.7/tokio_util/sync/struct.CancellationToken.html

---

### 6. Error Classification Strategy

**Question**: How should we categorize different health check failure modes?

**Decision**: Classify errors into typed variants: `Timeout`, `ConnectionFailed`, `HttpError`, `ParseError`, `DnsError`, `TlsError`.

**Rationale**:
- Different failure modes have different operational implications — a timeout suggests the backend is overloaded, while a connection refused means it's down
- Typed errors enable structured logging with `thiserror` `#[error()]` messages
- The registry stores `last_error` as `Option<String>` — the `Display` impl on each variant provides a human-readable error for the `/health` endpoint and CLI output
- Timeout is detected via `reqwest::Error::is_timeout()` — all other errors are classified as `ConnectionFailed` as a safe default

**Alternatives Considered**:
- Simple `String` error: Rejected because it loses structure. Code that needs to distinguish timeout from connection failure would have to parse strings.
- `anyhow::Error`: Rejected because it erases the error type. Health check errors need to be cloneable (stored in `BackendHealthState`) and `anyhow::Error` doesn't implement `Clone`.
- Separate error type per backend type: Rejected because the failure modes are the same across backend types — it's the network and HTTP layer that fails, not the backend-specific parsing.

**Implementation**:
```rust
#[derive(Debug, Clone, Error)]
pub enum HealthCheckError {
    #[error("request timeout after {0}s")]
    Timeout(u64),
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    #[error("HTTP error: {0}")]
    HttpError(u16),
    #[error("invalid response: {0}")]
    ParseError(String),
    // ...
}

fn classify_error(e: reqwest::Error, timeout_seconds: u64) -> HealthCheckError {
    if e.is_timeout() {
        HealthCheckError::Timeout(timeout_seconds)
    } else {
        HealthCheckError::ConnectionFailed(e.to_string())
    }
}
```

---

### 7. Sequential vs Concurrent Backend Checks

**Question**: Should health checks for multiple backends run concurrently or sequentially?

**Decision**: Sequential iteration in `check_all_backends()`.

**Rationale**:
- Simplicity — sequential checks are easier to reason about and debug
- With typical deployments of 2-10 backends and a 5s timeout, worst case is 50s per cycle (well under the 30s interval only if many backends are down)
- Sequential checks avoid thundering herd effects where all backends receive probes simultaneously
- Each check's result is applied to the registry immediately, so routing decisions improve incrementally during a cycle

**Alternatives Considered**:
- `tokio::join!` / `FuturesUnordered`: Considered for deployments with many backends. Deferred because the current sequential approach works well for the v0.1 target of < 20 backends. Concurrent checks would be a straightforward future optimization.
- `tokio::spawn` per backend: Rejected because it makes error handling more complex and the apply_result step would need synchronization.

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Health check timeout blocks subsequent checks | Medium | 5s timeout per check is bounded. Sequential checks with Skip missed ticks ensures the next cycle starts fresh. |
| Model list thrashing on parse errors | Medium | `SuccessWithParseError` preserves `last_models` from state. Models only update on successful parse. |
| Backend flapping between healthy/unhealthy | High | Asymmetric thresholds (3 fail, 2 recover) prevent rapid oscillation. Status transitions are logged at INFO level. |
| Health checker continues after server shutdown | Low | `CancellationToken` ensures cooperative shutdown. `JoinHandle` is awaited in the cleanup phase. |

---

## References

- [reqwest documentation](https://docs.rs/reqwest/0.12/reqwest/)
- [tokio_util CancellationToken](https://docs.rs/tokio-util/0.7/tokio_util/sync/struct.CancellationToken.html)
- [thiserror documentation](https://docs.rs/thiserror/1/thiserror/)
- [Ollama API documentation](https://github.com/ollama/ollama/blob/main/docs/api.md)
- LEARNINGS.md: "Graceful Shutdown Pattern"
