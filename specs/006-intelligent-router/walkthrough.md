# F06 Intelligent Router - Code Walkthrough

**Audience**: Junior developers joining the Nexus project  
**Prerequisite Reading**: [spec.md](./spec.md), [plan.md](./plan.md)

---

## 1. The Big Picture

### What Problem Does the Router Solve?

Nexus is an LLM orchestrator that manages multiple inference backends (Ollama servers, vLLM instances, etc.). When a client sends a chat completion request, Nexus needs to decide **which backend** should handle it.

This isn't trivial because:
1. Different backends host different models (`llama3:8b` vs `mistral:7b`)
2. Some models have special capabilities (vision, tool calling)
3. Backends have varying loads and response times
4. Some backends may be down

The **Intelligent Router** solves this by:
1. **Filtering** - Find backends that CAN serve the request
2. **Scoring** - Rank candidates by how WELL they can serve it
3. **Selecting** - Pick the best one using a configurable strategy

### Where Routing Fits in the Request Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│                         REQUEST LIFECYCLE                            │
└─────────────────────────────────────────────────────────────────────┘

  Client Request                           
       │                                   
       ▼                                   
┌──────────────┐                          
│  API Layer   │  POST /v1/chat/completions
│ (src/api/)   │                          
└──────────────┘                          
       │                                   
       │ ChatCompletionRequest             
       ▼                                   
┌──────────────┐                          
│   ROUTER     │  ◀── YOU ARE HERE        
│(src/routing/)│                          
└──────────────┘                          
       │                                   
       │ Arc<Backend>                      
       ▼                                   
┌──────────────┐                          
│   Registry   │  Source of truth for backends
│(src/registry)│                          
└──────────────┘                          
       │                                   
       ▼                                   
┌──────────────┐                          
│   Backend    │  Actual inference server  
│  (external)  │                          
└──────────────┘                          
```

### Key Architectural Insight

The router **never makes network calls**. It only reads from the in-memory Registry, which is kept up-to-date by the Health Checker (background task). This is why routing decisions are guaranteed to complete in < 1ms.

---

## 2. File-by-File Walkthrough

### 2.1 `src/routing/mod.rs` - The Brain

This is the main file containing the `Router` struct and `select_backend` logic.

#### The Router Struct

```rust
// src/routing/mod.rs, lines 25-43

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

**Key Design Decisions:**
- `Arc<Registry>` - Shared ownership allows the router to read backend data that other components (health checker) update
- `AtomicU64` for round-robin - No mutex needed, just atomic increment

#### The select_backend Method

This is the core algorithm. Let's break it down:

```rust
// src/routing/mod.rs, lines 87-134

pub fn select_backend(
    &self,
    requirements: &RequestRequirements,
) -> Result<Arc<Backend>, RoutingError> {
    // Step 1: Resolve alias first
    // If request asks for "gpt-4", convert to "llama3:70b"
    let model = self.resolve_alias(&requirements.model);

    // Step 2: Try to find backend for the primary model
    let candidates = self.filter_candidates(&model, requirements);

    if !candidates.is_empty() {
        // Step 3: Apply routing strategy to pick from candidates
        let selected = match self.strategy {
            RoutingStrategy::Smart => self.select_smart(&candidates),
            RoutingStrategy::RoundRobin => self.select_round_robin(&candidates),
            RoutingStrategy::PriorityOnly => self.select_priority_only(&candidates),
            RoutingStrategy::Random => self.select_random(&candidates),
        };
        return Ok(Arc::new(selected));
    }

    // Step 4: Primary model failed - try fallback chain
    let fallbacks = self.get_fallbacks(&model);
    for fallback_model in &fallbacks {
        let candidates = self.filter_candidates(fallback_model, requirements);
        if !candidates.is_empty() {
            // Found a fallback that works!
            let selected = match self.strategy { /* ... */ };
            return Ok(Arc::new(selected));
        }
    }

    // Step 5: All attempts failed - return appropriate error
    if !fallbacks.is_empty() {
        Err(RoutingError::FallbackChainExhausted { chain })
    } else {
        Err(RoutingError::ModelNotFound { model })
    }
}
```

