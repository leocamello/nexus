# Intelligent Router - Code Walkthrough

**Feature**: F06 - Intelligent Router  
**Audience**: Junior developers joining the project  
**Last Updated**: 2026-02-08

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: mod.rs - The Router Brain](#file-1-modrs---the-router-brain)
4. [File 2: error.rs - What Can Go Wrong](#file-2-errorrs---what-can-go-wrong)
5. [File 3: scoring.rs - The Scoring Algorithm](#file-3-scoringrs---the-scoring-algorithm)
6. [File 4: strategies.rs - Strategy Definitions](#file-4-strategiesrs---strategy-definitions)
7. [File 5: requirements.rs - Request Analysis](#file-5-requirementsrs---request-analysis)
8. [File 6: config/routing.rs - Configuration](#file-6-configroutingrs---configuration)
9. [Understanding the Tests](#understanding-the-tests)
10. [Key Rust Concepts](#key-rust-concepts)
11. [Common Patterns in This Codebase](#common-patterns-in-this-codebase)
12. [Next Steps](#next-steps)

---

## The Big Picture

Think of the Intelligent Router as a **smart traffic controller for AI requests**. When a client asks "chat with llama3", the router needs to decide:
- Which backends have that model?
- Are they healthy and available?
- Do they support the needed capabilities (vision, tools)?
- Which one is the best choice right now?

### Why Do We Need Intelligent Routing?

Simple load balancing isn't enough because:
1. **Different backends host different models** - Backend A has `llama3:8b`, Backend B has `mistral:7b`
2. **Capabilities vary** - Some models support vision, others don't
3. **Load changes constantly** - A backend busy with 50 requests shouldn't get more
4. **Response times matter** - Prefer backends with faster historical latency

### How It Fits in Nexus

```
┌─────────────────────────────────────────────────────────────────┐
│                         Nexus                                   │
│                                                                 │
│  ┌──────────┐     ┌──────────────────┐     ┌──────────────┐    │
│  │   API    │────▶│ Intelligent Router│────▶│   Registry   │    │
│  │ Gateway  │     │ (you are here!)   │     │              │    │
│  └──────────┘     └──────────────────┘     └──────────────┘    │
│       │                   │                        │            │
│       │                   │                        │            │
│       │                   ▼                        ▼            │
│       │           ┌──────────────┐         ┌────────────┐       │
│       │           │   Scoring    │         │  Health    │       │
│       │           │   Engine     │         │  Checker   │       │
│       └───────────┤              │         │            │       │
│                   └──────────────┘         └────────────┘       │
└─────────────────────────────────────────────────────────────────┘
                            │
            ┌───────────────┼───────────────┐
            ▼               ▼               ▼
     ┌──────────┐    ┌──────────┐    ┌──────────┐
     │ Ollama 1 │    │ Ollama 2 │    │  vLLM    │
     └──────────┘    └──────────┘    └──────────┘
```

### The Routing Pipeline

```
                          REQUEST FLOW
    ┌─────────────────────────────────────────────────────────┐
    │                                                         │
    │   1. EXTRACT         2. FILTER           3. SELECT      │
    │   ───────────       ──────────          ──────────      │
    │                                                         │
    │   ┌─────────┐       ┌─────────┐        ┌─────────┐      │
    │   │ Request │──────▶│  All    │───────▶│ Best    │      │
    │   │  Needs  │       │Candidates│       │ Backend │      │
    │   └─────────┘       └─────────┘        └─────────┘      │
    │                                                         │
    │   • Model name       • Has model?       • Smart score   │
    │   • Context size     • Is healthy?      • Round-robin   │
    │   • Needs vision?    • Has capacity?    • Priority      │
    │   • Needs tools?     • Supports caps?   • Random        │
    │                                                         │
    └─────────────────────────────────────────────────────────┘
```

### Key Architectural Insight

The router **never makes network calls**. It only reads from the in-memory Registry (kept up-to-date by Health Checker). This guarantees routing decisions complete in **< 1ms**.

---

## File Structure

```
src/
├── routing/
│   ├── mod.rs              # Router struct, select_backend(), filter_candidates()
│   ├── error.rs            # RoutingError enum
│   ├── scoring.rs          # ScoringWeights, score_backend()
│   ├── strategies.rs       # RoutingStrategy enum
│   └── requirements.rs     # RequestRequirements extraction
└── config/
    └── routing.rs          # RoutingConfig, RoutingWeights

tests/
└── routing_integration.rs  # End-to-end routing tests
```

---

## File 1: mod.rs - The Router Brain

This is the main file containing the `Router` struct and all selection logic.

### The Router Struct

```rust
pub struct Router {
    /// Reference to backend registry
    registry: Arc<Registry>,           // Shared access to backend data

    /// Routing strategy to use
    strategy: RoutingStrategy,          // smart, round_robin, etc.

    /// Scoring weights for smart strategy
    weights: ScoringWeights,            // priority=50, load=30, latency=20

    /// Model aliases (alias → target)
    aliases: HashMap<String, String>,   // e.g., "gpt-4" → "llama3:70b"

    /// Fallback chains (model → [fallback1, fallback2, ...])
    fallbacks: HashMap<String, Vec<String>>,

    /// Round-robin counter for round-robin strategy
    round_robin_counter: AtomicU64,     // Thread-safe counter
}
```

**Breaking it down:**

| Field | Type | Purpose |
|-------|------|---------|
| `registry` | `Arc<Registry>` | Shared access to backend data |
| `strategy` | `RoutingStrategy` | How to select from candidates |
| `weights` | `ScoringWeights` | Weights for smart scoring |
| `aliases` | `HashMap` | Model name mappings |
| `fallbacks` | `HashMap` | Backup models when primary unavailable |
| `round_robin_counter` | `AtomicU64` | Thread-safe counter for rotation |

### The select_backend Method

This is the core algorithm - the "brain" of routing:

```rust
pub fn select_backend(
    &self,
    requirements: &RequestRequirements,
) -> Result<RoutingResult, RoutingError> {
    // Step 1: Resolve alias (e.g., "gpt-4" → "llama3:70b")
    let model = self.resolve_alias(&requirements.model);

    // Step 2: Check if model exists anywhere
    let all_backends = self.registry.get_backends_for_model(&model);
    let model_exists = !all_backends.is_empty();

    // Step 3: Filter to healthy backends with required capabilities
    let candidates = self.filter_candidates(&model, requirements);

    if !candidates.is_empty() {
        // Step 4: Apply routing strategy to pick the best
        let selected = match self.strategy {
            RoutingStrategy::Smart => self.select_smart(&candidates),
            RoutingStrategy::RoundRobin => self.select_round_robin(&candidates),
            RoutingStrategy::PriorityOnly => self.select_priority_only(&candidates),
            RoutingStrategy::Random => self.select_random(&candidates),
        };
        return Ok(RoutingResult {
            backend: Arc::new(selected),
            actual_model: model.clone(),
            fallback_used: false,
        });
    }

    // Step 5: Try fallback chain if primary failed
    let fallbacks = self.get_fallbacks(&model);
    for fallback_model in &fallbacks {
        let candidates = self.filter_candidates(fallback_model, requirements);
        if !candidates.is_empty() {
            let selected = /* apply strategy */;
            return Ok(RoutingResult {
                backend: Arc::new(selected),
                actual_model: fallback_model.clone(),
                fallback_used: true,
            });
        }
    }

    // Step 6: Return appropriate error
    if !fallbacks.is_empty() {
        Err(RoutingError::FallbackChainExhausted { chain })
    } else if model_exists {
        Err(RoutingError::NoHealthyBackend { model })
    } else {
        Err(RoutingError::ModelNotFound { model })
    }
}
```

**Mental Model - The Funnel:**
```
    ALL BACKENDS
         │
         ▼
    ┌─────────┐
    │Has model│  ← Only backends with requested model
    └────┬────┘
         │
         ▼
    ┌─────────┐
    │ Healthy │  ← Only healthy backends
    └────┬────┘
         │
         ▼
    ┌─────────┐
    │Has caps │  ← Only backends with required capabilities
    └────┬────┘
         │
         ▼
    ┌─────────┐
    │  Score  │  ← Rank by strategy
    └────┬────┘
         │
         ▼
    BEST BACKEND
```

### The filter_candidates Method

This is where capability matching happens:

```rust
fn filter_candidates(&self, model: &str, requirements: &RequestRequirements) -> Vec<Backend> {
    // Get all backends that have this model
    let mut candidates = self.registry.get_backends_for_model(model);

    // Filter by health status
    candidates.retain(|backend| backend.status == BackendStatus::Healthy);

    // Filter by capabilities
    candidates.retain(|backend| {
        if let Some(model_info) = backend.models.iter().find(|m| m.id == model) {
            // Check vision capability
            if requirements.needs_vision && !model_info.supports_vision {
                return false;
            }
            // Check tools capability
            if requirements.needs_tools && !model_info.supports_tools {
                return false;
            }
            // Check JSON mode capability
            if requirements.needs_json_mode && !model_info.supports_json_mode {
                return false;
            }
            // Check context length
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

**Why `retain()` instead of `filter()`?** It modifies in place rather than creating a new Vec, which is more memory-efficient.

### Strategy Implementations

**Smart Strategy** (weighted scoring):
```rust
fn select_smart(&self, candidates: &[Backend]) -> Backend {
    let best = candidates
        .iter()
        .max_by_key(|backend| {
            let priority = backend.priority as u32;
            let pending = backend.pending_requests.load(Ordering::Relaxed);
            let latency = backend.avg_latency_ms.load(Ordering::Relaxed);
            score_backend(priority, pending, latency, &self.weights)
        })
        .unwrap();
    
    // Clone the backend (atomics need special handling)
    clone_backend(best)
}
```

**Round-Robin Strategy**:
```rust
fn select_round_robin(&self, candidates: &[Backend]) -> Backend {
    // Atomically increment and get previous value
    let counter = self.round_robin_counter.fetch_add(1, Ordering::Relaxed);
    let index = (counter as usize) % candidates.len();
    clone_backend(&candidates[index])
}
```

---

## File 2: error.rs - What Can Go Wrong

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RoutingError {
    /// The requested model was not found in any backend
    #[error("Model '{model}' not found")]
    ModelNotFound { model: String },

    /// No healthy backend is available for the requested model
    #[error("No healthy backend available for model '{model}'")]
    NoHealthyBackend { model: String },

    /// No backend supports the required capabilities
    #[error("No backend supports required capabilities for model '{model}': {missing:?}")]
    CapabilityMismatch { model: String, missing: Vec<String> },

    /// All models in the fallback chain were exhausted
    #[error("All backends in fallback chain unavailable: {chain:?}")]
    FallbackChainExhausted { chain: Vec<String> },
}
```

**Breaking it down:**

| Variant | When It Happens | HTTP Status |
|---------|-----------------|-------------|
| `ModelNotFound` | Model doesn't exist in any backend | 404 |
| `NoHealthyBackend` | Model exists but all backends are down | 503 |
| `CapabilityMismatch` | No backend has required features | 400 |
| `FallbackChainExhausted` | Primary and all fallbacks failed | 503 |

---

## File 3: scoring.rs - The Scoring Algorithm

### Weights Configuration

```rust
pub struct ScoringWeights {
    pub priority: u32,  // How much admin-assigned priority matters (0-100)
    pub load: u32,      // How much current load matters (0-100)
    pub latency: u32,   // How much response time matters (0-100)
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            priority: 50,   // Admin priority is most important
            load: 30,       // Then current load
            latency: 20,    // Then historical latency
        }
    }
}
```

**Why these defaults?**
- **Priority (50%)**: Admins explicitly mark backends as preferred (GPU > CPU)
- **Load (30%)**: Prevent overwhelming busy backends
- **Latency (20%)**: Tiebreaker using historical performance

### The Scoring Function

```rust
pub fn score_backend(
    priority: u32,
    pending_requests: u32,
    avg_latency_ms: u32,
    weights: &ScoringWeights,
) -> u32 {
    // Priority score: lower priority number = higher score
    // priority=1 → 99, priority=10 → 90, priority=100+ → 0
    let priority_score = 100 - priority.min(100);

    // Load score: fewer pending requests = higher score
    // pending=0 → 100, pending=50 → 50, pending=100+ → 0
    let load_score = 100 - pending_requests.min(100);

    // Latency score: lower latency = higher score
    // 0ms → 100, 100ms → 90, 500ms → 50, 1000ms → 0
    let latency_score = 100 - (avg_latency_ms / 10).min(100);

    // Weighted average (weights must sum to 100)
    (priority_score * weights.priority
     + load_score * weights.load
     + latency_score * weights.latency) / 100
}
```

**Example Calculation:**
```
Backend A: priority=1, pending=0, latency=50ms
┌─────────────────────────────────────────┐
│ priority_score = 100 - 1 = 99           │
│ load_score     = 100 - 0 = 100          │
│ latency_score  = 100 - 5 = 95           │
│                                         │
│ total = (99×50 + 100×30 + 95×20) / 100  │
│       = (4950 + 3000 + 1900) / 100      │
│       = 98                               │
└─────────────────────────────────────────┘

Backend B: priority=10, pending=50, latency=500ms
┌─────────────────────────────────────────┐
│ priority_score = 100 - 10 = 90          │
│ load_score     = 100 - 50 = 50          │
│ latency_score  = 100 - 50 = 50          │
│                                         │
│ total = (90×50 + 50×30 + 50×20) / 100   │
│       = (4500 + 1500 + 1000) / 100      │
│       = 70                               │
└─────────────────────────────────────────┘

Winner: Backend A (98 > 70)
```

---

## File 4: strategies.rs - Strategy Definitions

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoutingStrategy {
    #[default]
    Smart,        // Score by priority, load, latency
    RoundRobin,   // Rotate through backends in order
    PriorityOnly, // Always use lowest priority number
    Random,       // Random selection (useful for testing)
}
```

**When to use each:**

| Strategy | Use Case |
|----------|----------|
| `Smart` | Production default - balances all factors |
| `RoundRobin` | Even distribution when backends are similar |
| `PriorityOnly` | Strict preference (always prefer GPU server) |
| `Random` | Testing load distribution |

### FromStr Implementation

```rust
impl FromStr for RoutingStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "smart" => Ok(RoutingStrategy::Smart),
            "round_robin" => Ok(RoutingStrategy::RoundRobin),
            "priority_only" => Ok(RoutingStrategy::PriorityOnly),
            "random" => Ok(RoutingStrategy::Random),
            _ => Err(format!("Unknown routing strategy: {}", s)),
        }
    }
}
```

**Why `to_lowercase()`?** Config files might have `"Smart"`, `"SMART"`, or `"smart"`. We accept all.

---

## File 5: requirements.rs - Request Analysis

This module extracts what a request *needs* from what it *contains*.

```rust
pub struct RequestRequirements {
    pub model: String,              // What model was requested
    pub estimated_tokens: u32,      // How big is the context
    pub needs_vision: bool,         // Does it have images?
    pub needs_tools: bool,          // Does it use function calling?
    pub needs_json_mode: bool,      // Does it need structured output?
}
```

### Extraction Logic

```rust
impl RequestRequirements {
    pub fn from_request(request: &ChatCompletionRequest) -> Self {
        let model = request.model.clone();
        let mut estimated_tokens = 0;
        let mut needs_vision = false;

        // Walk through all messages
        for message in &request.messages {
            match &message.content {
                // Simple text content
                MessageContent::Text { content } => {
                    // Rough token estimate: 1 token ≈ 4 characters
                    estimated_tokens += content.len() as u32 / 4;
                }
                // Multipart content (text + images)
                MessageContent::Parts { content } => {
                    for part in content {
                        if part.part_type == "text" {
                            if let Some(text) = &part.text {
                                estimated_tokens += text.len() as u32 / 4;
                            }
                        } else if part.part_type == "image_url" {
                            needs_vision = true;
                        }
                    }
                }
            }
        }

        // Check for tools in extra fields
        let needs_tools = request.extra.contains_key("tools");

        // Check for JSON mode in response_format
        let needs_json_mode = request.extra
            .get("response_format")
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("type"))
            .and_then(|v| v.as_str())
            .map(|t| t == "json_object")
            .unwrap_or(false);

        Self { model, estimated_tokens, needs_vision, needs_tools, needs_json_mode }
    }
}
```

**Design Note:** We use `request.extra` for tools and response_format because these are OpenAI API fields captured via `#[serde(flatten)]`.

---

## File 6: config/routing.rs - Configuration

### Config Structs

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RoutingConfig {
    pub strategy: RoutingStrategy,              // Enum
    pub max_retries: u32,                       // Used by API layer
    pub weights: RoutingWeights,                // Nested struct
    #[serde(default)]
    pub aliases: HashMap<String, String>,       // Optional section
    #[serde(default)]
    pub fallbacks: HashMap<String, Vec<String>>,
}
```

### How Config Flows to Router

```
nexus.toml                     NexusConfig                    Router
    │                              │                            │
    │  [routing]                   │  routing: RoutingConfig    │
    │  strategy = "smart"    ──▶   │    strategy: Smart    ──▶  │  strategy: Smart
    │  max_retries = 2             │    max_retries: 2          │
    │                              │                            │
    │  [routing.weights]           │    weights:                │  weights:
    │  priority = 50          ──▶  │      priority: 50     ──▶  │    priority: 50
    │  load = 30                   │      load: 30              │    load: 30
    │  latency = 20                │      latency: 20           │    latency: 20
    │                              │                            │
    │  [routing.aliases]           │    aliases: HashMap        │  aliases: HashMap
    │  "gpt-4" = "llama3:70b" ──▶  │      "gpt-4" → ...    ──▶  │    "gpt-4" → ...
```

### Type Conversions

Config types are separate from routing types (separation of concerns):

```rust
impl From<RoutingWeights> for crate::routing::ScoringWeights {
    fn from(weights: RoutingWeights) -> Self {
        crate::routing::ScoringWeights {
            priority: weights.priority,
            load: weights.load,
            latency: weights.latency,
        }
    }
}
```

---

## Understanding the Tests

### Test Categories

| Category | Location | Purpose |
|----------|----------|---------|
| Unit Tests | `src/routing/mod.rs` | Test individual methods |
| Scoring Tests | `src/routing/scoring.rs` | Test scoring algorithm |
| Requirements Tests | `src/routing/requirements.rs` | Test request parsing |
| Integration Tests | `tests/routing_integration.rs` | End-to-end routing |

### Key Test Examples

#### 1. Filtering by Health Status

```rust
#[test]
fn filters_out_unhealthy_backends() {
    let registry = Arc::new(Registry::new());
    registry.add_backend(create_backend("a", BackendStatus::Healthy)).unwrap();
    registry.add_backend(create_backend("b", BackendStatus::Unhealthy)).unwrap();

    let router = Router::new(registry, RoutingStrategy::Smart, ScoringWeights::default());
    let requirements = RequestRequirements { model: "llama3:8b".to_string(), /* ... */ };

    let result = router.select_backend(&requirements).unwrap();
    assert_eq!(result.backend.id, "a");  // Only healthy backend selected
}
```

**What this tests:** Unhealthy backends are excluded from routing.

#### 2. Smart Strategy Scoring

```rust
#[test]
fn smart_selects_highest_score() {
    // Backend A: high priority (1), no load, low latency → high score
    // Backend B: low priority (10), high load, high latency → low score
    
    let result = router.select_backend(&requirements).unwrap();
    assert_eq!(result.backend.name, "Backend A");  // Higher score wins
}
```

**What this tests:** Smart strategy picks the backend with higher score.

#### 3. Round-Robin Distribution

```rust
#[test]
fn round_robin_cycles_through_backends() {
    let names: Vec<String> = (0..6)
        .map(|_| router.select_backend(&req).unwrap().backend.name.clone())
        .collect();

    assert_eq!(names, vec!["A", "B", "C", "A", "B", "C"]);
}
```

**What this tests:** Round-robin visits each backend in order, then wraps.

#### 4. Performance Test

```rust
#[test]
fn routing_performance() {
    // 100 backends, 1000 routing decisions
    let start = std::time::Instant::now();
    for _ in 0..1000 {
        let _ = router.select_backend(&requirements).unwrap();
    }
    let avg_micros = start.elapsed().as_micros() / 1000;
    
    assert!(avg_micros < 1000, "Routing too slow: {} µs", avg_micros);
}
```

**What this tests:** Routing decisions complete in < 1ms.

---

## Key Rust Concepts

| Concept | What It Means | Where Used |
|---------|---------------|------------|
| `Arc<T>` | Shared ownership across threads | Registry reference |
| `AtomicU64` | Thread-safe counter without locks | Round-robin counter |
| `HashMap<K, V>` | Key-value storage | Aliases, fallbacks |
| `Result<T, E>` | Either success (`Ok`) or error (`Err`) | select_backend return |
| `Option<T>` | Either `Some(value)` or `None` | Model lookup |
| `Ordering::Relaxed` | Weakest memory ordering (fastest) | Atomic operations |
| `thiserror::Error` | Derive macro for error types | RoutingError |
| `retain()` | In-place filtering of Vec | filter_candidates |
| `max_by_key()` | Find max element by computed key | select_smart |

---

## Common Patterns in This Codebase

### Pattern 1: Error Handling with `?`

```rust
// Instead of:
let backend = match self.backends.get(id) {
    Some(b) => b,
    None => return Err(RoutingError::ModelNotFound { model }),
};

// We write:
let backend = self.backends
    .get(id)
    .ok_or_else(|| RoutingError::ModelNotFound { model })?;
```

### Pattern 2: Method Chaining for Filtering

```rust
let mut candidates = self.registry.get_backends_for_model(model);
candidates.retain(|b| b.status == BackendStatus::Healthy);
candidates.retain(|b| has_required_capabilities(b, requirements));
```

### Pattern 3: View Models for Serialization

```rust
// Internal type (has atomics, not serializable)
pub struct Backend {
    pub pending_requests: AtomicU32,
}

// View type (simple, serializable)
pub struct BackendView {
    pub pending_requests: u32,
}

impl From<&Backend> for BackendView { /* ... */ }
```

### Pattern 4: Graceful Error Handling

```rust
// Bad: panics on missing model
let backend = candidates.first().unwrap();

// Good: returns error the API can convert to HTTP response
if candidates.is_empty() {
    return Err(RoutingError::ModelNotFound { model });
}
```

### Pattern 5: Builder-Style Constructors

```rust
// Basic construction
let router = Router::new(registry, strategy, weights);

// Full construction with optional features
let router = Router::with_aliases_and_fallbacks(
    registry, strategy, weights, aliases, fallbacks,
);
```

---

## Next Steps

Now that you understand the Intelligent Router, explore:

1. **Model Aliases (F07)** - specs/007-model-aliases/ - How alias resolution works
2. **Fallback Chains (F08)** - specs/008-fallback-chains/ - How fallbacks are configured
3. **API Gateway** (`src/api/`) - How requests flow through to the router
4. **Run the tests** - `cargo test routing::` to see all routing tests

Each feature builds on the core routing logic covered here.
