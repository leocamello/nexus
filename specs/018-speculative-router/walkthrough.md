# F15: Speculative Router — Code Walkthrough

**Feature**: Speculative Router (F15)  
**Audience**: Junior developers joining the project  
**Last Updated**: 2025-02-17

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: routing/requirements.rs — The Request Inspector](#file-1-routingrequirementsrs--the-request-inspector)
4. [File 2: routing/reconciler/request_analyzer.rs — The Alias Resolver](#file-2-routingreconcilerrequest_analyzerrs--the-alias-resolver)
5. [File 3: routing/mod.rs — The Capability Filter](#file-3-routingmodrs--the-capability-filter)
6. [File 4: benches/routing.rs — The Performance Guardrail](#file-4-benchesroutingrs--the-performance-guardrail)
7. [Understanding the Tests](#understanding-the-tests)
8. [Key Rust Concepts](#key-rust-concepts)
9. [Common Patterns in This Codebase](#common-patterns-in-this-codebase)
10. [Next Steps](#next-steps)

---

## The Big Picture

Imagine you work at a **printing shop** with several printers. Some printers handle only black-and-white documents, some can print in color, some handle large-format posters, and some have special paper trays for envelopes. When a customer walks in, you don't ask "which printer do you want?" — you look at what they're holding, figure out what kind of job it is, and send it to the right printer automatically.

That's what the **Speculative Router** does for LLM requests. Before a request reaches any backend, the router inspects the request payload — not the prompt text, just the structure — and extracts **routing signals**: Does the request contain images? Does it need function calling? How big is the conversation history? Does the client want JSON output?

### What Problem Does This Solve?

Without F15, Nexus would send a request with an image attachment to a backend that can't process images (resulting in an error), or send a 50,000-token conversation to a backend with only an 8K context window (resulting in truncation). The user would get cryptic failures from the backend instead of a clear "no capable backend available" from Nexus.

F15 makes capability matching **automatic and structural** — the user never needs to know which backends support which features.

### How F15 Fits Into Nexus

```
┌──────────────────────────────────────────────────────────────────────────┐
│                              Nexus                                      │
│                                                                         │
│  Client Request                                                         │
│    │  POST /v1/chat/completions                                         │
│    │  {                                                                  │
│    │    "model": "gpt-4",                                                │
│    │    "messages": [{"role": "user", "content": [...image...]}],        │
│    │    "tools": [...],                                                   │
│    │    "stream": true                                                   │
│    │  }                                                                  │
│    ▼                                                                    │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │  ① RequestRequirements::from_request()              ◄── F15    │    │
│  │     Inspects request structure (NOT content):                   │    │
│  │     • Scans content parts for image_url → needs_vision=true    │    │
│  │     • Counts characters / 4 → estimated_tokens=1250            │    │
│  │     • Checks extra["tools"] → needs_tools=true                 │    │
│  │     • Checks response_format → needs_json_mode=false           │    │
│  │     • Reads stream field → prefers_streaming=true              │    │
│  └──┼──────────────────────────────────────────────────────────────┘    │
│     │                                                                   │
│     ▼                                                                   │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │  RECONCILER PIPELINE                                            │    │
│  │                                                                 │    │
│  │  ② RequestAnalyzer::reconcile()                     ◄── F15    │    │
│  │     • Resolves alias: "gpt-4" → "llama3:70b" (max 3 levels)   │    │
│  │     • Populates candidate_agents from registry                  │    │
│  │                                                                 │    │
│  │  ③ PrivacyReconciler → exclude by zone (F13)                   │    │
│  │  ④ BudgetReconciler  → exclude by cost (F14)                   │    │
│  │  ⑤ TierReconciler    → exclude by quality (F13)                │    │
│  │  ⑥ QualityReconciler → quality-based scoring                   │    │
│  │  ⑦ SchedulerReconciler → score & select (uses requirements)   │    │
│  │                                                                 │    │
│  │  Result: Route | Queue | Reject                                 │    │
│  └──┼──────────────────────────────────────────────────────────────┘    │
│     │                                                                   │
│     ▼                                                                   │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │  Router::filter_candidates()                        ◄── F15    │    │
│  │     Applies requirements as hard filters:                       │    │
│  │     • needs_vision=true? Remove non-vision backends             │    │
│  │     • needs_tools=true? Remove non-tool backends                │    │
│  │     • estimated_tokens > context_length? Remove backend         │    │
│  │     • needs_json_mode=true? Remove non-JSON backends            │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                                                         │
│  Data Flow: Request JSON → RequestRequirements (struct)                 │
│             → RoutingIntent (shared pipeline state)                     │
│             → RequestAnalyzer (alias + candidates)                      │
│             → filter_candidates (capability matching)                   │
│             → SchedulerReconciler (scoring + selection)                  │
└──────────────────────────────────────────────────────────────────────────┘
```

### Key Design Decisions

| Decision | Why |
|----------|-----|
| Inspect structure, not content | Constitution: zero ML, sub-millisecond. We check `type == "image_url"`, not "does this prompt discuss images?" |
| Token estimate = chars/4 | Simple heuristic that's fast and good enough. Exact tokenization (tiktoken) is used later by F14 for cost estimation |
| `extra` field for tools/response_format | OpenAI's API has many optional fields; `extra: HashMap<String, Value>` catches them all without listing every one |
| `prefers_streaming` is a hint, not a filter | All backends should support streaming; this field exists for future scheduler optimizations |
| Alias resolution capped at 3 levels | Prevents infinite loops from circular alias chains (a→b→c→a) |
| `filter_candidates()` is separate from reconciler | The reconciler populates candidates; filtering happens during scoring. Separation of concerns |

---

## File Structure

```
src/routing/
├── requirements.rs              ← F15: RequestRequirements struct + from_request() (249 lines, 6 tests)
├── reconciler/
│   └── request_analyzer.rs      ← F15: RequestAnalyzer reconciler (255 lines, 5 tests)
├── mod.rs                       ← F15: filter_candidates() method (1834 lines, 8 filter tests)

benches/
└── routing.rs                   ← F15: Performance benchmarks (419 lines, 8 benchmarks)
```

**F15 Contribution**: 1 new file (`requirements.rs`), 1 new reconciler (`request_analyzer.rs`), 1 modified file (`mod.rs`), 1 benchmark file. ~550 lines added, 19 unit tests, 8 benchmarks.

---

## File 1: routing/requirements.rs — The Request Inspector

**Purpose**: Extract routing-relevant signals from an incoming `ChatCompletionRequest` without analyzing prompt content.  
**Lines**: 249  |  **Tests**: 6  |  **Status**: NEW

### Why Does This Exist?

When a request arrives at Nexus, we need to answer questions like "does this request need a vision-capable model?" before we can pick a backend. The `RequestRequirements` struct captures all these signals in one place.

The critical design constraint: **zero ML, sub-millisecond decisions**. We only look at the JSON structure (field names, types, array contents), never at the actual prompt text.

### The Struct

```rust
// src/routing/requirements.rs

/// Requirements extracted from an incoming request
#[derive(Debug, Clone, PartialEq)]
pub struct RequestRequirements {
    pub model: String,              // The model name from the request
    pub estimated_tokens: u32,      // Character count / 4 (heuristic)
    pub needs_vision: bool,         // Contains image_url content parts?
    pub needs_tools: bool,          // Has "tools" in extra fields?
    pub needs_json_mode: bool,      // Has response_format.type == "json_object"?
    pub prefers_streaming: bool,    // stream: true?
}
```

Each field answers one routing question. Downstream components (the reconciler pipeline, the capability filter) read these flags to decide which backends are eligible.

### The `from_request()` Method

This is the heart of F15 — a single-pass scan of the request that extracts all routing signals:

```rust
pub fn from_request(request: &ChatCompletionRequest) -> Self {
    let model = request.model.clone();

    let mut estimated_tokens = 0;
    let mut needs_vision = false;

    for message in &request.messages {
        match &message.content {
            MessageContent::Text { content } => {
                // Simple text: count characters, divide by 4
                estimated_tokens += content.len() as u32 / 4;
            }
            MessageContent::Parts { content } => {
                for part in content {
                    if part.part_type == "text" {
                        if let Some(text) = &part.text {
                            estimated_tokens += text.len() as u32 / 4;
                        }
                    } else if part.part_type == "image_url" {
                        needs_vision = true;  // ◄── One image = vision required
                    }
                }
            }
        }
    }

    // Check for tools in extra fields
    let needs_tools = request.extra.contains_key("tools");

    // Check for JSON mode: response_format.type == "json_object"
    let needs_json_mode = request
        .extra
        .get("response_format")
        .and_then(|v| v.as_object())
        .and_then(|obj| obj.get("type"))
        .and_then(|v| v.as_str())
        .map(|t| t == "json_object")
        .unwrap_or(false);

    let prefers_streaming = request.stream;

    Self { model, estimated_tokens, needs_vision, needs_tools, needs_json_mode, prefers_streaming }
}
```

Let's trace through each extraction:

1. **Token estimation**: Iterates all messages, counts total characters across text content, divides by 4. This `chars/4` heuristic approximates tokenization without loading any tokenizer. For a 4000-character conversation, this yields ~1000 estimated tokens.

2. **Vision detection**: During the same loop, checks if any content part has `part_type == "image_url"`. OpenAI's multimodal API uses `content: [{type: "text", ...}, {type: "image_url", ...}]` to send images. One image anywhere triggers the flag.

3. **Tools detection**: Checks `request.extra.contains_key("tools")`. The `extra` field is a `HashMap<String, serde_json::Value>` that captures any JSON fields not explicitly modeled in the `ChatCompletionRequest` struct. Presence of the key is enough — we don't need to parse the tool definitions.

4. **JSON mode detection**: Navigates `response_format → type → "json_object"` using a chain of `and_then()` calls. Each step safely handles missing or wrong-typed values without panicking.

5. **Streaming preference**: Reads the `stream` boolean directly from the request struct.

### Key Tests

```rust
#[test]
fn extracts_model_name() {
    let request = create_simple_request("llama3:8b", "Hello");
    let requirements = RequestRequirements::from_request(&request);
    assert_eq!(requirements.model, "llama3:8b");
}

#[test]
fn estimates_tokens_from_content() {
    let content = "a".repeat(1000);
    let request = create_simple_request("llama3:8b", &content);
    let requirements = RequestRequirements::from_request(&request);
    assert!(requirements.estimated_tokens >= 250); // 1000 chars / 4
}

#[test]
fn detects_vision_requirement() {
    let request = create_vision_request("llava", "http://example.com/image.jpg");
    let requirements = RequestRequirements::from_request(&request);
    assert!(requirements.needs_vision);
}

#[test]
fn simple_request_has_no_special_requirements() {
    let request = create_simple_request("llama3:8b", "Hello");
    let requirements = RequestRequirements::from_request(&request);
    assert!(!requirements.needs_vision);
    assert!(!requirements.needs_tools);
    assert!(!requirements.needs_json_mode);
    // ◄── Zero false positives: plain text triggers nothing
}
```

The tests follow a pattern: create a specific request type with a helper function, extract requirements, and verify the right flags are set. The `simple_request_has_no_special_requirements` test is especially important — it verifies no false positives. A plain text request should not accidentally trigger vision or tools requirements.

---

## File 2: routing/reconciler/request_analyzer.rs — The Alias Resolver

**Purpose**: The first reconciler in the pipeline — resolves model aliases and populates the initial candidate list from the registry.  
**Lines**: 255  |  **Tests**: 5  |  **Status**: NEW

### Why Does This Exist?

Users might configure aliases like `"gpt-4" → "llama3:70b"` so they can use familiar names. Before we can look up which backends serve a model, we need to resolve the alias to the actual model name. The `RequestAnalyzer` does two things:

1. **Alias resolution**: Converts `"gpt-4"` → `"llama3:70b"` (supports up to 3 levels of chaining)
2. **Candidate population**: Queries the registry for all backends that serve the resolved model

### The Struct

```rust
const MAX_ALIAS_DEPTH: usize = 3;

pub struct RequestAnalyzer {
    model_aliases: HashMap<String, String>,  // alias → target
    registry: Arc<Registry>,                  // backend/model registry
}
```

### Alias Resolution

```rust
fn resolve_alias(&self, model: &str) -> String {
    let mut current = model.to_string();
    let mut depth = 0;

    while depth < MAX_ALIAS_DEPTH {
        match self.model_aliases.get(&current) {
            Some(target) => {
                tracing::debug!(from = %current, to = %target, depth = depth + 1,
                    "RequestAnalyzer: resolved alias");
                current = target.clone();
                depth += 1;
            }
            None => break,  // ◄── Not an alias, stop here
        }
    }

    current
}
```

This is a simple loop: look up the current name in the alias map, replace if found, repeat up to 3 times. The depth limit prevents infinite loops from circular aliases (a→b→c→a).

Example chain: `"gpt" → "gpt-4" → "llama3:70b"` (2 levels, within limit).

### The Reconciler Implementation

```rust
impl Reconciler for RequestAnalyzer {
    fn name(&self) -> &'static str { "RequestAnalyzer" }

    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        // Step 1: Resolve aliases
        let resolved = self.resolve_alias(&intent.requested_model);
        intent.resolved_model = resolved.clone();

        // Step 2: Populate candidates from registry
        let backends = self.registry.get_backends_for_model(&resolved);
        intent.candidate_agents = backends.iter().map(|b| b.id.clone()).collect();

        Ok(())
    }
}
```

The `Reconciler` trait requires two methods: `name()` (for logging and rejection reasons) and `reconcile()` (the actual work). The `RequestAnalyzer` writes to two fields on `RoutingIntent`:
- `resolved_model`: The actual model name after alias resolution
- `candidate_agents`: A `Vec<String>` of backend IDs that serve this model

Later reconcilers in the pipeline (Privacy, Budget, Tier) will narrow this candidate list by removing backends that don't meet their criteria.

### Key Tests

```rust
#[test]
fn resolves_single_alias() {
    // "gpt-4" → "llama3:70b", backend b1 serves llama3:70b
    let analyzer = RequestAnalyzer::new(aliases, registry);
    let mut intent = RoutingIntent::new(/* model: "gpt-4" */);
    analyzer.reconcile(&mut intent).unwrap();
    assert_eq!(intent.resolved_model, "llama3:70b");
    assert_eq!(intent.candidate_agents, vec!["b1"]);
}

#[test]
fn resolves_chained_aliases_max_3() {
    // a → b → c → d → e (4 levels)
    // But MAX_ALIAS_DEPTH=3, so stops at d
    analyzer.reconcile(&mut intent).unwrap();
    assert_eq!(intent.resolved_model, "d");  // NOT "e"
}

#[test]
fn populates_all_backend_ids_for_model() {
    // Two backends (b1, b2) both serve llama3:8b
    analyzer.reconcile(&mut intent).unwrap();
    assert_eq!(intent.candidate_agents.len(), 2);
}

#[test]
fn no_alias_passes_through() {
    // No aliases configured — model name passes through unchanged
    assert_eq!(intent.resolved_model, "llama3:8b");
}

#[test]
fn empty_candidates_for_unknown_model() {
    // No backends serve "nonexistent" — empty candidate list (not an error)
    assert!(intent.candidate_agents.is_empty());
}
```

Notice that `empty_candidates_for_unknown_model` returns `Ok(())` with an empty list, not an error. The pipeline lets the `SchedulerReconciler` handle the "no candidates" case and produce the appropriate rejection.

---

## File 3: routing/mod.rs — The Capability Filter

**Purpose**: The `Router` struct's `filter_candidates()` method applies `RequestRequirements` as hard filters, removing backends that lack required capabilities.  
**Lines**: 1834 total (F15 adds `filter_candidates()` at lines 593–632 and 8 tests)  
**Tests**: 8 in `filter_tests` module

### Why Does This Exist?

The `RequestAnalyzer` populates candidates based on model name only — "which backends serve `llama3:8b`?" But not all backends serving `llama3:8b` may support vision, tools, or have large enough context windows. The `filter_candidates()` method is the hard filter that ensures only **capable** backends remain.

### The Filter Method

```rust
fn filter_candidates(&self, model: &str, requirements: &RequestRequirements) -> Vec<Backend> {
    // Step 1: Get all backends that have this model
    let mut candidates = self.registry.get_backends_for_model(model);

    // Step 2: Remove unhealthy backends
    candidates.retain(|backend| backend.status == BackendStatus::Healthy);

    // Step 3: Check each backend's model capabilities against requirements
    candidates.retain(|backend| {
        if let Some(model_info) = backend.models.iter().find(|m| m.id == model) {
            // Vision check
            if requirements.needs_vision && !model_info.supports_vision {
                return false;
            }
            // Tools check
            if requirements.needs_tools && !model_info.supports_tools {
                return false;
            }
            // JSON mode check
            if requirements.needs_json_mode && !model_info.supports_json_mode {
                return false;
            }
            // Context length check
            if requirements.estimated_tokens > model_info.context_length {
                return false;
            }
            true
        } else {
            false
        }
    });

    candidates
}
```

The method uses `Vec::retain()` — Rust's in-place filter that removes elements where the closure returns `false`. Three filtering stages:

1. **Health filter**: Only `BackendStatus::Healthy` backends pass. Unhealthy backends are excluded regardless of capability.

2. **Capability filter**: For each backend, finds the matching model (by ID) and checks:
   - `needs_vision` → `supports_vision` must be true
   - `needs_tools` → `supports_tools` must be true
   - `needs_json_mode` → `supports_json_mode` must be true
   - `estimated_tokens` → must not exceed `context_length`

3. **Safety net**: If the model isn't found in the backend's model list (shouldn't happen, but defensive), the backend is excluded.

Each check is a **negative filter** — the requirement flag is only checked when it's `true`. A request with `needs_vision: false` skips the vision check entirely, so all backends pass through regardless of their vision support.

### How This Connects to the Pipeline

The `filter_candidates()` method is available to the Router for direct use, but in the reconciler pipeline architecture, capability filtering happens within the `SchedulerReconciler`. The pipeline flow is:

```
select_backend(requirements)
  │
  ├─ resolve_alias("gpt-4") → "llama3:70b"
  │
  ├─ run_pipeline_for_model(requirements, "llama3:70b")
  │   │
  │   ├─ RequestAnalyzer → populate candidates
  │   ├─ PrivacyReconciler → filter by zone
  │   ├─ BudgetReconciler → filter by cost
  │   ├─ TierReconciler → filter by quality tier
  │   ├─ QualityReconciler → quality-based scoring
  │   └─ SchedulerReconciler → score remaining candidates → Route
  │
  ├─ If pipeline rejects, try fallback chain
  │
  └─ Return RoutingResult or RoutingError
```

### Key Tests

```rust
#[test]
fn filters_by_model_name() {
    // Backend A has llama3:8b, Backend B has mistral:7b
    // Request for llama3:8b → only Backend A in candidates
    let candidates = router.filter_candidates("llama3:8b", &requirements);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "Backend A");
}

#[test]
fn filters_out_unhealthy_backends() {
    // Backend A: Healthy, Backend B: Unhealthy (both serve llama3:8b)
    let candidates = router.filter_candidates("llama3:8b", &requirements);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "Backend A");
}

#[test]
fn filters_by_vision_capability() {
    // Backend A: no vision, Backend B: vision capable
    // Request needs_vision=true → only Backend B
    let candidates = router.filter_candidates("llama3:8b", &requirements);
    assert_eq!(candidates.len(), 1);
    assert!(candidates[0].models[0].supports_vision);
}

#[test]
fn filters_by_context_length() {
    // Backend A: 4096 context, Backend B: 128000 context
    // Request estimated_tokens=10000 → only Backend B
    let candidates = router.filter_candidates("llama3:8b", &requirements);
    assert_eq!(candidates.len(), 1);
    assert!(candidates[0].models[0].context_length >= 10000);
}

#[test]
fn filters_by_tools_capability() {
    // Backend A: no tools, Backend B: supports tools
    // Request needs_tools=true → only Backend B
    let candidates = router.filter_candidates("llama3:8b", &requirements);
    assert_eq!(candidates.len(), 1);
    assert!(candidates[0].models[0].supports_tools);
}

#[test]
fn filters_by_json_mode_capability() {
    // Backend A: no JSON mode, Backend B: supports JSON mode
    // Request needs_json_mode=true → only Backend B
    assert_eq!(candidates[0].name, "Backend B");
}

#[test]
fn filters_by_multiple_capabilities() {
    // Backend A: basic (4096, no vision, no tools)
    // Backend B: full featured (128K, vision, tools, JSON mode)
    // Request needs ALL → only Backend B survives
    let requirements = RequestRequirements {
        estimated_tokens: 50000,
        needs_vision: true,
        needs_tools: true,
        needs_json_mode: true,
        ..
    };
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "Backend B");
}

#[test]
fn returns_empty_when_no_match() {
    // Request for "nonexistent" model → empty Vec (not an error)
    let candidates = router.filter_candidates("nonexistent", &requirements);
    assert!(candidates.is_empty());
}
```

The `filters_by_multiple_capabilities` test is the most important — it verifies that all filters compose correctly. A request needing vision + tools + JSON mode + large context should only match backends that satisfy **all** of those simultaneously.

---

## File 4: benches/routing.rs — The Performance Guardrail

**Purpose**: Criterion benchmarks that validate routing decisions meet the constitution's latency requirements: routing decision < 1ms, request analysis < 0.5ms.  
**Lines**: 419  |  **Benchmarks**: 8

### Why Does This Exist?

Nexus adds overhead to every LLM request. The constitution mandates that routing overhead must be negligible — under 1ms for the full pipeline. Without benchmarks, a well-intentioned code change could accidentally make routing 10x slower, violating the performance contract.

### The Benchmarks

The file defines helper functions that create realistic test setups (multiple backends with varying models, capabilities, and load states), then measures how long routing takes:

```rust
fn create_backend(id: usize, model_count: usize) -> Backend {
    let models: Vec<Model> = (0..model_count)
        .map(|m| Model {
            id: format!("model-{}", m),
            context_length: 4096 + (m * 1024) as u32,
            supports_vision: m % 3 == 0,    // ~1/3 of models support vision
            supports_tools: m % 2 == 0,     // ~1/2 support tools
            supports_json_mode: m % 4 == 0, // ~1/4 support JSON mode
            ..
        })
        .collect();
    // ... priority, latency, pending requests vary by id
}
```

| Benchmark | What It Measures | Target |
|-----------|-----------------|--------|
| `bench_smart_routing_by_backend_count` | Smart strategy routing with 1, 5, 10, 25, 50 backends | < 1ms |
| `bench_round_robin_routing` | Round-robin routing (should be O(1)) | < 1ms |
| `bench_capability_filtered_routing` | Routing with `needs_vision=true` across 25 backends | < 1ms |
| `bench_routing_with_fallback` | Routing with fallback chain resolution | < 1ms |
| `bench_routing_with_alias` | Alias resolution + routing | < 1ms |
| `bench_full_pipeline` | Full reconciler pipeline (all 6 reconcilers) with 5-50 backends | < 1ms p95 |
| `bench_request_analyzer` | RequestAnalyzer alone (alias resolution + candidate population) | < 0.5ms |
| `bench_tokenizer_counting` | Token counting across different tokenizer tiers | < 200ms p95 |

The capability filtering benchmark is F15-specific:

```rust
fn bench_capability_filtered_routing(c: &mut Criterion) {
    let router = create_router(25, 5);  // 25 backends, 5 models each
    let requirements = RequestRequirements {
        model: "model-0".to_string(),
        needs_vision: true,          // ◄── Only ~1/3 of backends pass
        needs_tools: false,
        needs_json_mode: false,
        estimated_tokens: 100,
        prefers_streaming: false,
    };

    c.bench_function("capability_filtered_25_backends", |b| {
        b.iter(|| {
            black_box(router.select_backend(&requirements, None).unwrap());
        });
    });
}
```

`black_box()` prevents the compiler from optimizing away the result (since it's not used in the benchmark loop). This is a standard Criterion pattern.

### Running the Benchmarks

```bash
cargo bench                              # Run all benchmarks
cargo bench -- smart_routing             # Run only smart routing benchmarks
cargo bench -- request_analyzer          # Run only analyzer benchmarks
```

Criterion generates HTML reports in `target/criterion/` with statistical analysis including mean, standard deviation, and comparison with previous runs.

---

## Understanding the Tests

### Test Helpers

All test modules use helper functions to create test fixtures. This pattern keeps individual tests short and focused:

```rust
// In requirements.rs — create a request with specific characteristics
fn create_simple_request(model: &str, content: &str) -> ChatCompletionRequest { ... }
fn create_vision_request(model: &str, image_url: &str) -> ChatCompletionRequest { ... }
fn create_tools_request(model: &str) -> ChatCompletionRequest { ... }
fn create_json_mode_request(model: &str) -> ChatCompletionRequest { ... }

// In request_analyzer.rs — create a backend with specific model
fn create_test_backend(id: &str, model_id: &str) -> Backend { ... }
fn create_requirements(model: &str) -> RequestRequirements { ... }

// In mod.rs — create a backend with specific capabilities
fn create_test_model(id: &str, context_length: u32, vision: bool, tools: bool) -> Model { ... }
fn create_test_backend(id: &str, name: &str, status: BackendStatus, models: Vec<Model>) -> Backend { ... }
fn create_test_router(backends: Vec<Backend>) -> Router { ... }
```

### Test Organization

| Module | File | Test Count | What It Covers |
|--------|------|------------|----------------|
| `requirements::tests` | `requirements.rs` | 6 | Requirements extraction from various request types |
| `request_analyzer::tests` | `request_analyzer.rs` | 5 | Alias resolution and candidate population |
| `filter_tests` | `mod.rs` | 8 | Capability filtering across all dimensions |

### Testing Patterns

**Pattern 1: Setup → Extract → Assert**

```rust
#[test]
fn detects_tools_requirement() {
    // Setup: build a request with tools field
    let request = create_tools_request("llama3:8b");
    // Extract: run the function under test
    let requirements = RequestRequirements::from_request(&request);
    // Assert: verify the expected flag
    assert!(requirements.needs_tools);
}
```

**Pattern 2: Two-Backend Filtering**

The capability filter tests consistently use two backends — one that should pass and one that should be filtered out:

```rust
#[test]
fn filters_by_vision_capability() {
    let backends = vec![
        // Backend A: does NOT support vision
        create_test_backend("a", "A", Healthy, vec![model(vision: false)]),
        // Backend B: DOES support vision
        create_test_backend("b", "B", Healthy, vec![model(vision: true)]),
    ];
    let requirements = RequestRequirements { needs_vision: true, .. };

    let candidates = router.filter_candidates("llama3:8b", &requirements);
    assert_eq!(candidates.len(), 1);  // Only B survives
    assert!(candidates[0].models[0].supports_vision);
}
```

This pattern makes it crystal clear which backend was filtered and why.

**Pattern 3: Reconciler Mutation Testing**

```rust
#[test]
fn resolves_single_alias() {
    // Create intent with unresolved model name
    let mut intent = RoutingIntent::new(/* model: "gpt-4" */);

    // Run reconciler — it mutates the intent in place
    analyzer.reconcile(&mut intent).unwrap();

    // Verify the intent was correctly mutated
    assert_eq!(intent.resolved_model, "llama3:70b");
    assert_eq!(intent.candidate_agents, vec!["b1"]);
}
```

Reconcilers take `&mut RoutingIntent` and modify it. Tests verify the mutations happened correctly by asserting on the intent's fields after `reconcile()` returns.

---

## Key Rust Concepts

### 1. `Vec::retain()` for In-Place Filtering

```rust
candidates.retain(|backend| backend.status == BackendStatus::Healthy);
```

`retain()` keeps elements where the closure returns `true` and removes the rest, modifying the `Vec` in place. This is more efficient than `.filter().collect()` because it avoids allocating a new `Vec`. You'll see this pattern everywhere in the filtering code.

### 2. `HashMap<String, serde_json::Value>` as a Catch-All

```rust
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
    // ... explicitly modeled fields ...

    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,  // ◄── catches everything else
}
```

`#[serde(flatten)]` tells serde to put any JSON fields that don't match explicit struct fields into this `HashMap`. This lets us handle `tools`, `response_format`, and any future OpenAI API additions without updating the struct for each one. The cost: you have to navigate the `Value` type with `as_object()`, `as_str()`, etc.

### 3. `and_then()` Chains for Safe Navigation

```rust
let needs_json_mode = request
    .extra
    .get("response_format")           // Option<&Value>
    .and_then(|v| v.as_object())      // Option<&Map>
    .and_then(|obj| obj.get("type"))  // Option<&Value>
    .and_then(|v| v.as_str())         // Option<&str>
    .map(|t| t == "json_object")      // Option<bool>
    .unwrap_or(false);                // bool (default: false)
```

This chain safely navigates nested JSON structure. If any step returns `None` (field missing, wrong type), the whole chain short-circuits to `None`, and `unwrap_or(false)` provides the default. This is Rust's equivalent of optional chaining (`?.`) in other languages.

### 4. `enum` with Data for Message Content

```rust
pub enum MessageContent {
    Text { content: String },
    Parts { content: Vec<ContentPart> },
}
```

OpenAI's API supports two formats for message content: a plain string or an array of parts (text + images). Rust's enum naturally models this — `from_request()` uses `match` to handle both variants, extracting text length from both and detecting images only in the `Parts` variant.

### 5. `#[derive(Clone, PartialEq)]` for Test Assertions

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct RequestRequirements { ... }
```

`PartialEq` enables `assert_eq!()` comparisons in tests. `Clone` is needed because `RequestRequirements` is stored in `RoutingIntent` (which needs to clone it when constructing intents for fallback models). `Debug` enables `{:?}` formatting in assertion failure messages.

---

## Common Patterns in This Codebase

### 1. The Inspect → Signal → Filter Pattern

F15 follows a clean three-stage pipeline:

```
Request JSON
  │
  ▼
RequestRequirements::from_request()     ← INSPECT: scan structure, extract signals
  │
  ▼
RequestRequirements {
    needs_vision: true,                 ← SIGNAL: boolean flags summarize findings
    estimated_tokens: 1250,
    ...
}
  │
  ▼
filter_candidates(model, &requirements) ← FILTER: remove incapable backends
  │
  ▼
Vec<Backend>                            ← Only capable backends remain
```

This separation means the inspection logic doesn't need to know about backends, and the filtering logic doesn't need to know about request parsing. Each stage is independently testable.

### 2. The Reconciler Pipeline Pattern

Every reconciler follows the same contract:

```rust
impl Reconciler for RequestAnalyzer {
    fn name(&self) -> &'static str { "RequestAnalyzer" }

    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        // 1. Read from intent (requested_model, requirements, etc.)
        // 2. Do your work (resolve aliases, query registry)
        // 3. Write results back to intent (resolved_model, candidate_agents)
    }
}
```

Reconcilers don't know about each other. They all read from and write to the shared `RoutingIntent` struct. The pipeline runs them in order: RequestAnalyzer → Privacy → Budget → Tier → Quality → Scheduler.

### 3. The Negative Filter Pattern

Capability checks use negative logic — only check when the requirement is active:

```rust
// Only filters when requirement is true
if requirements.needs_vision && !model_info.supports_vision {
    return false;  // Exclude this backend
}
```

When `needs_vision` is `false`, this entire check is skipped via short-circuit evaluation. This means a plain text request flows through all backends without any capability filtering — no false positives.

### 4. The Test Helper Factory Pattern

Each test module defines small factory functions for creating test data. This keeps tests concise and makes the setup assumptions visible:

```rust
fn create_test_backend(id: &str, model_id: &str) -> Backend {
    Backend {
        id: id.to_string(),
        status: BackendStatus::Healthy,
        models: vec![Model { id: model_id.to_string(), context_length: 4096, ... }],
        priority: 1,
        pending_requests: AtomicU32::new(0),
        ...
    }
}
```

When a test needs non-default values (unhealthy status, large context), it either uses a more configurable factory or constructs the backend directly.

---

## Next Steps

After understanding F15, here's what to explore next:

1. **F13: Privacy Zones & Capability Tiers** — The PrivacyReconciler and TierReconciler that run after RequestAnalyzer (see `specs/015-privacy-zones-capability-tiers/walkthrough.md`)
2. **F14: Inference Budget Management** — The BudgetReconciler that uses token estimates from F15 for cost tracking (see `specs/016-inference-budget-mgmt/walkthrough.md`)
3. **Scoring & Selection** — The `SchedulerReconciler` in `src/routing/reconciler/scheduler.rs` that scores the filtered candidates and picks the winner
4. **Try it yourself**: Add a backend with `supports_vision: false` to a test registry, send a vision request, and verify it gets filtered out

### Questions to Investigate

- What happens if `estimated_tokens` is exactly equal to `context_length`? (Hint: look at the `>` vs `>=` in `filter_candidates`)
- Why does `needs_tools` check for key presence instead of parsing the tools array? (Hint: presence is enough to know the client expects tool support)
- How would you add a new requirement, say `needs_audio`? (Hint: add a field to `RequestRequirements`, detect it in `from_request()`, add a filter check in `filter_candidates()`, and add a `supports_audio` field to `Model`)