**Mental Model:** Think of it as a funnel:
1. Start with ALL backends
2. Keep only those with the requested model
3. Keep only healthy ones
4. Keep only those with required capabilities
5. Score and pick the best

#### The filter_candidates Method

This is where capability matching happens:

```rust
// src/routing/mod.rs, lines 276-319

fn filter_candidates(
    &self,
    model: &str,
    requirements: &RequestRequirements,
) -> Vec<Backend> {
    // Get all backends that have this model
    let mut candidates = self.registry.get_backends_for_model(model);

    // Filter by health status
    candidates.retain(|backend| backend.status == BackendStatus::Healthy);

    // Filter by capabilities
    candidates.retain(|backend| {
        if let Some(model_info) = backend.models.iter().find(|m| m.id == model) {
            // Check each required capability
            if requirements.needs_vision && !model_info.supports_vision {
                return false;  // Vision needed but not supported
            }
            if requirements.needs_tools && !model_info.supports_tools {
                return false;  // Tools needed but not supported
            }
            if requirements.needs_json_mode && !model_info.supports_json_mode {
                return false;
            }
            // Check context length
            if requirements.estimated_tokens > model_info.context_length {
                return false;  // Request too large for this model
            }
            true
        } else {
            false  // Model not found (shouldn't happen)
        }
    });

    candidates
}
```

**Why `retain`?** It's more efficient than `filter()` because it modifies in place rather than creating a new Vec.

#### Strategy Implementations

**Smart Strategy** (weighted scoring):
```rust
// src/routing/mod.rs, lines 137-171

fn select_smart(&self, candidates: &[Backend]) -> Backend {
    let best = candidates
        .iter()
        .max_by_key(|backend| {
            // Read atomic values (thread-safe)
            let priority = backend.priority as u32;
            let pending = backend.pending_requests.load(Ordering::Relaxed);
            let latency = backend.avg_latency_ms.load(Ordering::Relaxed);
            
            // Calculate weighted score (0-100)
            score_backend(priority, pending, latency, &self.weights)
        })
        .unwrap();  // Safe: we only call this when candidates is non-empty

    // Clone the backend (atomics need special handling)
    Backend { /* ... field by field copy ... */ }
}
```

**Round-Robin Strategy**:
```rust
// src/routing/mod.rs, lines 173-204

fn select_round_robin(&self, candidates: &[Backend]) -> Backend {
    // Atomically increment and get previous value
    let counter = self
        .round_robin_counter
        .fetch_add(1, Ordering::Relaxed);
    
    // Modulo to wrap around
    let index = (counter as usize) % candidates.len();
    
    // Return the backend at this index
    let best = &candidates[index];
    Backend { /* ... */ }
}
```

**Why `fetch_add` with `Relaxed`?** We don't need strong ordering guarantees - it's fine if two concurrent requests occasionally get the same backend. The important thing is the counter increments atomically.

---

### 2.2 `src/routing/requirements.rs` - Request Analysis

This module extracts what a request *needs* from what it *contains*.

```rust
// src/routing/requirements.rs, lines 6-22

pub struct RequestRequirements {
    pub model: String,              // What model was requested
    pub estimated_tokens: u32,      // How big is the context
    pub needs_vision: bool,         // Does it have images?
    pub needs_tools: bool,          // Does it use function calling?
    pub needs_json_mode: bool,      // Does it need structured output?
}
```

#### Extraction Logic

```rust
// src/routing/requirements.rs, lines 24-72

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
                            needs_vision = true;  // Found an image!
                        }
                    }
                }
            }
        }

        // Check for tools in extra fields
        let needs_tools = request.extra.contains_key("tools");

        // Check for JSON mode
        let needs_json_mode = request
            .extra
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

**Design Note:** We use `request.extra` for tools and response_format because these are OpenAI API fields that aren't in our core `ChatCompletionRequest` struct. The `#[serde(flatten)]` pattern captures them.

---

### 2.3 `src/routing/scoring.rs` - The Scoring Algorithm

This module implements the scoring function for the Smart strategy.

#### Weights Configuration

