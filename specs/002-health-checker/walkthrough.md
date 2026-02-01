# Health Checker - Code Walkthrough

**Feature**: F03 - Health Checker  
**Audience**: Junior developers joining the project  
**Last Updated**: 2026-02-01

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: config.rs - The Settings](#file-1-configrs---the-settings)
4. [File 2: error.rs - What Can Go Wrong](#file-2-errorrs---what-can-go-wrong)
5. [File 3: state.rs - Tracking Each Backend](#file-3-staters---tracking-each-backend)
6. [File 4: parser.rs - Understanding Backend Responses](#file-4-parserrs---understanding-backend-responses)
7. [File 5: mod.rs - The Main Logic](#file-5-modrs---the-main-logic)
8. [Understanding the Tests](#understanding-the-tests)
9. [Key Rust Concepts](#key-rust-concepts)
10. [Common Patterns in This Module](#common-patterns-in-this-module)

---

## The Big Picture

Think of the Health Checker as a **doctor making rounds**. Every 30 seconds, it visits each backend server, checks if it's healthy, and updates its medical chart (the Registry).

### Why Do We Need This?

Imagine you have 5 AI servers. One crashes. Without a health checker:
- Users would keep getting errors
- No one would know until someone complains

With a health checker:
- Nexus detects the crash within 30 seconds
- Requests automatically route to healthy servers
- You can sleep at night!

### How It Fits in Nexus

```
┌─────────────────────────────────────────────────────────────────┐
│                         Nexus                                   │
│                                                                 │
│  ┌──────────┐     ┌──────────┐     ┌──────────────────────┐    │
│  │   API    │────▶│  Router  │────▶│  Backend Registry    │    │
│  │ Gateway  │     │          │     │                      │    │
│  └──────────┘     └──────────┘     └──────────────────────┘    │
│                                              ▲                  │
│                                              │ updates          │
│                                    ┌─────────┴────────┐         │
│                                    │  Health Checker  │         │
│                                    │  (you are here!) │         │
│                                    └──────────────────┘         │
└─────────────────────────────────────────────────────────────────┘
```

The Health Checker runs in the background, periodically asking each backend "Are you alive?" and updating the Registry with what it finds.

---

## File Structure

```
src/health/
├── mod.rs      (270 lines) - Main HealthChecker struct and loop
├── config.rs   (31 lines)  - Configuration with sensible defaults
├── error.rs    (31 lines)  - Error types (timeout, connection, etc.)
├── state.rs    (85 lines)  - Per-backend state (failures, models)
├── parser.rs   (91 lines)  - Parse Ollama/OpenAI/LlamaCpp responses
└── tests.rs    (414 lines) - 40 unit tests

tests/
└── health_integration.rs (279 lines) - 6 tests with mock HTTP servers
```

---

## File 1: config.rs - The Settings

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HealthCheckConfig {
    pub enabled: bool,           // Turn health checking on/off
    pub interval_seconds: u64,   // How often to check (default: 30)
    pub timeout_seconds: u64,    // Max wait for response (default: 5)
    pub failure_threshold: u32,  // Failures before "unhealthy" (default: 3)
    pub recovery_threshold: u32, // Successes before "healthy" (default: 2)
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: 30,
            timeout_seconds: 5,
            failure_threshold: 3,
            recovery_threshold: 2,
        }
    }
}
```

**Breaking it down:**

| Field | Default | Why This Value? |
|-------|---------|-----------------|
| `enabled` | `true` | Health checking should be on by default |
| `interval_seconds` | `30` | Often enough to detect problems, not so often we overwhelm backends |
| `timeout_seconds` | `5` | Long enough for slow backends, short enough to fail fast |
| `failure_threshold` | `3` | Don't panic on one hiccup - require 3 failures |
| `recovery_threshold` | `2` | Make sure recovery is real, not a fluke |

**What `#[serde(default)]` means:**

When parsing TOML like this:
```toml
[health_check]
interval_seconds = 60
```

Missing fields (like `timeout_seconds`) use their default values instead of causing an error.

---

## File 2: error.rs - What Can Go Wrong

```rust
#[derive(Debug, Clone, thiserror::Error)]
pub enum HealthCheckError {
    #[error("request timeout after {0}s")]
    Timeout(u64),

    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("DNS resolution failed: {0}")]
    DnsError(String),

    #[error("TLS certificate error: {0}")]
    TlsError(String),

    #[error("HTTP error: {0}")]
    HttpError(u16),

    #[error("invalid response: {0}")]
    ParseError(String),
}
```

**Breaking it down:**

| Error | When It Happens | Example |
|-------|-----------------|---------|
| `Timeout(5)` | Backend didn't respond in time | Server is overloaded |
| `ConnectionFailed(...)` | Couldn't connect at all | Server is down, port closed |
| `DnsError(...)` | Hostname doesn't resolve | `ollama.local` not found |
| `TlsError(...)` | SSL/TLS problem | Expired certificate |
| `HttpError(503)` | Backend returned error status | Server says it's unhealthy |
| `ParseError(...)` | Response is garbage | Backend returned HTML instead of JSON |

**Key Concept - thiserror:**

The `#[error("...")]` attribute automatically implements `Display` for each variant. So `HealthCheckError::Timeout(5).to_string()` gives you `"request timeout after 5s"`.

---

## File 3: state.rs - Tracking Each Backend

### Part 1: The State Struct

```rust
#[derive(Debug, Clone)]
pub struct BackendHealthState {
    pub consecutive_failures: u32,    // How many failures in a row?
    pub consecutive_successes: u32,   // How many successes in a row?
    pub last_check_time: Option<DateTime<Utc>>,  // When was last check?
    pub last_status: BackendStatus,   // What's the current status?
    pub last_models: Vec<Model>,      // What models did we last see?
}
```

**Why track consecutive counts?**

Imagine a backend that's flaky - sometimes it responds, sometimes it doesn't:

```
Check 1: ✓ Success
Check 2: ✗ Failure   (consecutive_failures = 1)
Check 3: ✓ Success   (consecutive_failures reset to 0!)
Check 4: ✗ Failure   (consecutive_failures = 1)
Check 5: ✗ Failure   (consecutive_failures = 2)
Check 6: ✓ Success   (consecutive_failures reset to 0!)
```

With a threshold of 3, this backend stays "Healthy" because it never fails 3 times in a row. This prevents "flapping" - rapidly switching between healthy/unhealthy.

### Part 2: The Health Check Result

```rust
pub enum HealthCheckResult {
    Success {
        latency_ms: u32,      // How long did the request take?
        models: Vec<Model>,   // What models are available?
    },
    Failure {
        error: HealthCheckError,  // What went wrong?
    },
}
```

**Either it worked or it didn't.** Rust enums are perfect for this - no null, no ambiguity.

### Part 3: The Status Transition Logic

```rust
pub fn apply_result(
    &mut self,
    result: &HealthCheckResult,
    config: &HealthCheckConfig,
) -> Option<BackendStatus> {
    match result {
        HealthCheckResult::Success { .. } => {
            self.consecutive_failures = 0;     // Reset failures
            self.consecutive_successes += 1;   // Count success

            match self.last_status {
                // First check ever: immediately healthy
                BackendStatus::Unknown => Some(BackendStatus::Healthy),

                // Was unhealthy, need N successes to recover
                BackendStatus::Unhealthy
                    if self.consecutive_successes >= config.recovery_threshold =>
                {
                    Some(BackendStatus::Healthy)
                }

                // Already healthy or not enough successes yet
                _ => None,
            }
        }
        HealthCheckResult::Failure { .. } => {
            self.consecutive_successes = 0;    // Reset successes
            self.consecutive_failures += 1;    // Count failure

            match self.last_status {
                // First check ever: immediately unhealthy
                BackendStatus::Unknown => Some(BackendStatus::Unhealthy),

                // Was healthy, need N failures to mark unhealthy
                BackendStatus::Healthy
                    if self.consecutive_failures >= config.failure_threshold =>
                {
                    Some(BackendStatus::Unhealthy)
                }

                // Already unhealthy or not enough failures yet
                _ => None,
            }
        }
    }
}
```

**The State Machine:**

```
          ┌─────────┐
          │ Unknown │ ← All backends start here
          └────┬────┘
               │
    1 success  │  1 failure
        ┌──────┴───────┐
        ▼              ▼
   ┌─────────┐    ┌───────────┐
   │ Healthy │    │ Unhealthy │
   └────┬────┘    └─────┬─────┘
        │               │
        │ 3 failures    │ 2 successes
        │               │
        └───────┬───────┘
                ▼
           Status change!
```

**Why return `Option<BackendStatus>`?**

- `Some(status)` = "Status changed! Update the registry."
- `None` = "No change, don't bother updating."

---

## File 4: parser.rs - Understanding Backend Responses

### Parsing Ollama Responses

```rust
#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
}

pub fn parse_ollama_response(body: &str) -> Result<Vec<Model>, HealthCheckError> {
    // Step 1: Parse JSON
    let response: OllamaTagsResponse = serde_json::from_str(body)
        .map_err(|e| HealthCheckError::ParseError(e.to_string()))?;

    // Step 2: Convert to our Model type
    Ok(response.models.into_iter().map(|m| {
        let name_lower = m.name.to_lowercase();

        // Auto-detect capabilities from model name
        let supports_vision = name_lower.contains("llava")
            || name_lower.contains("vision");
        let supports_tools = name_lower.contains("mistral");

        Model {
            id: m.name.clone(),
            name: m.name,
            context_length: 4096,  // Ollama doesn't tell us, assume 4K
            supports_vision,
            supports_tools,
            supports_json_mode: false,
            max_output_tokens: None,
        }
    }).collect())
}
```

**What Ollama returns:**

```json
{
  "models": [
    {"name": "llama3:70b"},
    {"name": "llava:13b"},
    {"name": "mistral:7b"}
  ]
}
```

**What we do with it:**

1. Parse the JSON
2. Extract model names
3. Guess capabilities from the name:
   - "llava" → probably supports vision
   - "mistral" → probably supports tool calling
4. Create our `Model` structs

### Parsing OpenAI-Format Responses

```rust
pub fn parse_openai_response(body: &str) -> Result<Vec<Model>, HealthCheckError> {
    let response: OpenAIModelsResponse = serde_json::from_str(body)
        .map_err(|e| HealthCheckError::ParseError(e.to_string()))?;

    Ok(response.data.into_iter().map(|m| {
        Model {
            id: m.id.clone(),
            name: m.id,
            context_length: 4096,
            supports_vision: false,  // No way to know
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }
    }).collect())
}
```

**What vLLM/OpenAI returns:**

```json
{
  "data": [
    {"id": "gpt-4", "object": "model"},
    {"id": "gpt-3.5-turbo", "object": "model"}
  ]
}
```

### Parsing LlamaCpp Responses

```rust
pub fn parse_llamacpp_response(body: &str) -> Result<bool, HealthCheckError> {
    let response: LlamaCppHealthResponse = serde_json::from_str(body)
        .map_err(|e| HealthCheckError::ParseError(e.to_string()))?;

    Ok(response.status == "ok")  // Just returns true/false!
}
```

**What llama.cpp returns:**

```json
{"status": "ok"}
```

LlamaCpp doesn't tell us about models - it just says if it's running.

---

## File 5: mod.rs - The Main Logic

### Part 1: The HealthChecker Struct

```rust
pub struct HealthChecker {
    registry: Arc<Registry>,                    // Shared reference to registry
    client: reqwest::Client,                    // HTTP client (reused!)
    config: HealthCheckConfig,                  // Our settings
    state: DashMap<String, BackendHealthState>, // Per-backend tracking
}
```

**Why these types?**

| Field | Type | Why? |
|-------|------|------|
| `registry` | `Arc<Registry>` | Shared with other components (API, Router) |
| `client` | `reqwest::Client` | Reuse connections, don't create per-request |
| `config` | `HealthCheckConfig` | Our settings (interval, thresholds, etc.) |
| `state` | `DashMap<...>` | Thread-safe map for per-backend state |

### Part 2: Choosing the Right Endpoint

```rust
pub fn get_health_endpoint(backend_type: BackendType) -> &'static str {
    match backend_type {
        BackendType::Ollama => "/api/tags",
        BackendType::LlamaCpp => "/health",
        BackendType::VLLM | BackendType::Exo |
        BackendType::OpenAI | BackendType::Generic => "/v1/models",
    }
}
```

**Each backend has its own API.** Ollama uses `/api/tags`, vLLM uses `/v1/models`, etc.

### Part 3: Checking One Backend

```rust
pub async fn check_backend(&self, backend: &Backend) -> HealthCheckResult {
    // Step 1: Build the URL
    let endpoint = Self::get_health_endpoint(backend.backend_type);
    let url = format!("{}{}", backend.url, endpoint);

    // Step 2: Start timer
    let start = Instant::now();

    // Step 3: Make HTTP request
    match self.client
        .get(&url)
        .timeout(Duration::from_secs(self.config.timeout_seconds))
        .send()
        .await
    {
        Ok(response) => {
            let latency_ms = start.elapsed().as_millis() as u32;

            // Step 4a: Check status code
            if !response.status().is_success() {
                return HealthCheckResult::Failure {
                    error: HealthCheckError::HttpError(response.status().as_u16()),
                };
            }

            // Step 4b: Parse response body
            match response.text().await {
                Ok(body) => self.parse_response(backend.backend_type, &body, latency_ms),
                Err(e) => HealthCheckResult::Failure {
                    error: HealthCheckError::ParseError(e.to_string()),
                },
            }
        }
        Err(e) => HealthCheckResult::Failure {
            error: Self::classify_error(e, self.config.timeout_seconds),
        },
    }
}
```

**Step by step:**

1. Build URL: `http://localhost:11434` + `/api/tags`
2. Start a timer (to measure latency)
3. Send GET request with timeout
4. If response OK → parse the body
5. If error → classify what went wrong

### Part 4: The Main Loop

```rust
pub fn start(self, cancel_token: CancellationToken) -> JoinHandle<()> {
    tokio::spawn(async move {
        // Create interval timer
        let mut interval = tokio::time::interval(
            Duration::from_secs(self.config.interval_seconds)
        );
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        tracing::info!(interval_seconds = self.config.interval_seconds,
            "Health checker started");

        loop {
            tokio::select! {
                // Option 1: Shutdown requested
                _ = cancel_token.cancelled() => {
                    tracing::info!("Health checker shutting down");
                    break;
                }
                // Option 2: Time for next check
                _ = interval.tick() => {
                    self.check_all_backends().await;
                }
            }
        }
    })
}
```

**Understanding `tokio::select!`:**

This waits for EITHER:
- The cancel token to be triggered (shutdown), OR
- The interval timer to tick (time for checks)

Whichever happens first wins. It's like `Promise.race()` in JavaScript.

**Why `MissedTickBehavior::Skip`?**

If a health check takes 45 seconds but the interval is 30 seconds:
- **Without Skip**: Would try to "catch up" by running checks back-to-back
- **With Skip**: Just waits for the next interval

---

## Understanding the Tests

### Test Categories

| Category | Count | What It Tests |
|----------|-------|---------------|
| Config | 4 | Default values, TOML parsing |
| Errors | 6 | Error messages display correctly |
| State | 2 | Default state, cloning |
| Transitions | 8 | All threshold scenarios |
| Parsing | 14 | Ollama, OpenAI, LlamaCpp formats |
| Endpoints | 6 | Correct endpoint per backend type |

### Example: Testing Configuration Defaults

```rust
#[test]
fn test_config_default_values() {
    let config = HealthCheckConfig::default();

    assert!(config.enabled);
    assert_eq!(config.interval_seconds, 30);
    assert_eq!(config.timeout_seconds, 5);
    assert_eq!(config.failure_threshold, 3);
    assert_eq!(config.recovery_threshold, 2);
}
```

**What this tests:** When someone creates a `HealthCheckConfig::default()`, they get sensible values.

### Example: Testing Status Transitions

```rust
#[test]
fn test_healthy_to_unhealthy_at_threshold() {
    // ARRANGE: Start with a healthy backend
    let mut state = BackendHealthState {
        last_status: BackendStatus::Healthy,
        ..Default::default()
    };
    let config = HealthCheckConfig::default(); // failure_threshold = 3
    let failure = HealthCheckResult::Failure {
        error: HealthCheckError::Timeout(5)
    };

    // ACT & ASSERT: First two failures - no transition
    assert_eq!(state.apply_result(&failure, &config), None);
    assert_eq!(state.consecutive_failures, 1);

    assert_eq!(state.apply_result(&failure, &config), None);
    assert_eq!(state.consecutive_failures, 2);

    // ACT & ASSERT: Third failure - transition!
    assert_eq!(
        state.apply_result(&failure, &config),
        Some(BackendStatus::Unhealthy)
    );
    assert_eq!(state.consecutive_failures, 3);
}
```

**What this tests:** A healthy backend needs exactly 3 consecutive failures to become unhealthy.

### Example: Testing Parser

```rust
#[test]
fn test_parse_ollama_vision_model() {
    let body = r#"{"models": [{"name": "llava:13b"}]}"#;

    let models = parse_ollama_response(body).unwrap();

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "llava:13b");
    assert!(models[0].supports_vision);  // Detected from name!
}
```

**What this tests:** When Ollama returns a model with "llava" in the name, we correctly detect it supports vision.

### Integration Tests with Mock Servers

```rust
#[tokio::test]
async fn test_full_health_check_cycle_ollama() {
    // ARRANGE: Start a fake Ollama server
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": [{"name": "llama3:70b"}, {"name": "mistral:7b"}]
        })))
        .mount(&mock_server)
        .await;

    // Create real registry + health checker
    let registry = Arc::new(Registry::new());
    let backend = Backend::new("test", &mock_server.uri(), BackendType::Ollama);
    registry.add_backend(backend.clone()).unwrap();

    let config = HealthCheckConfig {
        interval_seconds: 1,  // Fast for testing
        ..Default::default()
    };
    let checker = HealthChecker::new(registry.clone(), config);

    // ACT: Run health checker
    let cancel = CancellationToken::new();
    let handle = checker.start(cancel.clone());
    tokio::time::sleep(Duration::from_millis(1500)).await;
    cancel.cancel();
    handle.await.unwrap();

    // ASSERT: Registry was updated
    let updated = registry.get_backend(&backend.id).unwrap();
    assert_eq!(updated.status, BackendStatus::Healthy);
    assert_eq!(updated.models.len(), 2);
}
```

**What this tests:**
1. Health checker calls the mock server
2. Mock returns Ollama-format response
3. Registry gets updated with status + models
4. Graceful shutdown works

---

## Key Rust Concepts

| Concept | What It Means | Example |
|---------|---------------|---------|
| `async fn` | Function that can pause/resume | `async fn check_backend(...)` |
| `await` | Pause until async operation completes | `response.text().await` |
| `Arc<T>` | Shared ownership across threads | `Arc<Registry>` |
| `DashMap` | Thread-safe HashMap | `DashMap<String, BackendHealthState>` |
| `Option<T>` | Either `Some(value)` or `None` | `Option<BackendStatus>` for "maybe changed" |
| `Result<T, E>` | Either `Ok(value)` or `Err(error)` | `Result<Vec<Model>, HealthCheckError>` |
| `match` | Pattern matching | `match backend_type { Ollama => ... }` |
| `tokio::spawn` | Start a background task | `tokio::spawn(async move { ... })` |
| `tokio::select!` | Wait for first of multiple async events | Used in the main loop |

---

## Common Patterns in This Module

### Pattern 1: The "Get or Create" Pattern

```rust
let mut state = self.state.entry(backend_id.to_string()).or_default();
```

**What it does:**
- If `backend_id` exists in the map → get mutable reference
- If not → create a new entry with `Default::default()` and get mutable reference

This is super common with DashMap and HashMap.

### Pattern 2: Graceful Shutdown with CancellationToken

```rust
// Caller:
let cancel = CancellationToken::new();
let handle = checker.start(cancel.clone());
// ... later ...
cancel.cancel();
handle.await.unwrap();

// Inside the loop:
tokio::select! {
    _ = cancel_token.cancelled() => break,
    _ = do_work() => {}
}
```

**Why this pattern?**
- Clean shutdown: finish what you're doing, then exit
- No orphaned tasks
- No resource leaks

### Pattern 3: Backend-Specific Logic with Match

```rust
match backend_type {
    BackendType::Ollama => parse_ollama_response(body),
    BackendType::LlamaCpp => parse_llamacpp_response(body),
    BackendType::VLLM | BackendType::Exo |
    BackendType::OpenAI | BackendType::Generic => parse_openai_response(body),
}
```

**Why this pattern?**
- Compiler ensures we handle ALL variants
- Adding a new backend type? Compiler tells you everywhere to update
- No "default" case hiding bugs

### Pattern 4: Map Error Types

```rust
let response: OllamaTagsResponse = serde_json::from_str(body)
    .map_err(|e| HealthCheckError::ParseError(e.to_string()))?;
```

**What it does:**
- `serde_json::from_str` returns `Result<T, serde_json::Error>`
- We want `Result<T, HealthCheckError>`
- `.map_err(...)` converts the error type
- `?` returns early if it's an error

---

## Summary

The Health Checker is a background service that:

1. **Runs continuously** using `tokio::spawn` and an interval timer
2. **Checks all backends** by calling their health endpoints
3. **Parses responses** differently for Ollama, OpenAI, and LlamaCpp
4. **Uses thresholds** to prevent status flapping (3 failures, 2 recoveries)
5. **Updates the Registry** with status, models, and latency
6. **Shuts down gracefully** when asked via CancellationToken

### Key Files to Remember

| File | One-Sentence Summary |
|------|---------------------|
| `config.rs` | Settings with sensible defaults |
| `error.rs` | All the ways a health check can fail |
| `state.rs` | Tracks failures/successes per backend |
| `parser.rs` | Understands Ollama, OpenAI, LlamaCpp responses |
| `mod.rs` | The main loop and HTTP logic |

### Next Steps

Now that you understand the Health Checker, you're ready to explore:

- **The Router** - Uses health status to pick backends
- **The API Gateway** - Starts the health checker on startup
- **Adding a new backend type** - Try adding "LocalAI"!
