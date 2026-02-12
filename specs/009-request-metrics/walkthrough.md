# Request Metrics - Code Walkthrough

**Feature**: F09 - Request Metrics  
**Audience**: Junior developers joining the project  
**Last Updated**: 2026-02-12

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: mod.rs - The MetricsCollector](#file-1-modrs---the-metricscollector)
4. [File 2: types.rs - JSON Response Shapes](#file-2-typesrs---json-response-shapes)
5. [File 3: handler.rs - HTTP Endpoints](#file-3-handlerrs---http-endpoints)
6. [File 4: completions.rs - Where Metrics Get Recorded](#file-4-completionsrs---where-metrics-get-recorded)
7. [File 5: health/mod.rs - Backend Latency Tracking](#file-5-healthmodrs---backend-latency-tracking)
8. [File 6: api/mod.rs - Wiring It All Together](#file-6-apimodrs---wiring-it-all-together)
9. [Understanding the Tests](#understanding-the-tests)
10. [Key Rust Concepts](#key-rust-concepts)
11. [Common Patterns in This Codebase](#common-patterns-in-this-codebase)
12. [Next Steps](#next-steps)

---

## The Big Picture

Think of Request Metrics as the **dashboard of a car**. Without it, the engine still runs—but you have no idea how fast you're going, how much fuel you have left, or if something is about to break. Metrics give Nexus that dashboard.

### What Problem Does This Solve?

When Nexus is routing requests to multiple backends, operators need answers to questions like:

- "How many requests is each backend handling?"
- "Which models are slowest?"
- "Are there error spikes happening right now?"
- "How many backends are healthy?"

Without metrics, the only way to answer these is to dig through logs. Metrics give us **numbers we can graph, alert on, and query in real-time**.

### Two Endpoints, Two Audiences

This feature exposes metrics through two separate endpoints because different tools consume them differently:

```
┌────────────────────────────────────────────────────────────────────────┐
│                     Who Reads What                                     │
│                                                                        │
│  GET /metrics                          GET /v1/stats                   │
│  ═══════════                           ══════════════                   │
│                                                                        │
│  Audience: Prometheus, Grafana         Audience: Humans, Dashboards    │
│  Format:   Plain text                  Format:   JSON                  │
│                                                                        │
│  Example output:                       Example output:                 │
│  ┌────────────────────────────┐        ┌────────────────────────────┐  │
│  │ # HELP nexus_requests_total│        │ {                          │  │
│  │ # TYPE nexus_requests_total│        │   "uptime_seconds": 3600,  │  │
│  │ nexus_requests_total{      │        │   "requests": {            │  │
│  │   model="llama3",          │        │     "total": 1535          │  │
│  │   backend="gpu1",          │        │   },                       │  │
│  │   status="200"             │        │   "backends": [...]        │  │
│  │ } 1523                     │        │ }                          │  │
│  └────────────────────────────┘        └────────────────────────────┘  │
│                                                                        │
│  Perfect for:                          Perfect for:                    │
│  • Time-series databases               • Quick debugging (curl)        │
│  • Alerting rules                       • Web dashboards (F10)         │
│  • Historical analysis                  • Health checks                │
└────────────────────────────────────────────────────────────────────────┘
```

### How Metrics Flow Through Nexus

Here's what happens when a request arrives and how metrics get recorded at each stage:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                       Request Lifecycle + Metrics                       │
│                                                                         │
│  Client                                                                 │
│    │                                                                    │
│    │  POST /v1/chat/completions                                         │
│    ▼                                                                    │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │  completions.rs :: handle()                                        │ │
│  │                                                                    │ │
│  │  ① start_time = Instant::now()     ← Start the stopwatch          │ │
│  │  │                                                                 │ │
│  │  ② Router selects backend                                         │ │
│  │  │  └─ On error:                                                   │ │
│  │  │     📊 nexus_errors_total{type="model_not_found"}++             │ │
│  │  │                                                                 │ │
│  │  ③ proxy_request() sends to backend                               │ │
│  │  │                                                                 │ │
│  │  ├─ On SUCCESS:                                                    │ │
│  │  │   📊 nexus_requests_total{model, backend, status="200"}++       │ │
│  │  │   📊 nexus_request_duration_seconds{model, backend} ← elapsed  │ │
│  │  │   📊 nexus_fallbacks_total{from, to}++ (if fallback was used)   │ │
│  │  │   📊 nexus_tokens_total{type="prompt"} ← token count           │ │
│  │  │   📊 nexus_tokens_total{type="completion"} ← token count       │ │
│  │  │                                                                 │ │
│  │  └─ On ERROR:                                                      │ │
│  │      📊 nexus_errors_total{type="backend_error" or "timeout"}++    │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                         │
│  Meanwhile, in the background...                                        │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │  health/mod.rs :: check_backend()                                  │ │
│  │   📊 nexus_backend_latency_seconds{backend} ← health check time   │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                         │
│  When Prometheus scrapes or a user curls:                               │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │  handler.rs :: metrics_handler()                                   │ │
│  │   📊 nexus_backends_total       ← computed from Registry           │ │
│  │   📊 nexus_backends_healthy     ← computed from Registry           │ │
│  │   📊 nexus_models_available     ← computed from Registry           │ │
│  └────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
```

### Three Types of Metrics

If you're new to Prometheus-style metrics, here are the three types we use:

| Type | What It Is | Analogy | Example |
|------|-----------|---------|---------|
| **Counter** | A number that only goes up | Car odometer | `nexus_requests_total` = 1523 |
| **Histogram** | Tracks the distribution of values | Lap time records | `nexus_request_duration_seconds` = 2.5s |
| **Gauge** | A number that goes up and down | Speedometer | `nexus_backends_healthy` = 3 |

---

## File Structure

```
src/
├── metrics/                     ← NEW MODULE (this feature)
│   ├── mod.rs                   # MetricsCollector: setup, sanitization, gauges
│   ├── types.rs                 # JSON response structs (StatsResponse, etc.)
│   └── handler.rs               # HTTP handlers for /metrics and /v1/stats
├── api/
│   ├── mod.rs                   # MODIFIED: added MetricsCollector to AppState,
│   │                            #           registered new routes
│   └── completions.rs           # MODIFIED: added metrics recording
├── health/
│   └── mod.rs                   # MODIFIED: added backend latency histogram
└── lib.rs                       # MODIFIED: added `pub mod metrics;`

Cargo.toml                       # MODIFIED: added metrics + prometheus deps
```

---

## File 1: mod.rs - The MetricsCollector

This is the heart of the metrics system. It handles three responsibilities:
1. **Setting up** the Prometheus recorder (the thing that stores metric values)
2. **Sanitizing** model/backend names so Prometheus doesn't reject them
3. **Computing** fleet-wide gauges on demand

### The MetricsCollector Struct

```rust
pub struct MetricsCollector {
    /// Reference to backend registry for computing gauges
    registry: Arc<Registry>,
    /// Gateway startup time for uptime calculation
    start_time: Instant,
    /// Thread-safe cache for sanitized Prometheus labels
    label_cache: DashMap<String, String>,
    /// Prometheus handle for rendering metrics
    prometheus_handle: metrics_exporter_prometheus::PrometheusHandle,
}
```

**What each field does:**
- `registry` — Shared reference to the backend registry. Needed to count healthy backends, available models, etc. when computing gauges.
- `start_time` — When the server started. Used to calculate uptime for the `/v1/stats` endpoint.
- `label_cache` — A concurrent `HashMap` that stores already-sanitized label strings. This avoids re-processing the same model name every time a request comes in.
- `prometheus_handle` — The object that can render all recorded metrics as Prometheus text format. Think of it as the "print" button for all metrics.

### Label Sanitization (Why It Matters)

Prometheus has strict rules for label values. Model names like `llama3:70b` or `ollama-local:11434` contain characters (`:`, `-`) that could cause parsing issues. The `sanitize_label` method fixes this:

```rust
pub fn sanitize_label(&self, label: &str) -> String {
    // Check cache first
    if let Some(cached) = self.label_cache.get(label) {
        return cached.clone();
    }

    // Replace non-alphanumeric (except underscore) with underscore
    let mut sanitized = label
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();

    // Prometheus labels can't start with a digit
    if sanitized.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        sanitized.insert(0, '_');
    }

    // Cache and return
    self.label_cache.insert(label.to_string(), sanitized.clone());
    sanitized
}
```

**Step-by-step example:**

```
Input: "ollama-local:11434"

Step 1: Replace invalid chars
  'o' → 'o'  (alphanumeric, keep)
  'l' → 'l'  (alphanumeric, keep)
  ...
  '-' → '_'  (not alphanumeric, replace!)
  ...
  ':' → '_'  (not alphanumeric, replace!)
  '1' → '1'  (alphanumeric, keep)
  ...
  Result: "ollama_local_11434"

Step 2: Check first char
  'o' is not a digit → no prefix needed

Step 3: Cache it
  label_cache["ollama-local:11434"] = "ollama_local_11434"

Output: "ollama_local_11434"
```

Another example where the first character is a digit:

```
Input: "4o"

Step 1: Replace invalid chars → "4o" (no changes)
Step 2: '4' IS a digit → prepend '_'
Output: "_4o"
```

### Fleet Gauges (Pull-Based)

Gauges represent "current state" — how many backends are up right now. Unlike counters (which tick up on every request), gauges are **computed fresh every time someone asks**:

```rust
pub fn update_fleet_gauges(&self) {
    let backends = self.registry.get_all_backends();

    // Total backends (healthy + unhealthy)
    metrics::gauge!("nexus_backends_total").set(backends.len() as f64);

    // Only count healthy ones
    let healthy_count = backends
        .iter()
        .filter(|b| b.status == BackendStatus::Healthy)
        .count();
    metrics::gauge!("nexus_backends_healthy").set(healthy_count as f64);

    // Count unique model names across healthy backends
    let unique_models: HashSet<String> = backends
        .iter()
        .filter(|b| b.status == BackendStatus::Healthy)
        .flat_map(|b| b.models.iter().map(|m| m.id.clone()))
        .collect();
    metrics::gauge!("nexus_models_available").set(unique_models.len() as f64);
}
```

**Why "pull-based" instead of "push-based"?** We could update these numbers every time a backend registers/unregisters/changes health. But that would mean extra work on every health check. Instead, we just compute them on-demand when someone asks for metrics. This is simpler and the cost of counting is negligible.

### setup_metrics() — Global Initialization

```rust
pub fn setup_metrics()
-> Result<metrics_exporter_prometheus::PrometheusHandle, Box<dyn std::error::Error>> {
    use metrics_exporter_prometheus::{Matcher, PrometheusBuilder};

    let duration_buckets = &[
        0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0,
    ];

    let token_buckets = &[
        10.0, 50.0, 100.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0, 32000.0, 64000.0,
        128000.0,
    ];

    let handle = PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("nexus_request_duration_seconds".to_string()),
            duration_buckets,
        )?
        .set_buckets_for_metric(
            Matcher::Full("nexus_backend_latency_seconds".to_string()),
            duration_buckets,
        )?
        .set_buckets_for_metric(
            Matcher::Full("nexus_tokens_total".to_string()),
            token_buckets,
        )?
        .install_recorder()?;

    Ok(handle)
}
```

**What are "buckets"?** Histograms in Prometheus don't store every individual value. Instead, they count how many values fell into each "bucket" (range). For example, with duration buckets `[0.1, 0.5, 1.0, 5.0]`:

```
Request took 0.3s → falls in the 0.5 bucket
Request took 2.1s → falls in the 5.0 bucket
Request took 0.05s → falls in the 0.1 bucket

Prometheus output:
nexus_request_duration_seconds_bucket{le="0.1"} 1
nexus_request_duration_seconds_bucket{le="0.5"} 2   (includes the 0.1 bucket count)
nexus_request_duration_seconds_bucket{le="1.0"} 2
nexus_request_duration_seconds_bucket{le="5.0"} 3
```

We use different bucket ranges for different metrics:
- **Duration buckets** `[0.1s ... 300s]` — LLM requests can take anywhere from 100ms to 5 minutes
- **Token buckets** `[10 ... 128,000]` — prompts/completions can be tiny or fill an entire context window

**Important quirk:** `install_recorder()` sets a **global** recorder. In Rust, you can only set it once. This matters for tests — see [Understanding the Tests](#understanding-the-tests).

---

## File 2: types.rs - JSON Response Shapes

This file defines the shape of the JSON returned by `GET /v1/stats`. There's no logic here — just data containers:

```rust
/// JSON response for GET /v1/stats endpoint.
pub struct StatsResponse {
    pub uptime_seconds: u64,
    pub requests: RequestStats,
    pub backends: Vec<BackendStats>,
    pub models: Vec<ModelStats>,
}

pub struct RequestStats {
    pub total: u64,
    pub success: u64,
    pub errors: u64,
}

pub struct BackendStats {
    pub id: String,
    pub requests: u64,
    pub average_latency_ms: f64,
    pub pending: usize,
}

pub struct ModelStats {
    pub name: String,
    pub requests: u64,
    pub average_duration_ms: f64,
}
```

All structs derive `Serialize` (from the `serde` crate), which lets us convert them to JSON with a single `Json(response)` call in the handler.

**Design note:** These types are separate from the internal `Backend` struct in the registry. This is the **View Model pattern** — we create simple, serializable types for output rather than exposing internal types (which may have `Arc`, atomics, or other non-serializable fields). See [Common Patterns](#common-patterns-in-this-codebase).

---

## File 3: handler.rs - HTTP Endpoints

This file has two handlers — one per endpoint.

### GET /metrics — Prometheus Text Format

```rust
pub async fn metrics_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Update fleet gauges before rendering
    state.metrics_collector.update_fleet_gauges();

    // Get Prometheus text format from collector
    let metrics = state.metrics_collector.render_metrics();
    (StatusCode::OK, metrics)
}
```

This is remarkably simple because all the hard work happens elsewhere:
1. Counters and histograms are recorded in `completions.rs` when requests happen
2. Gauges are computed from the Registry right before rendering
3. `render_metrics()` delegates to the `PrometheusHandle` which formats everything

### GET /v1/stats — JSON Format

```rust
pub async fn stats_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.metrics_collector.update_fleet_gauges();

    let uptime_seconds = state.metrics_collector.uptime_seconds();
    let requests = compute_request_stats();
    let backends = compute_backend_stats(state.metrics_collector.registry());
    let models = compute_model_stats();

    let response = StatsResponse {
        uptime_seconds,
        requests,
        backends,
        models,
    };

    Json(response)
}
```

### Known Limitation: Stubbed Functions

Two of the helper functions return placeholder data:

```rust
fn compute_request_stats() -> RequestStats {
    // The `metrics` crate records metrics but doesn't provide a query API.
    // Use GET /metrics (Prometheus format) for accurate request counts.
    RequestStats { total: 0, success: 0, errors: 0 }
}

fn compute_model_stats() -> Vec<ModelStats> {
    // Same limitation — no way to query histogram values from the metrics crate.
    Vec::new()
}
```

**Why are these stubs?** The `metrics` crate follows a "fire-and-forget" pattern: you record values with `counter!()` and `histogram!()`, but there's no API to ask "what's the current value of counter X?" The only way to get the data back is to parse the Prometheus text output from `render()`.

The `compute_backend_stats()` function **does** return real data because it reads from the Registry's atomic counters directly (not from the metrics crate):

```rust
fn compute_backend_stats(registry: &Registry) -> Vec<BackendStats> {
    let backends = registry.get_all_backends();
    backends.into_iter().map(|backend| {
        BackendStats {
            id: backend.id.clone(),
            requests: backend.total_requests.load(Ordering::SeqCst),
            average_latency_ms: backend.avg_latency_ms.load(Ordering::SeqCst) as f64,
            pending: backend.pending_requests.load(Ordering::SeqCst) as usize,
        }
    }).collect()
}
```

---

## File 4: completions.rs - Where Metrics Get Recorded

This is the busiest file for metrics. Every chat completion request passes through here, and we record metrics at multiple points.

### Timer Start (Line 32)

```rust
pub async fn handle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Response, ApiError> {
    let start_time = std::time::Instant::now();  // ← Start the stopwatch!
    let requested_model = request.model.clone();
    // ...
```

`Instant::now()` captures a monotonic timestamp. Later, `start_time.elapsed()` gives us the duration. We use `Instant` (not `SystemTime`) because it's immune to clock adjustments.

### Error Metrics (Lines 48-61)

When the router can't find a backend, we classify the error type and record it:

```rust
let routing_result = state.router.select_backend(&requirements).map_err(|e| {
    // Classify the error for metrics
    let error_type = match &e {
        RoutingError::ModelNotFound { .. } => "model_not_found",
        RoutingError::FallbackChainExhausted { .. } => "fallback_exhausted",
        RoutingError::NoHealthyBackend { .. } => "no_healthy_backend",
        RoutingError::CapabilityMismatch { .. } => "capability_mismatch",
    };

    let sanitized_model = state.metrics_collector.sanitize_label(&requested_model);
    metrics::counter!("nexus_errors_total",
        "error_type" => error_type,
        "model" => sanitized_model.clone()
    )
    .increment(1);

    // ... convert to ApiError ...
})?;
```

**Why classify errors?** If you just had one `errors_total` counter, a spike could mean anything. By adding the `error_type` label, you can distinguish "all backends are down" (`no_healthy_backend`) from "user requested a model that doesn't exist" (`model_not_found`).

### Success Metrics (Lines 104-149)

On a successful response, we record several things at once:

```rust
Ok(response) => {
    // Stop the timer
    let duration = start_time.elapsed().as_secs_f64();
    let sanitized_model = state.metrics_collector.sanitize_label(&actual_model);
    let sanitized_backend = state.metrics_collector.sanitize_label(&backend.id);

    // 1. Count the request
    metrics::counter!("nexus_requests_total",
        "model" => sanitized_model.clone(),
        "backend" => sanitized_backend.clone(),
        "status" => "200"
    ).increment(1);

    // 2. Record how long it took
    metrics::histogram!("nexus_request_duration_seconds",
        "model" => sanitized_model.clone(),
        "backend" => sanitized_backend.clone()
    ).record(duration);

    // 3. Track fallback usage
    if fallback_used {
        let sanitized_requested =
            state.metrics_collector.sanitize_label(&requested_model);
        metrics::counter!("nexus_fallbacks_total",
            "from_model" => sanitized_requested,
            "to_model" => sanitized_model.clone()
        ).increment(1);
    }

    // 4. Record token usage (if the backend reported it)
    if let Some(ref usage) = response.usage {
        metrics::histogram!("nexus_tokens_total",
            "model" => sanitized_model.clone(),
            "backend" => sanitized_backend.clone(),
            "type" => "prompt"
        ).record(usage.prompt_tokens as f64);

        metrics::histogram!("nexus_tokens_total",
            "model" => sanitized_model.clone(),
            "backend" => sanitized_backend.clone(),
            "type" => "completion"
        ).record(usage.completion_tokens as f64);
    }
}
```

**Notice the pattern:** Every `counter!()` and `histogram!()` call uses `sanitized_model` and `sanitized_backend`. This ensures Prometheus never receives invalid label values.

### Backend Error Metrics (Lines 166-177)

When a backend returns an error (e.g., timeout, 500), we record it differently from routing errors:

```rust
Err(e) => {
    let error_type = if e.error.code.as_deref() == Some("gateway_timeout") {
        "timeout"
    } else {
        "backend_error"
    };

    metrics::counter!("nexus_errors_total",
        "error_type" => error_type,
        "model" => sanitized_model
    ).increment(1);
}
```

---

## File 5: health/mod.rs - Backend Latency Tracking

The health checker runs in the background, polling backends every 30 seconds. When a health check succeeds, we record how long it took:

```rust
// Record backend latency histogram (convert ms to seconds for Prometheus)
let latency_seconds = latency_ms as f64 / 1000.0;
metrics::histogram!("nexus_backend_latency_seconds",
    "backend" => backend.id.clone()
)
.record(latency_seconds);
```

**Why convert ms to seconds?** Prometheus convention is to use base units: seconds for time, bytes for data. The health checker internally measures in milliseconds, so we divide by 1000 before recording.

---

## File 6: api/mod.rs - Wiring It All Together

This file connects all the pieces.

### Adding MetricsCollector to AppState

```rust
pub struct AppState {
    pub registry: Arc<Registry>,
    pub config: Arc<NexusConfig>,
    pub router: Router,
    pub http_client: reqwest::Client,
    pub start_time: Instant,
    pub metrics_collector: Arc<MetricsCollector>,  // ← NEW
}
```

`Arc<MetricsCollector>` means the collector is shared across all request handlers. Since `DashMap` (inside MetricsCollector) is thread-safe, many requests can sanitize labels simultaneously without locking.

### Initializing Metrics in AppState::new()

```rust
// Initialize metrics (safe to call multiple times - will reuse existing if already set)
let prometheus_handle = crate::metrics::setup_metrics().unwrap_or_else(|e| {
    // If metrics are already initialized (e.g., in tests), create a new handle
    tracing::debug!("Metrics already initialized, creating new handle: {}", e);
    crate::metrics::PrometheusBuilder::new()
        .build_recorder()
        .handle()
});
```

**Why the fallback?** `setup_metrics()` calls `install_recorder()`, which sets a global recorder. If it's already been set (by another test or a previous `AppState::new()` call), it returns an error. The fallback creates a recorder handle that works locally without needing global installation.

### Registering Routes

```rust
pub fn create_router(state: Arc<AppState>) -> axum::Router {
    axum::Router::new()
        .route("/v1/chat/completions", post(completions::handle))
        .route("/v1/models", get(models::handle))
        .route("/health", get(health::handle))
        .route("/metrics", get(crate::metrics::handler::metrics_handler))    // ← NEW
        .route("/v1/stats", get(crate::metrics::handler::stats_handler))     // ← NEW
        .layer(RequestBodyLimitLayer::new(MAX_BODY_SIZE))
        .with_state(state)
}
```

---

## Understanding the Tests

### Test Categories

| Category | File | # Tests | What They Verify |
|----------|------|---------|------------------|
| Construction | `metrics/mod.rs` | 1 | MetricsCollector can be created, uptime works |
| Label Sanitization | `metrics/mod.rs` | 4 | Valid names, special chars, leading digits, caching |
| JSON Serialization | `metrics/types.rs` | 1 | StatsResponse serializes correctly |
| Handler Stubs | `metrics/handler.rs` | 3 | Stub functions return expected defaults |

### Test: MetricsCollector Construction

```rust
#[test]
fn test_metrics_collector_construction() {
    let registry = Arc::new(Registry::new());
    let start_time = Instant::now();
    let handle = get_test_handle();

    let collector = MetricsCollector::new(Arc::clone(&registry), start_time, handle);

    assert!(collector.uptime_seconds() < 1); // Should be very small
}
```

**What it verifies:** The collector can be created, and `uptime_seconds()` returns a reasonable value (less than 1 second since we just created it).

### Tests: Label Sanitization (4 tests)

These are the most important unit tests — they verify the sanitization rules:

```rust
#[test]
fn test_label_sanitization_valid_names() {
    // Names that are already valid should pass through unchanged
    assert_eq!(collector.sanitize_label("valid_name"), "valid_name");
    assert_eq!(collector.sanitize_label("ValidName123"), "ValidName123");
    assert_eq!(collector.sanitize_label("_underscore"), "_underscore");
}

#[test]
fn test_label_sanitization_special_chars() {
    // Colons, hyphens, slashes, @ signs → all become underscores
    assert_eq!(collector.sanitize_label("ollama-local:11434"), "ollama_local_11434");
    assert_eq!(collector.sanitize_label("model/gpt-4"), "model_gpt_4");
    assert_eq!(collector.sanitize_label("backend@host"), "backend_host");
}

#[test]
fn test_label_sanitization_leading_digit() {
    // Prometheus labels can't start with a digit → prefix with underscore
    assert_eq!(collector.sanitize_label("123backend"), "_123backend");
    assert_eq!(collector.sanitize_label("4o"), "_4o");
}

#[test]
fn test_label_sanitization_caching() {
    // Same input should return same output (from cache)
    let first = collector.sanitize_label("test-label");
    let second = collector.sanitize_label("test-label");
    assert_eq!(first, second);
    assert_eq!(first, "test_label");
}
```

### The Test Handle Trick (Global Recorder in Tests)

All tests in `mod.rs` share this setup function:

```rust
static INIT: Once = Once::new();
static TEST_HANDLE: Mutex<Option<PrometheusHandle>> = Mutex::new(None);

fn get_test_handle() -> PrometheusHandle {
    INIT.call_once(|| {
        // build_recorder() creates a recorder WITHOUT installing it globally
        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        *TEST_HANDLE.lock().unwrap() = Some(handle);

        // Install globally (once for all tests in this module)
        metrics::set_global_recorder(Box::new(recorder)).ok();
    });

    TEST_HANDLE.lock().unwrap().as_ref().unwrap().clone()
}
```

**Why this complexity?** The `metrics` crate allows setting the global recorder **exactly once** per process. Since `cargo test` runs all tests in one process, we use `Once::call_once` to ensure setup runs only once, no matter how many tests call `get_test_handle()`. The `Mutex<Option<...>>` stores the handle so subsequent calls can clone it.

### Test: StatsResponse Serialization

```rust
#[test]
fn test_stats_response_serialization() {
    let response = StatsResponse {
        uptime_seconds: 3600,
        requests: RequestStats { total: 1000, success: 950, errors: 50 },
        backends: vec![BackendStats {
            id: "ollama-local".to_string(),
            requests: 500,
            average_latency_ms: 1250.5,
            pending: 2,
        }],
        models: vec![ModelStats {
            name: "llama3:70b".to_string(),
            requests: 300,
            average_duration_ms: 5000.0,
        }],
    };

    let json = serde_json::to_string(&response).expect("Failed to serialize");
    assert!(json.contains("uptime_seconds"));
    assert!(json.contains("3600"));
    assert!(json.contains("ollama-local"));
    assert!(json.contains("llama3:70b"));
}
```

**What it verifies:** The struct serializes to JSON correctly — field names, values, and nested objects all appear in the output.

### Tests: Handler Stubs

```rust
#[test]
fn test_compute_request_stats_stub() {
    let stats = compute_request_stats();
    assert_eq!(stats.total, 0);  // Stub returns zeros
}

#[test]
fn test_compute_backend_stats_empty() {
    let registry = Registry::new();
    let stats = compute_backend_stats(&registry);
    assert_eq!(stats.len(), 0);  // Empty registry → empty vec
}

#[test]
fn test_compute_model_stats_stub() {
    let stats = compute_model_stats();
    assert_eq!(stats.len(), 0);  // Stub returns empty vec
}
```

These tests document the current behavior of the stubs and will need updating when the implementations are completed.

---

## Key Rust Concepts

| Concept | What It Means | Example in This Code |
|---------|---------------|----------------------|
| `Arc<T>` | Shared ownership across threads | `Arc<MetricsCollector>` in AppState |
| `DashMap<K, V>` | Thread-safe HashMap (no `Mutex` needed) | `label_cache` in MetricsCollector |
| `Instant` | Monotonic timestamp (not wall clock) | `start_time` for duration tracking |
| `Once` | Run initialization exactly once | `INIT.call_once` for test recorder |
| `counter!()` / `histogram!()` / `gauge!()` | `metrics` crate macros to record values | Throughout completions.rs |
| `impl IntoResponse` | Axum trait for HTTP response types | Handler return types |
| `Ordering::SeqCst` | Strictest atomics ordering | Reading backend counters |
| `flat_map()` | Map + flatten in one step | Collecting models from all backends |

### The `metrics` Crate's "Macro + Facade" Pattern

```rust
// This macro does NOT store the counter locally.
// It looks up (or creates) a global counter by name + labels,
// then calls .increment(1) on it.
metrics::counter!("nexus_requests_total",
    "model" => "llama3",
    "backend" => "gpu1",
    "status" => "200"
).increment(1);
```

The `counter!()` macro is a **facade** — it hides the underlying Prometheus recorder. You don't need a reference to the recorder; the macro finds it via the global installation from `setup_metrics()`.

### DashMap vs Mutex\<HashMap\>

```rust
// With Mutex<HashMap> — one thread at a time:
let mut map = self.label_cache.lock().unwrap();
map.insert(key, value);
// Lock released here — other threads were waiting

// With DashMap — multiple threads simultaneously:
self.label_cache.insert(key, value);
// No lock! DashMap uses internal sharding
```

We use `DashMap` because label sanitization can happen on many request threads simultaneously.

---

## Common Patterns in This Codebase

### Pattern 1: View Model (Internal → Display)

```rust
// Internal type (complex: atomics, Arc, business logic)
pub struct Backend {
    pub total_requests: AtomicU64,     // Can't serialize!
    pub avg_latency_ms: AtomicU64,     // Can't serialize!
    // ...
}

// Display type (simple: plain types, Serialize)
pub struct BackendStats {
    pub requests: u64,                  // Just a number
    pub average_latency_ms: f64,        // Just a number
    // ...
}

// Conversion in handler.rs
BackendStats {
    id: backend.id.clone(),
    requests: backend.total_requests.load(Ordering::SeqCst),  // Atomic → u64
    average_latency_ms: backend.avg_latency_ms.load(Ordering::SeqCst) as f64,
    pending: backend.pending_requests.load(Ordering::SeqCst) as usize,
}
```

### Pattern 2: Record-Then-Forget

```rust
// Record the metric...
metrics::counter!("nexus_requests_total", /* labels */).increment(1);

// ...and move on. No return value, no error handling needed.
// The metrics crate handles storage internally.
```

This "fire-and-forget" pattern keeps metrics recording out of the critical path. Even if the recorder somehow fails, it won't affect the request.

### Pattern 3: Sanitize Before Record

```rust
// ALWAYS sanitize labels before passing to metrics macros
let sanitized_model = state.metrics_collector.sanitize_label(&actual_model);
let sanitized_backend = state.metrics_collector.sanitize_label(&backend.id);

metrics::counter!("nexus_requests_total",
    "model" => sanitized_model,       // ← Sanitized
    "backend" => sanitized_backend,   // ← Sanitized
    "status" => "200"                 // ← Static, no sanitization needed
).increment(1);
```

### Pattern 4: Pull-Based Gauges

```rust
// DON'T push gauge updates on every change:
//   registry.add_backend(b);
//   gauge!("nexus_backends_total").increment(1);  // Error-prone!

// DO compute them on demand:
pub fn update_fleet_gauges(&self) {
    let backends = self.registry.get_all_backends();
    gauge!("nexus_backends_total").set(backends.len() as f64);  // Always correct
}
```

---

## Next Steps

Now that you understand Request Metrics, explore:

1. **Backend Registry** (`src/registry/mod.rs`) — Where `total_requests`, `avg_latency_ms`, and `pending_requests` atomics live
2. **Intelligent Router** (`src/routing/`) — How `select_backend()` works (which this feature instruments)
3. **Health Checker** (`src/health/mod.rs`) — The background loop that records `nexus_backend_latency_seconds`

### Try It Yourself

1. Start Nexus with debug logging:
   ```bash
   RUST_LOG=debug cargo run -- serve
   ```

2. Make a few requests to generate metrics:
   ```bash
   curl -X POST http://localhost:8000/v1/chat/completions \
     -H "Content-Type: application/json" \
     -d '{"model": "llama3:8b", "messages": [{"role": "user", "content": "Hi"}]}'
   ```

3. Check Prometheus metrics:
   ```bash
   curl http://localhost:8000/metrics
   ```

4. Check JSON stats:
   ```bash
   curl http://localhost:8000/v1/stats | jq
   ```

5. Compare the two — notice that `/metrics` has the full data while `/v1/stats` has partial data (stubs for request totals and model stats)