```rust
// src/routing/scoring.rs, lines 4-24

pub struct ScoringWeights {
    pub priority: u32,  // How much does admin-assigned priority matter?
    pub load: u32,      // How much does current load matter?
    pub latency: u32,   // How much does response time matter?
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

**Why These Defaults?** Priority gets 50% because admins explicitly mark backends as preferred (e.g., GPU server > CPU server). Load gets 30% to prevent overwhelming busy backends. Latency gets 20% as a tiebreaker.

#### The Scoring Function

```rust
// src/routing/scoring.rs, lines 41-63

pub fn score_backend(
    priority: u32,
    pending_requests: u32,
    avg_latency_ms: u32,
    weights: &ScoringWeights,
) -> u32 {
    // Priority score: lower priority number = higher score
    // priority=1 → score=99, priority=10 → score=90, priority=100+ → score=0
    let priority_score = 100 - priority.min(100);

    // Load score: fewer pending requests = higher score
    // pending=0 → score=100, pending=50 → score=50, pending=100+ → score=0
    let load_score = 100 - pending_requests.min(100);

    // Latency score: lower latency = higher score
    // 0ms→100, 100ms→90, 500ms→50, 1000ms→0
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
- priority_score = 100 - 1 = 99
- load_score = 100 - 0 = 100  
- latency_score = 100 - 5 = 95
- total = (99*50 + 100*30 + 95*20) / 100 = 78.5 ≈ 78

Backend B: priority=10, pending=50, latency=500ms
- priority_score = 100 - 10 = 90
- load_score = 100 - 50 = 50
- latency_score = 100 - 50 = 50
- total = (90*50 + 50*30 + 50*20) / 100 = 70

Winner: Backend A (78 > 70)
```

---

### 2.4 `src/routing/strategies.rs` - Strategy Definitions

A simple enum with string parsing support:

```rust
// src/routing/strategies.rs, lines 6-20

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoutingStrategy {
    #[default]
    Smart,        // Score by priority, load, latency
    RoundRobin,   // Rotate through backends
    PriorityOnly, // Always use lowest priority number
    Random,       // Random selection (useful for testing)
}
```

#### FromStr Implementation

```rust
// src/routing/strategies.rs, lines 22-35

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

### 2.5 `src/routing/error.rs` - Error Types

Using `thiserror` for ergonomic error definitions:

```rust
// src/routing/error.rs, lines 1-26

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
    CapabilityMismatch {
        model: String,
        missing: Vec<String>,
    },

    /// All models in the fallback chain were exhausted
    #[error("All backends in fallback chain unavailable: {chain:?}")]
    FallbackChainExhausted { chain: Vec<String> },
}
```

**Why `thiserror`?** It generates the `std::error::Error` trait implementation and formats nice error messages. The `#[error("...")]` attribute defines the `Display` implementation.

---

## 3. Key Rust Concepts Used

### 3.1 `Arc<T>` - Atomic Reference Counting

```rust
registry: Arc<Registry>,
```

**What it does:** Allows multiple owners of the same data. When the last `Arc` is dropped, the data is freed.

**Why we use it:** The `Registry` is shared between:
- Router (reads backend data)
- Health Checker (updates backend status)
- API handlers (reads for responses)

**Common pattern:**
```rust
let registry = Arc::new(Registry::new());
let router = Router::new(Arc::clone(&registry), ...);  // Cheap clone
let health_checker = HealthChecker::new(Arc::clone(&registry), ...);
```

### 3.2 `AtomicU64` / `AtomicU32` - Lock-Free Counters

```rust
round_robin_counter: AtomicU64,
pending_requests: AtomicU32,
```

**What it does:** Allows multiple threads to read/write a value without locks.

**Key operations:**
```rust
// Increment and get old value (atomic)
let old = counter.fetch_add(1, Ordering::Relaxed);

// Just read the current value
let current = counter.load(Ordering::Relaxed);

// Set a new value
counter.store(42, Ordering::Relaxed);
```

**Why `Ordering::Relaxed`?** We don't need synchronization with other memory operations. We just need the increment itself to be atomic.

### 3.3 `HashMap<K, V>` - Key-Value Storage

```rust
aliases: HashMap<String, String>,
fallbacks: HashMap<String, Vec<String>>,
```

**Usage patterns:**
```rust
// Lookup with default
let target = aliases.get("gpt-4").cloned().unwrap_or_else(|| "gpt-4".to_string());

// Check if key exists
if request.extra.contains_key("tools") { ... }
```

### 3.4 `thiserror` - Error Derivation

```rust
#[derive(Debug, Error)]
pub enum RoutingError {
    #[error("Model '{model}' not found")]
    ModelNotFound { model: String },
}
```

**What it generates:**
```rust
impl std::fmt::Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoutingError::ModelNotFound { model } => {
                write!(f, "Model '{}' not found", model)
            }
            // ...
        }
    }
}

impl std::error::Error for RoutingError {}
```

---

## 4. Configuration Integration

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

### Config Structs (`src/config/routing.rs`)

```rust
// src/config/routing.rs, lines 32-43

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

### Type Conversions

Config types are separate from routing types (separation of concerns). We use `From` trait:

```rust
// src/config/routing.rs, lines 75-83

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

### Router Creation in AppState

```rust
// src/api/mod.rs, lines 102-109

let router = Arc::new(routing::Router::with_aliases_and_fallbacks(
    Arc::clone(&registry),
    config.routing.strategy.into(),      // RoutingStrategy conversion
    config.routing.weights.clone().into(), // ScoringWeights conversion
    config.routing.aliases.clone(),        // Direct clone
    config.routing.fallbacks.clone(),
));
```

---

## 5. Test Strategy and Key Examples

### Test Organization

Tests are organized by what they verify:

```
src/routing/mod.rs
├── mod tests                    # Strategy parsing
├── mod filter_tests             # Candidate filtering
├── mod smart_strategy_tests     # Smart scoring
├── mod other_strategies_tests   # RoundRobin, Priority, Random
└── mod alias_and_fallback_tests # Alias resolution, fallbacks
```

### Test Helper Pattern

Each test module defines helper functions to create test data:

```rust
// src/routing/mod.rs, lines 377-399

fn create_test_backend(
    id: &str,
    name: &str,
    status: BackendStatus,
    models: Vec<Model>,
) -> Backend {
    Backend {
        id: id.to_string(),
        name: name.to_string(),
        url: format!("http://{}", name),
        backend_type: BackendType::Ollama,
        status,
        // ... other fields with sensible defaults
    }
}
```

**Why helpers?** Backend has many fields. Helpers let tests focus on what matters.

### Key Test Examples

#### 1. Filtering by Health Status

```rust
// src/routing/mod.rs, lines 462-491

#[test]
fn filters_out_unhealthy_backends() {
    let backends = vec![
        create_test_backend("a", "Backend A", BackendStatus::Healthy, /*...*/),
        create_test_backend("b", "Backend B", BackendStatus::Unhealthy, /*...*/),
    ];

    let router = create_test_router(backends);
    let requirements = RequestRequirements { /* ... */ };

    let candidates = router.filter_candidates("llama3:8b", &requirements);
    
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "Backend A");
}
```

**What this tests:** Unhealthy backends are excluded from routing.

#### 2. Smart Strategy Scoring

```rust
// src/routing/mod.rs, lines 632-652

#[test]
fn smart_selects_highest_score() {
    let backends = vec![
        // Backend A: high priority (1), no load, low latency → high score
        create_test_backend_with_state("a", "Backend A", 1, 0, 50),
        // Backend B: low priority (10), high load, high latency → low score
        create_test_backend_with_state("b", "Backend B", 10, 50, 500),
    ];

    let router = create_test_router(backends);
    let requirements = RequestRequirements { /* ... */ };

    let backend = router.select_backend(&requirements).unwrap();
    assert_eq!(backend.name, "Backend A");
}
```

**What this tests:** Smart strategy picks the backend with higher score.

#### 3. Round-Robin Distribution

```rust
// src/routing/mod.rs, lines 768-803

#[test]
fn round_robin_cycles_through_backends() {
    let backends = vec![
        create_test_backend_simple("a", "Backend A", 1),
        create_test_backend_simple("b", "Backend B", 1),
        create_test_backend_simple("c", "Backend C", 1),
    ];

    let router = create_test_router_with_strategy(backends, RoutingStrategy::RoundRobin);
    
    // Should cycle: A, B, C, A, B, C
    let names: Vec<String> = (0..6)
        .map(|_| router.select_backend(&req).unwrap().name.clone())
        .collect();

    assert_eq!(names, vec!["Backend A", "Backend B", "Backend C",
                           "Backend A", "Backend B", "Backend C"]);
}
```

**What this tests:** Round-robin visits each backend in order, then wraps.

#### 4. Alias Resolution

```rust
// src/routing/mod.rs, lines 895-929

#[test]
fn resolves_alias_transparently() {
    let backends = vec![
        create_test_backend_with_model("a", "Backend A", "llama3:70b"),
    ];

    let mut aliases = HashMap::new();
    aliases.insert("gpt-4".to_string(), "llama3:70b".to_string());

    let router = Router::with_aliases_and_fallbacks(
        registry, strategy, weights, aliases, HashMap::new(),
    );

    let requirements = RequestRequirements {
        model: "gpt-4".to_string(),  // Ask for alias
        /* ... */
    };

    let backend = router.select_backend(&requirements).unwrap();
    assert_eq!(backend.name, "Backend A");  // Gets routed to actual model
}
```

**What this tests:** Client can use "gpt-4" and get routed to "llama3:70b".

---

## 6. Common Patterns in This Codebase

### Pattern 1: View Models for Output

Internal types (with atomics, complex state) are separate from API response types:

```rust
// Internal - has atomics, not serializable directly
pub struct Backend {
    pub pending_requests: AtomicU32,
    // ...
}

// View - simple, serializable
pub struct BackendView {
    pub pending_requests: u32,
    // ...
}

impl From<&Backend> for BackendView {
    fn from(backend: &Backend) -> Self {
        BackendView {
            pending_requests: backend.pending_requests.load(Ordering::Relaxed),
            // ...
        }
    }
}
```

### Pattern 2: Builder-Style Configuration

```rust
// Basic construction
let router = Router::new(registry, strategy, weights);

// Full construction with optional features
let router = Router::with_aliases_and_fallbacks(
    registry,
    strategy, 
    weights,
    aliases,      // Optional
    fallbacks,    // Optional
);
```

### Pattern 3: Method Chaining for Filtering

```rust
let mut candidates = self.registry.get_backends_for_model(model);
candidates.retain(|b| b.status == BackendStatus::Healthy);
candidates.retain(|b| self.check_capabilities(b, requirements));
```

### Pattern 4: Graceful Error Handling

Never panic on user input. Return descriptive errors:

```rust
// Bad: panics on missing model
let backend = candidates.first().unwrap();

// Good: returns error the API can convert to HTTP 404
if candidates.is_empty() {
    return Err(RoutingError::ModelNotFound { model });
}
```

### Pattern 5: Test Module Organization

```rust
// Main implementation
pub struct Router { /* ... */ }

impl Router { /* ... */ }

// Tests at bottom, gated by cfg
#[cfg(test)]
mod tests {
    use super::*;
    
    // Helper functions
    fn create_test_backend() -> Backend { /* ... */ }
    
    // Actual tests
    #[test]
    fn test_something() { /* ... */ }
}

// Separate test modules for different concerns
#[cfg(test)]
mod filter_tests { /* ... */ }

#[cfg(test)]
mod scoring_tests { /* ... */ }
```

---

## Quick Reference

| File | Purpose | Key Types |
|------|---------|-----------|
| `mod.rs` | Router struct, select_backend | `Router`, strategy methods |
| `requirements.rs` | Request analysis | `RequestRequirements` |
| `scoring.rs` | Backend scoring | `ScoringWeights`, `score_backend()` |
| `strategies.rs` | Strategy enum | `RoutingStrategy` |
| `error.rs` | Error types | `RoutingError` |

---

## Next Steps

1. **Read the spec** - [spec.md](./spec.md) has the full requirements
2. **Run the tests** - `cargo test routing::` to see all routing tests
3. **Trace a request** - Start at `src/api/completions.rs` and follow the routing call
4. **Experiment** - Change weights in `nexus.example.toml` and observe behavior
