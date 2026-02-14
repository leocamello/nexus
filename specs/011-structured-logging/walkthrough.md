# F11: Structured Request Logging — Code Walkthrough

**Feature**: F11 - Structured Request Logging  
**Audience**: Junior developers joining the project  
**Last Updated**: 2025-07-18

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: logging/mod.rs — Module Exports + Filter Builder](#file-1-loggingmodrs--module-exports--filter-builder)
4. [File 2: logging/middleware.rs — Request ID Generation](#file-2-loggingmiddlewarers--request-id-generation)
5. [File 3: logging/fields.rs — Field Extraction Helpers](#file-3-loggingfieldsrs--field-extraction-helpers)
6. [File 4: api/completions.rs — Span Instrumentation + Retry Tracking](#file-4-apicompletionsrs--span-instrumentation--retry-tracking)
7. [File 5: routing/mod.rs — Route Reason](#file-5-routingmodrs--route-reason)
8. [File 6: config/logging.rs — Logging Configuration](#file-6-configloggingrs--logging-configuration)
9. [File 7: cli/serve.rs — Tracing Initialization](#file-7-cliservers--tracing-initialization)
10. [How Data Flows](#how-data-flows)
11. [Key Tests Explained](#key-tests-explained)
12. [Key Rust Concepts](#key-rust-concepts)
13. [Common Questions](#common-questions)

---

## The Big Picture

Think of structured logging as the **flight recorder** for Nexus. Without it, you'd only know that a request happened — but not which backend handled it, why that backend was chosen, how many retries occurred, or how long the whole thing took. Structured logging captures all of that metadata in machine-parseable fields, so you can pipe logs into tools like Grafana Loki, Datadog, or `jq` and answer questions like:

- "Which requests are taking longer than 2 seconds?"
- "How often does the fallback chain get triggered?"
- "Is backend X seeing more errors than backend Y?"

### What Problem Does This Solve?

When Nexus routes requests across multiple backends, operators need **observability** — the ability to understand what happened after the fact. Plain text logs like `"Request completed"` aren't enough. You need structured fields (`latency_ms=1234`, `backend="ollama-local"`, `retry_count=2`) that tools can filter, aggregate, and alert on.

### Key Design Decisions

1. **Privacy by default** — Message content is **never** logged unless an operator explicitly opts in via `enable_content_logging = true`. Even then, content is truncated to ~100 characters.
2. **Correlation IDs** — Every request gets a UUID v4 `request_id` generated once and reused across all retries. This lets you find every log line from a single user request.
3. **Deferred field recording** — The `#[instrument]` macro creates a span with empty fields at the start. Fields like `backend`, `latency_ms`, and `status` are filled in later as the request progresses. This is the `tracing` crate's "record later" pattern.
4. **Component-level filtering** — Instead of one global log level, operators can set `routing=debug` while keeping `api=info`. This avoids drowning in noise while debugging a specific subsystem.

### Architecture Overview

```
┌──────────────────────────────────────────────────────────────────────────┐
│              Structured Logging Data Flow                                │
│                                                                          │
│  ① Request arrives at POST /v1/chat/completions                          │
│     │                                                                    │
│  ② #[instrument] macro creates a tracing span with empty fields:         │
│     │  request_id=Empty, model="gpt-4", backend=Empty,                   │
│     │  status=Empty, latency_ms=Empty, ...                               │
│     │                                                                    │
│  ③ generate_request_id() → UUID v4 → Span::current().record()           │
│     │                                                                    │
│  ④ Router selects backend → record backend, route_reason                 │
│     │                                                                    │
│  ⑤ Proxy to backend (retry loop):                                       │
│     │  ├─ Success → record status="success", tokens, latency_ms          │
│     │  └─ Failure → record retry_count++, error_message,                 │
│     │              fallback_chain, retry again                            │
│     │                                                                    │
│  ⑥ Span closes → tracing subscriber emits structured log line            │
│     │                                                                    │
│  ⑦ Output (depends on config):                                           │
│     ├─ format = "pretty" → human-readable colored output                 │
│     └─ format = "json"  → machine-parseable JSON log line                │
│                                                                          │
│  Example JSON output:                                                    │
│  {                                                                       │
│    "timestamp": "2025-02-14T10:00:00Z",                                  │
│    "level": "INFO",                                                      │
│    "target": "nexus::api::completions",                                  │
│    "fields": {                                                           │
│      "request_id": "550e8400-...",                                       │
│      "model": "gpt-4",                                                   │
│      "backend": "ollama-local",                                          │
│      "status": "success",                                                │
│      "latency_ms": 1234,                                                 │
│      "tokens_prompt": 100,                                               │
│      "route_reason": "highest_score:Backend 1:0.95"                      │
│    }                                                                     │
│  }                                                                       │
└──────────────────────────────────────────────────────────────────────────┘
```

---

## File Structure

```
src/
├── logging/                     ← NEW MODULE (this feature)
│   ├── mod.rs                   # Module exports + build_filter_directives()
│   ├── middleware.rs             # UUID v4 request ID generation
│   └── fields.rs                # Token extraction, status extraction, prompt truncation
├── api/
│   └── completions.rs           # MODIFIED: #[instrument] macro, span field recording
├── routing/
│   └── mod.rs                   # MODIFIED: route_reason field on RoutingResult
├── config/
│   └── logging.rs               # MODIFIED: component_levels + enable_content_logging
├── cli/
│   └── serve.rs                 # MODIFIED: init_tracing() with JSON layer + EnvFilter
tests/
└── structured_logging.rs        # 20 tests across 6 user story modules
```

---

## File 1: logging/mod.rs — Module Exports + Filter Builder

This is the entry point for the logging module. It re-exports the public API and contains one function: `build_filter_directives()`.

### Module Re-exports

```rust
pub mod fields;
pub mod middleware;

pub use fields::{extract_status, extract_tokens, truncate_prompt};
pub use middleware::generate_request_id;
```

These `pub use` lines create a flat public API. Instead of writing `nexus::logging::fields::extract_tokens`, consumers can write `nexus::logging::extract_tokens`. This keeps import lines short throughout the codebase.

### build_filter_directives() — Constructing the EnvFilter String

```rust
pub fn build_filter_directives(config: &crate::config::LoggingConfig) -> String {
    let mut filter_str = config.level.clone();

    if let Some(component_levels) = &config.component_levels {
        for (component, level) in component_levels {
            filter_str.push_str(&format!(",nexus::{}={}", component, level));
        }
    }

    filter_str
}
```

**What this produces:** A string like `"info,nexus::routing=debug,nexus::api=warn"`. This is the syntax that `tracing_subscriber::EnvFilter` expects — a comma-separated list of directives where each directive is either a global level (`info`) or a target-specific level (`nexus::routing=debug`).

**Why `nexus::` prefix?** Rust's `tracing` crate uses the module path as the log target. A log emitted from `src/routing/mod.rs` has target `nexus::routing`. The prefix ensures our component filters only affect Nexus logs, not logs from dependencies like `hyper` or `tokio`.

**Example configurations:**

| Config | Filter String | Effect |
|--------|---------------|--------|
| `level = "info"`, no components | `"info"` | Everything at INFO or above |
| `level = "warn"`, `routing = "debug"` | `"warn,nexus::routing=debug"` | Only WARN globally, but full DEBUG for routing |
| `level = "error"`, `routing = "trace"`, `api = "info"` | `"error,nexus::routing=trace,nexus::api=info"` | Selective verbosity per module |

---

## File 2: logging/middleware.rs — Request ID Generation

This is the simplest file in the feature — a single function that generates correlation IDs.

```rust
pub fn generate_request_id() -> String {
    Uuid::new_v4().to_string()
}
```

**UUID v4 format:** `550e8400-e29b-41d4-a716-446655440000` — 36 characters, 4 hyphens, 32 hex digits. The `4` in the third group identifies it as version 4 (randomly generated).

**Why UUID v4 and not a counter?** Counters are simpler but reset on restart, making it impossible to distinguish between "request #42 from yesterday" and "request #42 from today". UUIDs are globally unique without requiring coordination between instances.

### Unit Tests

The test module verifies three properties:

```rust
fn test_generate_request_id_format()     // Length is 36, exactly 4 hyphens
fn test_generate_request_id_uniqueness() // Two calls produce different IDs
fn test_generate_request_id_parseable()  // Output parses as valid UUID
```

The uniqueness test is important because we use `request_id` for correlation. If two requests got the same ID, their log lines would be indistinguishable.

---

## File 3: logging/fields.rs — Field Extraction Helpers

This file contains three pure functions that extract structured fields from request/response types. They're designed to be called from the completions handler after a request completes.

### extract_tokens() — Token Count Extraction

```rust
pub fn extract_tokens(response: &ChatCompletionResponse) -> (u32, u32, u32) {
    if let Some(usage) = &response.usage {
        (usage.prompt_tokens, usage.completion_tokens, usage.total_tokens)
    } else {
        (0, 0, 0)
    }
}
```

**Why return (0, 0, 0) instead of Option?** The tracing span fields `tokens_prompt`, `tokens_completion`, and `tokens_total` are numeric. Returning 0 means "usage data wasn't available" and avoids the complexity of optional span fields. Not all backends return token usage — notably, some Ollama configurations omit it.

### extract_status() — Status + Error Message

```rust
pub fn extract_status(
    result: &Result<axum::response::Response, ApiError>,
) -> (String, Option<String>) {
    match result {
        Ok(_) => ("success".to_string(), None),
        Err(e) => {
            let status = e.error.r#type.clone();
            let message = e.error.message.clone();
            (status, Some(message))
        }
    }
}
```

**Why `r#type`?** The `r#` prefix is Rust's raw identifier syntax. `type` is a reserved keyword in Rust, so we can't use it as a field name directly. `r#type` tells the compiler "treat this as an identifier, not a keyword". The `ApiError` struct uses this because it mirrors the OpenAI error format, which has a `type` field.

**The return pattern** — `(String, Option<String>)` — is a tuple where the first element is always present (status category like `"success"` or `"service_unavailable"`) and the second is only present on errors. This maps cleanly to two separate span fields: `status` and `error_message`.

### truncate_prompt() — Privacy-Safe Content Preview

```rust
pub fn truncate_prompt(
    request: &ChatCompletionRequest,
    enable_content_logging: bool,
) -> Option<String> {
    if !enable_content_logging {
        return None;  // Privacy gate: content logging is off
    }

    if let Some(first_message) = request.messages.first() {
        let content = match &first_message.content {
            MessageContent::Text { content } => content.as_str(),
            MessageContent::Parts { content: parts } => {
                // Concatenate text parts, skip image parts
                // ...
            }
        };
        return Some(truncate_string(content, 100));
    }
    None
}
```

**The privacy gate is the first line.** If `enable_content_logging` is `false` (the default), this function returns `None` immediately — no content is ever extracted, truncated, or returned. This is the "off by default, opt-in" pattern.

**Why only the first message?** Chat requests can have long conversation histories. Logging all of them would be noisy and memory-intensive. The first message is usually the system prompt or the user's initial question, which provides enough context for debugging.

**Why 100 characters?** Long enough to identify the request ("Translate this French paragraph about..."), short enough to avoid logging sensitive data. The `truncate_string` helper appends `...` when truncating.

**Handling multimodal content:** The `MessageContent::Parts` variant handles messages with mixed text and images (vision requests). Only text parts are concatenated; image URLs/base64 data are skipped entirely. This avoids accidentally logging image data.

---

## File 4: api/completions.rs — Span Instrumentation + Retry Tracking

This is the heart of the structured logging feature. The completions handler uses the `#[instrument]` macro to create a tracing span that follows the entire lifecycle of a request.

### The #[instrument] Macro — Deferred Field Recording

```rust
#[instrument(
    skip(state, headers, request),
    fields(
        request_id = tracing::field::Empty,
        model = %request.model,
        actual_model = tracing::field::Empty,
        backend = tracing::field::Empty,
        backend_type = tracing::field::Empty,
        status = tracing::field::Empty,
        status_code = tracing::field::Empty,
        error_message = tracing::field::Empty,
        latency_ms = tracing::field::Empty,
        tokens_prompt = tracing::field::Empty,
        tokens_completion = tracing::field::Empty,
        tokens_total = tracing::field::Empty,
        stream = %request.stream,
        route_reason = tracing::field::Empty,
        retry_count = 0u32,
        fallback_chain = tracing::field::Empty,
    )
)]
pub async fn handle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Response, ApiError> { /* ... */ }
```

**How `#[instrument]` works:** This is a procedural macro from the `tracing` crate. At compile time, it wraps the function body in a `tracing::span!` call. When the function is entered, a span is created and entered; when it returns, the span closes and the subscriber (the logging backend) emits the log line with all recorded fields.

**`skip(state, headers, request)`** — These parameters would normally be included in the span's debug output. We skip them because `AppState` is huge, `HeaderMap` might contain auth tokens, and `ChatCompletionRequest` might contain user messages. We manually extract only the safe fields we want.

**`tracing::field::Empty`** — This is the "record later" pattern. The field is declared in the span but has no value yet. We'll fill it in as the request progresses:

```
                    Time ─────────────────────────────────────▶

Span created:       request_id=Empty, model="gpt-4", backend=Empty, status=Empty
                    │
After ID generated: request_id="550e8400-...", model="gpt-4", backend=Empty, status=Empty
                    │
After routing:      request_id="550e8400-...", model="gpt-4", backend="ollama-local", status=Empty
                    │
After response:     request_id="550e8400-...", model="gpt-4", backend="ollama-local", status="success"
                    │
Span closes:        ──▶ Full log line emitted with ALL fields
```

**`model = %request.model`** — The `%` means "use the Display trait". This records the model name at span creation time (before the request body is consumed).

**`retry_count = 0u32`** — Unlike the `Empty` fields, this has a default value. It starts at 0 and gets updated on each retry iteration.

### Span::current().record() — Filling in Fields

Throughout the handler, fields are recorded as information becomes available:

```rust
// Step 1: Generate and record request ID
let request_id = generate_request_id();
Span::current().record("request_id", request_id.as_str());

// Step 2: After routing succeeds
Span::current().record("backend", backend.id.as_str());
Span::current().record("backend_type", format!("{:?}", backend.backend_type).as_str());
Span::current().record("route_reason", routing_result.route_reason.as_str());

// Step 3: During retry loop
Span::current().record("retry_count", attempt as u32);

// Step 4: After backend responds
Span::current().record("latency_ms", latency);
Span::current().record("status", "success");
Span::current().record("status_code", 200u16);
```

**Why `Span::current()` instead of holding a reference?** `Span::current()` returns the span that's active in the current async task. Since `#[instrument]` entered the span for us, `Span::current()` always returns the right span — even across `.await` points where the future might resume on a different thread.

### The Correlation ID Pattern

```rust
let request_id = generate_request_id();
Span::current().record("request_id", request_id.as_str());

// ... later, in the retry loop:
for attempt in 0..=max_retries {
    Span::current().record("retry_count", attempt as u32);
    // The request_id is STILL the same — it was set once, above the loop
}
```

**This is the key insight:** The `request_id` is generated **once** before the retry loop, and all retries share the same span. When you search your logs for `request_id=550e8400-...`, you'll find every log line from every retry attempt — and the `retry_count` field tells you which attempt each line is from.

### Retry Tracking and Fallback Chain

```rust
let mut fallback_chain_vec: Vec<String> = vec![];

for attempt in 0..=max_retries {
    Span::current().record("retry_count", attempt as u32);

    match proxy_request(&state, backend, &headers, &request).await {
        Ok(response) => { /* record success fields */ }
        Err(e) => {
            // Track this backend in fallback chain
            if !fallback_chain_vec.contains(&backend.id) {
                fallback_chain_vec.push(backend.id.clone());
            }
            Span::current().record("error_message", e.error.message.as_str());

            let fallback_chain_str = fallback_chain_vec.join(",");
            Span::current().record("fallback_chain", fallback_chain_str.as_str());
        }
    }
}
```

**The fallback chain builds incrementally.** Each failed backend is appended to the chain: first `"backend1"`, then `"backend1,backend2"`, etc. When you see `fallback_chain="backend1,backend2,backend3"` in a log, you know exactly which backends were tried and in what order.

**Log level progression:**
- `attempt == 0`: `info!("Trying backend")` — normal operation
- `attempt > 0`: `warn!("Retrying backend after failure")` — something went wrong
- All retries exhausted: `error!("All retry attempts exhausted")` — complete failure

---

## File 5: routing/mod.rs — Route Reason

The `route_reason` field was added to `RoutingResult` so every routing decision is self-documenting in the logs.

### RoutingResult — The Output Contract

```rust
pub struct RoutingResult {
    pub backend: Arc<Backend>,
    pub actual_model: String,
    pub fallback_used: bool,
    pub route_reason: String,  // ← NEW: explains WHY this backend was chosen
}
```

### How route_reason Is Populated

The `select_backend()` method populates `route_reason` differently based on the routing strategy and number of candidates:

```rust
let (selected, route_reason) = match self.strategy {
    RoutingStrategy::Smart => {
        let backend = self.select_smart(&candidates);
        let score = score_backend(/* ... */);
        let reason = if candidates.len() == 1 {
            "only_healthy_backend".to_string()
        } else {
            format!("highest_score:{}:{:.2}", backend.name, score)
        };
        (backend, reason)
    }
    RoutingStrategy::RoundRobin => {
        let index = (counter as usize) % candidates.len();
        let reason = if candidates.len() == 1 {
            "only_healthy_backend".to_string()
        } else {
            format!("round_robin:index_{}", index)
        };
        // ...
    }
    RoutingStrategy::PriorityOnly => {
        let reason = format!("priority:{}:{}", backend.name, backend.priority);
        // ...
    }
    RoutingStrategy::Random => {
        let reason = format!("random:{}", backend.name);
        // ...
    }
};
```

**Pattern: each strategy explains itself.** The reason string always starts with the strategy name, followed by relevant details:

| Scenario | route_reason Example |
|----------|---------------------|
| Smart routing, multiple candidates | `"highest_score:Backend 1:0.95"` |
| Round-robin, 5 candidates | `"round_robin:index_3"` |
| Priority-based | `"priority:Backend 1:10"` |
| Only one healthy backend | `"only_healthy_backend"` |
| Fallback triggered | `"fallback:gpt-4:highest_score:Backend 2:0.87"` |

**Fallback prefix:** When a fallback model is used, the reason gets wrapped with `"fallback:{original_model}:{inner_reason}"`:

```rust
route_reason = format!("fallback:{}:{}", model, route_reason);
```

This makes it immediately clear in the logs that the original model wasn't available and which model was used instead.

---

## File 6: config/logging.rs — Logging Configuration

This file defines the configuration types that control logging behavior.

### LoggingConfig

```rust
pub struct LoggingConfig {
    pub level: String,                                    // "info", "debug", "warn", etc.
    pub format: LogFormat,                                // Pretty or Json
    pub component_levels: Option<HashMap<String, String>>, // Per-module overrides
    pub enable_content_logging: bool,                      // Privacy gate (default: false)
}
```

**`component_levels`** is `Option<HashMap<...>>` rather than just `HashMap` because most users won't configure it. Using `Option` with `#[serde(skip_serializing_if = "Option::is_none")]` keeps the serialized config clean — no empty `component_levels: {}` in the output.

**`enable_content_logging`** uses `#[serde(default)]` which deserializes to `false` when the field is absent from the TOML file. This is the privacy-by-default mechanism: if you don't mention it, it's off.

### LogFormat Enum

```rust
pub enum LogFormat {
    Pretty,  // Human-readable, colored output
    Json,    // Machine-parseable JSON lines
}
```

**Pretty** is the default (good for development). **Json** is what you'd use in production with a log aggregator. The format affects how the tracing subscriber renders span fields — but the fields themselves are the same regardless of format.

### TOML Configuration Example

```toml
[logging]
level = "info"
format = "json"
enable_content_logging = false

[logging.component_levels]
routing = "debug"
api = "info"
health = "warn"
```

---

## File 7: cli/serve.rs — Tracing Initialization

The `init_tracing()` function wires everything together at startup.

```rust
pub fn init_tracing(
    config: &crate::config::LoggingConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    // Build filter directives using helper function
    let filter_str = crate::logging::build_filter_directives(config);

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&filter_str));

    // Warn if content logging is enabled
    if config.enable_content_logging {
        eprintln!("WARNING: Content logging is enabled. ...");
    }

    match config.format {
        LogFormat::Pretty => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer().pretty())
                .try_init()?;
        }
        LogFormat::Json => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer().json())
                .try_init()?;
        }
    }
    Ok(())
}
```

### Step by Step

1. **`build_filter_directives(config)`** — Produces a string like `"info,nexus::routing=debug"` from the configuration (see File 1).

2. **`EnvFilter::try_from_default_env()`** — First checks if `RUST_LOG` is set in the environment. If it is, that takes priority over the config file. This lets operators override log levels at runtime without editing config: `RUST_LOG=debug cargo run -- serve`.

3. **`unwrap_or_else(|_| EnvFilter::new(&filter_str))`** — If `RUST_LOG` isn't set, use the filter string built from config. This implements the precedence: environment variables > config file > defaults.

4. **Content logging warning** — Uses `eprintln!` (not `tracing`) because the tracing subscriber isn't initialized yet at this point. This warning appears unconditionally on stderr so operators can't miss it.

5. **Registry + Layer pattern** — `tracing_subscriber::registry()` creates a subscriber, `.with(env_filter)` adds filtering, and `.with(fmt::layer().json())` adds the output formatter. This is the layered architecture of `tracing-subscriber` — each `.with()` adds a layer that processes span events.

6. **`.try_init()`** — Registers this subscriber as the global default. Can only be called once per process (subsequent calls return an error). This is why tracing initialization happens early in `run_serve()`, before any other code emits log events.

---

## How Data Flows

Here's the complete lifecycle of a request through the structured logging system:

```
┌──────────────────────────────────────────────────────────────────────────┐
│          Step-by-Step: Request → Structured Log Entry                    │
│                                                                          │
│  ① Client sends POST /v1/chat/completions                               │
│     │                                                                    │
│  ② #[instrument] creates span with declared fields:                      │
│     │  model="gpt-4", stream=false, retry_count=0                        │
│     │  (all other fields = Empty)                                        │
│     │                                                                    │
│  ③ generate_request_id() → UUID v4                                       │
│     │  Span::current().record("request_id", "550e8400-...")              │
│     │                                                                    │
│  ④ Router selects backend via select_backend():                          │
│     │  ├─ Resolves aliases (max 3 levels)                                │
│     │  ├─ Filters by health + capability requirements                    │
│     │  ├─ Applies strategy (Smart/RoundRobin/Priority/Random)            │
│     │  └─ Returns RoutingResult { backend, route_reason }                │
│     │                                                                    │
│     │  Span::current().record("backend", "ollama-local")                 │
│     │  Span::current().record("backend_type", "Ollama")                  │
│     │  Span::current().record("route_reason", "highest_score:0.95")      │
│     │                                                                    │
│  ⑤ Retry loop (attempt 0..=max_retries):                                 │
│     │  Span::current().record("retry_count", attempt)                    │
│     │                                                                    │
│     │  proxy_request() → backend HTTP call                               │
│     │  ├─ Success:                                                       │
│     │  │  Span::current().record("status", "success")                    │
│     │  │  Span::current().record("status_code", 200)                     │
│     │  │  Span::current().record("latency_ms", elapsed)                  │
│     │  │  Span::current().record("tokens_prompt", usage.prompt_tokens)   │
│     │  │  Span::current().record("tokens_completion", ...)               │
│     │  │  └─ Return Ok(response)                                         │
│     │  │                                                                 │
│     │  └─ Failure:                                                       │
│     │     Span::current().record("error_message", e.message)             │
│     │     fallback_chain_vec.push(backend.id)                            │
│     │     Span::current().record("fallback_chain", "b1,b2")             │
│     │     └─ Continue to next attempt                                    │
│     │                                                                    │
│  ⑥ Span closes (function returns)                                        │
│     │                                                                    │
│  ⑦ Tracing subscriber formats and emits log entry:                       │
│     │                                                                    │
│     ├─ EnvFilter checks: does this span's target pass the filter?        │
│     │  (e.g., "nexus::api" at INFO level → yes)                          │
│     │                                                                    │
│     └─ fmt::Layer renders output:                                        │
│        ├─ Pretty: colored, multi-line, human-readable                    │
│        └─ Json: single-line JSON object with all fields                  │
│                                                                          │
└──────────────────────────────────────────────────────────────────────────┘
```

---

## Key Tests Explained

The test file `tests/structured_logging.rs` contains 20 tests organized into 6 modules, one per user story. Each module tests a specific aspect of the logging system.

### US1: Basic Logging (5 tests)

**`test_successful_request_produces_structured_log_fields`** (T101) — The foundation test. Creates a `ChatCompletionResponse` with known token counts, then verifies that `extract_tokens()` and `extract_status()` return the expected values:

```rust
let (prompt, completion, total) = extract_tokens(&response);
assert_eq!(prompt, 100);
assert_eq!(completion, 50);
assert_eq!(total, 150);

let (status, error_msg) = extract_status(&ok_result);
assert_eq!(status, "success");
assert!(error_msg.is_none());
```

**`test_json_format_log_entry_fields_are_valid_json`** (T102) — Constructs a JSON object from all log field values and verifies it round-trips through `serde_json` correctly. This catches type mismatches (e.g., if `latency_ms` were accidentally a string instead of a number).

**`test_request_id_is_valid_uuid_v4`** (T103) — Verifies the UUID format by checking string length (36), hyphen count (4), version nibble (`4`), and parseability via `Uuid::parse_str()`.

**`test_latency_ms_measurement`** (T104) — Sleeps for 10ms and verifies that `Instant::now().elapsed()` captures the delay accurately. This validates the timing mechanism used in the completions handler.

**`test_failed_request_produces_error_status`** (T105) — Creates an `ApiError::service_unavailable` and verifies that `extract_status()` returns a non-success status with the error message preserved.

### US2: Request Correlation (4 tests)

**`test_request_id_persists_across_retries`** (T106) — Simulates a retry loop where the same `request_id` is paired with incrementing `retry_count` values. Verifies all entries share the same ID:

```rust
let request_id = generate_request_id();
for retry_count in 0..3 {
    retry_entries.push((request_id.clone(), retry_count));
}
assert!(retry_entries.iter().all(|(id, _)| id == &request_id));
```

**`test_fallback_chain_shows_progression`** (T107) — Builds a fallback chain string by joining backend IDs with commas. Verifies the progression: `"backend1"` → `"backend1,backend2"` → `"backend1,backend2,backend3"`.

**`test_first_try_request_has_zero_retry_count`** (T108) — Ensures that a request that succeeds on the first try has `retry_count=0` and an empty `fallback_chain`.

**`test_retry_log_level_progression`** (T109) — Verifies the log level convention: INFO for the first attempt, WARN for retries, ERROR when all retries are exhausted. This matches the actual behavior in `completions.rs`.

### US3: Routing Visibility (3 tests)

These tests create real `Registry` and `Router` instances to verify that `route_reason` is populated correctly.

**`test_route_reason_score_based`** (T110) — Registers two backends with the same model, routes a request using `RoutingStrategy::Smart`, and verifies the `route_reason` starts with `"highest_score:"` or is `"only_healthy_backend"`.

**`test_route_reason_fallback_scenario`** (T111) — Registers a backend with `gpt-3.5-turbo` and configures `gpt-4 → gpt-3.5-turbo` as a fallback chain. Routes a request for `gpt-4` and verifies:

```rust
assert!(result.fallback_used);
assert_eq!(result.actual_model, "gpt-3.5-turbo");
assert!(result.route_reason.starts_with("fallback:"));
```

**`test_route_reason_single_healthy_backend`** (T112) — With only one backend available, verifies that `route_reason` is exactly `"only_healthy_backend"`. This is a special case: when there's no choice to make, the reason says so.

### US4: Privacy-Safe Logging (3 tests)

**`test_default_no_content_logging`** (T113) — Calls `truncate_prompt(&request, false)` and verifies it returns `None`. This is the most important privacy test: content must not leak when the feature is off.

**`test_startup_warning_when_content_logging_enabled`** (T114) — Creates a `LoggingConfig` with `enable_content_logging: true` and verifies the flag is set correctly. The actual `eprintln!` warning is tested implicitly through the `init_tracing()` code path.

**`test_prompt_preview_when_enabled`** (T115) — Creates a request with a 200-character message, calls `truncate_prompt(&request, true)`, and verifies the preview is at most 110 characters (100 chars + `...`):

```rust
let long_message = "A".repeat(200);
let preview = truncate_prompt(&request, true);
assert!(preview.unwrap().len() <= 110);
```

### US5: Component Log Levels (2 tests)

**`test_component_levels_builds_env_filter`** (T116) — Configures three component levels and verifies the filter string contains all of them:

```rust
let filter_str = build_filter_directives(&config);
assert!(filter_str.starts_with("info"));
assert!(filter_str.contains("nexus::routing=debug"));
assert!(filter_str.contains("nexus::api=info"));
assert!(filter_str.contains("nexus::health=warn"));
```

**`test_build_filter_directives_produces_valid_string`** (T117) — Tests both the simple case (no component levels → just `"warn"`) and the configured case (`"error,nexus::routing=trace"`). This covers the `None` branch of `component_levels`.

### US6: Aggregator Compatibility (3 tests)

**`test_numeric_fields_are_numbers_not_strings`** (T119) — Serializes token counts and latency to JSON and verifies they appear as numbers, not quoted strings:

```rust
assert!(json_str.contains("\"tokens_prompt\":100"));  // number, not "100"
assert!(json_str.contains("\"latency_ms\":123"));     // number, not "123"
```

This matters because log aggregators like Loki or Elasticsearch treat numbers and strings differently. If `latency_ms` were a string, you couldn't compute `avg(latency_ms)` in a dashboard query.

**`test_timestamp_format_rfc3339`** (T120) — Generates a timestamp with `chrono::Utc::now()`, formats it as RFC3339, and verifies it can be parsed back. RFC3339 is the standard timestamp format for JSON logs (e.g., `"2025-02-14T10:00:00Z"`).

**`test_json_schema_field_types`** (T118) — Constructs a complete log entry JSON object and verifies every field has the correct JSON type: numbers are numbers, booleans are booleans, strings are strings. This is the contract test for log aggregator compatibility.

---

## Key Rust Concepts

| Concept | What It Means | Example in This Code |
|---------|---------------|----------------------|
| `#[instrument]` | Procedural macro that wraps a function in a tracing span | `completions::handle()` — auto-creates span on entry, closes on return |
| `tracing::field::Empty` | Declares a span field with no value yet | Fields like `backend`, `status` that aren't known until later |
| `Span::current().record()` | Records a value into the current span's empty field | `Span::current().record("backend", "ollama-local")` |
| `EnvFilter` | Parses directive strings to filter log events by target/level | `"info,nexus::routing=debug"` — global INFO, routing at DEBUG |
| `tracing_subscriber::registry()` | Creates a composable subscriber with layered architecture | Combined with `fmt::layer()` for output and `EnvFilter` for filtering |
| `Uuid::new_v4()` | Generates a random UUID (128-bit, globally unique) | Request correlation IDs |
| `r#type` | Raw identifier — uses a reserved keyword as a field name | `ApiError.error.r#type` mirrors OpenAI's `type` field |
| `Option<HashMap<...>>` | Optional map — absent means "not configured" | `component_levels` — omitted in most configs |
| `#[serde(default)]` | Uses `Default::default()` when field is missing during deserialization | `enable_content_logging` defaults to `false` |
| `#[serde(skip_serializing_if)]` | Omits field from serialized output when condition is true | `component_levels` only appears in TOML if configured |

---

## Common Questions

### "What's the difference between `#[instrument]` and manually calling `tracing::info!()`?"

`#[instrument]` creates a **span** — a period of time with associated fields. All log events (`info!`, `warn!`, `error!`) emitted inside the span automatically inherit its fields. So when you `info!("Request succeeded")` inside the instrumented handler, the log line includes `request_id`, `model`, `backend`, etc. without you passing them to every `info!` call. Without `#[instrument]`, you'd need to manually include these fields in every log statement.

### "Why are span fields declared as `Empty` instead of just adding them later?"

Tracing spans have a fixed schema declared at creation time. You can't add new fields after creation — you can only fill in `Empty` slots. This is a deliberate design: it means the set of possible fields is known at compile time, which helps log aggregators build consistent schemas. If you try to `record()` a field that wasn't declared in the `#[instrument]` macro, it silently does nothing.

### "Why use `eprintln!` for the content logging warning instead of `tracing::warn!`?"

Because `init_tracing()` is the function that sets up the tracing subscriber. The warning needs to appear **before** the subscriber is initialized. If we used `tracing::warn!`, the message would be lost because no subscriber is listening yet. `eprintln!` writes directly to stderr, bypassing the tracing system entirely.

### "What happens if a field is never recorded (stays `Empty`)?"

It depends on the output format. With the JSON formatter, empty fields are omitted from the output entirely. With the pretty formatter, they may appear as `field=` with no value. This is fine — not all requests will have all fields (e.g., `tokens_prompt` is only recorded when the backend returns usage data).

### "Why not log the full request and response body?"

Privacy. Chat completion requests contain user messages — potentially containing personal information, medical records, legal documents, or proprietary code. Logging that by default would be a compliance violation (GDPR, HIPAA, etc.). The `enable_content_logging` flag exists for debugging specific issues in controlled environments, and even then, content is truncated to 100 characters.

### "How do I search for all log lines from a single request?"

If using JSON format, pipe through `jq`:
```bash
cat nexus.log | jq 'select(.fields.request_id == "550e8400-e29b-41d4-a716-446655440000")'
```

This returns every log line (including retries and warnings) from that specific request, in chronological order.
