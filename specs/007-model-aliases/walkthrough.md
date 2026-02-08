# Model Aliases - Code Walkthrough

**Feature**: F07 - Model Aliases  
**Audience**: Junior developers joining the project  
**Last Updated**: 2026-02-08

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: routing.rs - Alias Configuration](#file-1-routingrs---alias-configuration)
4. [File 2: mod.rs - Alias Resolution in Router](#file-2-modrs---alias-resolution-in-router)
5. [Understanding the Tests](#understanding-the-tests)
6. [Key Rust Concepts](#key-rust-concepts)
7. [Common Patterns in This Codebase](#common-patterns-in-this-codebase)
8. [Next Steps](#next-steps)

---

## The Big Picture

Think of Model Aliases as a **translator at a hotel front desk**. When a guest (client) asks for "gpt-4", the translator (alias system) looks up their phrasebook and says "Ah, you want llama3:70b!" to the hotel staff (backend).

### Why Aliases?

Many tools and client libraries are hardcoded to request OpenAI model names like `gpt-4` or `gpt-3.5-turbo`. Instead of modifying every client, Nexus translates these names to your local models:

```
Client Request          Nexus Alias System        Backend
     │                        │                      │
     │  "gpt-4 please!"       │                      │
     │───────────────────────>│                      │
     │                        │  Look up alias...    │
     │                        │  gpt-4 → llama3:70b  │
     │                        │                      │
     │                        │  "llama3:70b please!"│
     │                        │─────────────────────>│
     │                        │                      │
     │                        │      Response        │
     │<───────────────────────│<─────────────────────│
     │  (shows "gpt-4")       │                      │
```

The client never knows the translation happened—responses show the original requested model name.

### How It Fits in Nexus

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              Nexus                                       │
│                                                                          │
│  ┌──────────────┐     ┌─────────────────────────────────────────────┐   │
│  │   API        │────>│               Router                         │   │
│  │   Gateway    │     │                                              │   │
│  │              │     │   ┌─────────────────────────────────────┐   │   │
│  └──────────────┘     │   │  1. resolve_alias()                 │   │   │
│                       │   │     "gpt-4" → "llama-large"         │   │   │
│                       │   │     "llama-large" → "llama3:70b"    │   │   │
│                       │   │     (max 3 levels)                  │   │   │
│                       │   └─────────────────────────────────────┘   │   │
│                       │                     │                        │   │
│                       │                     ▼                        │   │
│                       │   ┌─────────────────────────────────────┐   │   │
│                       │   │  2. Find backends for resolved model │   │   │
│                       │   └─────────────────────────────────────┘   │   │
│                       │                     │                        │   │
│                       │                     ▼                        │   │
│                       │   ┌─────────────────────────────────────┐   │   │
│                       │   │  3. Select best backend              │   │   │
│                       │   └─────────────────────────────────────┘   │   │
│                       │                                              │   │
│                       └──────────────────────────────────────────────┘   │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### Key Behaviors

| Behavior | Description |
|----------|-------------|
| **Transparent** | Client sees requested name, not resolved name |
| **Direct match priority** | If "gpt-4" actually exists, use it (don't alias) |
| **Chaining** | `a → b → c` works (max 3 levels) |
| **Circular detection** | `a → b → a` rejected at config load |
| **Works with fallbacks** | Aliases resolve first, then fallback kicks in |

---

## File Structure

```
src/
├── config/
│   ├── routing.rs          # Alias HashMap + validate_aliases()
│   └── error.rs            # CircularAlias error variant
└── routing/
    └── mod.rs              # resolve_alias() method (lines 91-123)

tests/
└── routing_integration.rs  # Alias integration tests
```

---

## File 1: routing.rs - Alias Configuration

This file defines how aliases are stored and validated at config load time.

### The RoutingConfig Struct

```rust
/// Routing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RoutingConfig {
    pub strategy: RoutingStrategy,
    pub max_retries: u32,
    pub weights: RoutingWeights,
    #[serde(default)]
    pub aliases: HashMap<String, String>,   // <-- Alias storage
    #[serde(default)]
    pub fallbacks: HashMap<String, Vec<String>>,
}
```

**What's happening:**
- `aliases: HashMap<String, String>` - Maps alias names to target model names
- `#[serde(default)]` - If not in config file, use empty HashMap

**Example config file:**
```toml
[routing.aliases]
"gpt-4" = "llama3:70b"
"gpt-3.5-turbo" = "mistral:7b"
```

This becomes:
```rust
HashMap {
    "gpt-4" => "llama3:70b",
    "gpt-3.5-turbo" => "mistral:7b",
}
```

### Circular Alias Detection

Aliases can form chains: `gpt-4 → llama-large → llama3:70b`. But what if someone accidentally creates a loop: `a → b → a`? This would cause infinite loops at runtime!

The `validate_aliases` function catches these at config load:

```rust
/// Validate aliases for circular references
pub fn validate_aliases(aliases: &HashMap<String, String>) -> Result<(), ConfigError> {
    for start in aliases.keys() {
        let mut current = start;
        let mut visited = HashSet::new();
        visited.insert(start);

        while let Some(target) = aliases.get(current) {
            if visited.contains(target) {
                return Err(ConfigError::CircularAlias {
                    start: start.clone(),
                    cycle: target.clone(),
                });
            }
            visited.insert(target);
            current = target;
        }
    }
    Ok(())
}
```

**Algorithm walkthrough:**

```
Given aliases: {"a" → "b", "b" → "c", "c" → "a"}

Starting from "a":
  visited = {"a"}
  
  Step 1: a → b
    Is "b" in visited? No
    visited = {"a", "b"}
    current = "b"
    
  Step 2: b → c  
    Is "c" in visited? No
    visited = {"a", "b", "c"}
    current = "c"
    
  Step 3: c → a
    Is "a" in visited? YES! 🚨
    Return CircularAlias error!
```

**Visual representation:**

```
Linear chain (OK):           Circular chain (ERROR):
                             
a → b → c → d (end)          a → b → c
                             ↑       ↓
No alias for "d"             └───────┘
Resolution stops             Infinite loop!
```

### The CircularAlias Error

```rust
#[derive(Error, Debug)]
pub enum ConfigError {
    // ... other variants ...
    
    #[error("Circular alias detected: '{start}' eventually points back to '{cycle}'")]
    CircularAlias { start: String, cycle: String },
}
```

When this error occurs, the server **won't start**. This is intentional—fail fast at config load, not at runtime when a user makes a request.

---

## File 2: mod.rs - Alias Resolution in Router

This file contains the `resolve_alias()` method that runs on every request.

### The Router Struct

```rust
pub struct Router {
    registry: Arc<Registry>,
    strategy: RoutingStrategy,
    weights: ScoringWeights,
    
    /// Model aliases (alias → target)
    aliases: HashMap<String, String>,
    
    fallbacks: HashMap<String, Vec<String>>,
    round_robin_counter: AtomicU64,
}
```

### Constructor with Aliases

```rust
/// Create a new router with aliases and fallbacks
pub fn with_aliases_and_fallbacks(
    registry: Arc<Registry>,
    strategy: RoutingStrategy,
    weights: ScoringWeights,
    aliases: HashMap<String, String>,
    fallbacks: HashMap<String, Vec<String>>,
) -> Self {
    Self {
        registry,
        strategy,
        weights,
        aliases,
        fallbacks,
        round_robin_counter: AtomicU64::new(0),
    }
}
```

### The resolve_alias Method

This is the heart of the feature—it resolves alias chains up to 3 levels deep:

```rust
/// Resolve model aliases with chaining support (max 3 levels)
fn resolve_alias(&self, model: &str) -> String {
    let mut current = model.to_string();
    let mut depth = 0;
    const MAX_DEPTH: usize = 3;

    while depth < MAX_DEPTH {
        match self.aliases.get(&current) {
            Some(target) => {
                tracing::debug!(
                    from = %current,
                    to = %target,
                    depth = depth + 1,
                    "Resolved alias"
                );
                current = target.clone();
                depth += 1;
            }
            None => break,
        }
    }

    if depth > 0 {
        tracing::debug!(
            original = %model,
            resolved = %current,
            chain_depth = depth,
            "Alias resolution complete"
        );
    }

    current
}
```

**Step-by-step explanation:**

```
Input: "gpt-4"
Aliases: {
    "gpt-4" → "llama-large",
    "llama-large" → "llama3:70b"
}

Iteration 1 (depth=0):
  current = "gpt-4"
  aliases.get("gpt-4") = Some("llama-large")
  LOG: "Resolved alias from=gpt-4 to=llama-large depth=1"
  current = "llama-large"
  depth = 1

Iteration 2 (depth=1):
  current = "llama-large"
  aliases.get("llama-large") = Some("llama3:70b")
  LOG: "Resolved alias from=llama-large to=llama3:70b depth=2"
  current = "llama3:70b"
  depth = 2

Iteration 3 (depth=2):
  current = "llama3:70b"
  aliases.get("llama3:70b") = None
  Break out of loop!

LOG: "Alias resolution complete original=gpt-4 resolved=llama3:70b chain_depth=2"
Return: "llama3:70b"
```

**Visual flow:**

```
         MAX_DEPTH = 3
         ┌─────────┬─────────┬─────────┬─────────┐
Depth:   │    0    │    1    │    2    │    3    │
         └─────────┴─────────┴─────────┴─────────┘
                      │         │         │
                      ▼         ▼         ▼
Chain:   "gpt-4" → "llama-large" → "llama3:70b" → (stop)
         
         └──────────────────┬──────────────────┘
                           │
                    2 hops (within limit)
```

### Why Max 3 Levels?

Consider this 4-level chain:

```toml
[routing.aliases]
"a" = "b"
"b" = "c"
"c" = "d"
"d" = "e"
```

Resolution with MAX_DEPTH=3:

```
a → b (depth 1)
b → c (depth 2)
c → d (depth 3)
STOP! (at max depth)

Returns "d", not "e"
```

This prevents:
1. Accidental infinite loops (defense in depth—circular detection should catch these, but this is a safety net)
2. Confusing configurations with too many levels
3. Performance issues from deep chains

### How select_backend Uses resolve_alias

```rust
pub fn select_backend(
    &self,
    requirements: &RequestRequirements,
) -> Result<RoutingResult, RoutingError> {
    // Step 1: Resolve alias first
    let model = self.resolve_alias(&requirements.model);
    
    // Step 2: Check if model exists
    let all_backends = self.registry.get_backends_for_model(&model);
    let model_exists = !all_backends.is_empty();
    
    // Step 3: Try to find backend for the resolved model
    let candidates = self.filter_candidates(&model, requirements);
    
    if !candidates.is_empty() {
        // Apply routing strategy and return
        let selected = match self.strategy {
            RoutingStrategy::Smart => self.select_smart(&candidates),
            // ... other strategies
        };
        return Ok(RoutingResult {
            backend: Arc::new(selected),
            actual_model: model.clone(),
            fallback_used: false,
        });
    }
    
    // Step 4: If no backends, try fallback chain
    // ... fallback logic ...
}
```

**Key insight:** Aliases are resolved **before** backend lookup. The registry only sees the final resolved model name.

---

## Understanding the Tests

### Test Categories

| Category | Location | Purpose |
|----------|----------|---------|
| Circular Detection | `src/config/routing.rs` | Validate config rejects loops |
| Alias Resolution | `tests/routing_integration.rs` | End-to-end alias routing |
| Chain Depth | `tests/routing_integration.rs` | Max 3 level enforcement |
| Alias + Fallback | `tests/routing_integration.rs` | Combined feature interaction |

### Test 1: Basic Alias Resolution

```rust
#[test]
fn test_routing_with_aliases() {
    // Setup registry with target model
    let registry = Arc::new(Registry::new());
    registry.add_backend(create_test_backend(
        "backend1", "Backend 1", "llama3:70b", 1,
    )).unwrap();

    // Create config with alias: gpt-4 → llama3:70b
    let mut config = NexusConfig::default();
    config.routing.aliases.insert(
        "gpt-4".to_string(), 
        "llama3:70b".to_string()
    );
    
    let state = AppState::new(registry, Arc::new(config));

    // Request using alias
    let request = ChatCompletionRequest {
        model: "gpt-4".to_string(),  // <-- Alias!
        // ...
    };

    let requirements = RequestRequirements::from_request(&request);
    let result = state.router.select_backend(&requirements).unwrap();

    // Verify alias resolved to actual model
    assert_eq!(result.backend.models[0].id, "llama3:70b");
}
```

**What this tests:**
- Client requests "gpt-4"
- Router resolves to "llama3:70b"
- Backend with "llama3:70b" is selected

### Test 2: Chained Aliases

```rust
#[test]
fn test_routing_with_chained_aliases() {
    let registry = Arc::new(Registry::new());
    registry.add_backend(create_test_backend(
        "backend1", "Backend 1", "llama3:70b", 1,
    )).unwrap();

    // Create 2-level chain: gpt-4 → llama-large → llama3:70b
    let mut config = NexusConfig::default();
    config.routing.aliases.insert(
        "gpt-4".to_string(), 
        "llama-large".to_string()
    );
    config.routing.aliases.insert(
        "llama-large".to_string(), 
        "llama3:70b".to_string()
    );
    
    let state = AppState::new(registry, Arc::new(config));

    let request = ChatCompletionRequest {
        model: "gpt-4".to_string(),
        // ...
    };

    let requirements = RequestRequirements::from_request(&request);
    let result = state.router.select_backend(&requirements).unwrap();

    // Should resolve through 2-level chain
    assert_eq!(result.backend.models[0].id, "llama3:70b");
}
```

**Chain visualization:**

```
Config:
  gpt-4 → llama-large
  llama-large → llama3:70b

Resolution:
  "gpt-4" ─(hop 1)─> "llama-large" ─(hop 2)─> "llama3:70b"
```

### Test 3: Max Depth Enforcement

```rust
#[test]
fn test_routing_with_max_depth_chain() {
    let registry = Arc::new(Registry::new());
    // Register backends for "c" and "d" (not "e")
    registry.add_backend(create_test_backend("backend_c", "Backend C", "c", 1)).unwrap();
    registry.add_backend(create_test_backend("backend_d", "Backend D", "d", 2)).unwrap();

    // Create 4-level chain: a → b → c → d → e
    let mut config = NexusConfig::default();
    config.routing.aliases.insert("a".to_string(), "b".to_string());
    config.routing.aliases.insert("b".to_string(), "c".to_string());
    config.routing.aliases.insert("c".to_string(), "d".to_string());
    config.routing.aliases.insert("d".to_string(), "e".to_string());
    
    let state = AppState::new(registry, Arc::new(config));

    let request = ChatCompletionRequest {
        model: "a".to_string(),
        // ...
    };

    let requirements = RequestRequirements::from_request(&request);
    let result = state.router.select_backend(&requirements).unwrap();

    // Should stop at depth 3, resolve to "d" (not "e")
    assert_eq!(result.backend.models[0].id, "d");
}
```

**Why "d" and not "e"?**

```
Chain: a → b → c → d → e
       │   │   │   │   
       ▼   ▼   ▼   ▼   
Depth: 1   2   3   STOP (max reached)

Resolved: "d"
Backend "d" exists → selected
```

### Test 4: Circular Alias Detection

```rust
#[test]
fn test_routing_rejects_circular_config() {
    let mut config = NexusConfig::default();
    config.routing.aliases.insert("a".to_string(), "b".to_string());
    config.routing.aliases.insert("b".to_string(), "a".to_string());

    let result = config.validate();
    
    assert!(result.is_err());
    match result.unwrap_err() {
        ConfigError::CircularAlias { start, cycle } => {
            assert!(start == "a" || start == "b");
        }
        err => panic!("Expected CircularAlias error, got: {:?}", err),
    }
}
```

**What this tests:**
- Config with circular aliases (`a → b → a`) is rejected
- Server won't start with invalid config

### Test 5: Alias + Fallback Combination

```rust
#[test]
fn test_routing_result_with_alias_and_fallback() {
    // Only "fallback" model exists
    let registry = Arc::new(Registry::new());
    registry.add_backend(create_test_backend(
        "backend_fallback", "Backend Fallback", "fallback", 1,
    )).unwrap();

    // Alias: alias → primary
    // Fallback: primary → [fallback]
    let mut config = NexusConfig::default();
    config.routing.aliases.insert(
        "alias".to_string(), 
        "primary".to_string()
    );
    config.routing.fallbacks.insert(
        "primary".to_string(), 
        vec!["fallback".to_string()]
    );
    
    let state = AppState::new(registry, Arc::new(config));

    let request = ChatCompletionRequest {
        model: "alias".to_string(),
        // ...
    };

    let requirements = RequestRequirements::from_request(&request);
    let result = state.router.select_backend(&requirements).unwrap();

    // Alias resolved → primary not found → fallback used
    assert!(result.fallback_used);
    assert_eq!(result.actual_model, "fallback");
}
```

**Resolution flow:**

```
1. Request: "alias"
2. Alias lookup: alias → primary
3. Backend lookup: "primary" not found
4. Fallback lookup: primary → [fallback]
5. Backend lookup: "fallback" found ✓
6. Result: fallback_used = true
```

---

## Key Rust Concepts

| Concept | What It Means | Example in This Code |
|---------|---------------|----------------------|
| `HashMap<K, V>` | Key-value storage with O(1) lookup | `aliases: HashMap<String, String>` |
| `HashSet<T>` | Collection of unique values | `visited` in circular detection |
| `Option<T>` | Value that may or may not exist | `aliases.get(&current)` returns `Option<&String>` |
| `Result<T, E>` | Success or error | `validate_aliases()` returns `Result<(), ConfigError>` |
| `match` | Pattern matching | Handling `Some(target)` vs `None` |
| `while let` | Loop while pattern matches | `while let Some(target) = aliases.get(...)` |
| `clone()` | Create owned copy of data | `target.clone()` to get owned `String` |
| `tracing::debug!` | Structured logging | Logs alias resolution steps |

### HashMap Lookup Pattern

```rust
// Pattern 1: Get reference (returns Option<&V>)
match self.aliases.get(&current) {
    Some(target) => { /* use target */ }
    None => { /* not found */ }
}

// Pattern 2: Check existence
if visited.contains(target) {
    // target is in the set
}

// Pattern 3: Insert
visited.insert(start);
```

### The while let Pattern

```rust
// This loop continues as long as aliases.get() returns Some
while let Some(target) = aliases.get(current) {
    // Process target
    current = target;
}
// Loop exits when aliases.get() returns None
```

Equivalent to:
```rust
loop {
    match aliases.get(current) {
        Some(target) => {
            current = target;
        }
        None => break,
    }
}
```

---

## Common Patterns in This Codebase

### Pattern 1: Config Validation at Load Time

```rust
// In config/mod.rs
impl NexusConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_aliases(&self.routing.aliases)?;
        // ... other validations
        Ok(())
    }
}
```

**Why:** Fail fast at startup, not during user requests.

### Pattern 2: Depth-Limited Traversal

```rust
const MAX_DEPTH: usize = 3;
let mut depth = 0;

while depth < MAX_DEPTH {
    // Do work
    depth += 1;
}
```

**Why:** Prevent runaway recursion/loops.

### Pattern 3: Debug Logging with Structured Fields

```rust
tracing::debug!(
    from = %current,
    to = %target,
    depth = depth + 1,
    "Resolved alias"
);
```

**Output:**
```
DEBUG routing: Resolved alias from="gpt-4" to="llama-large" depth=1
```

**Why:** Structured logs are searchable and parseable.

### Pattern 4: Returning Owned vs Borrowed

```rust
// Returns owned String (caller owns the data)
fn resolve_alias(&self, model: &str) -> String {
    let mut current = model.to_string();  // Convert &str to owned String
    // ... modifications ...
    current  // Return owned String
}
```

**Why:** The function may modify the value through alias resolution, so it returns an owned `String` rather than a reference.

---

## Next Steps

Now that you understand Model Aliases, explore:

1. **Fallback Chains** (`specs/008-fallback-chains/`) - What happens when the resolved model isn't available
2. **Intelligent Router** (`src/routing/scoring.rs`) - How backends are scored and selected
3. **API Layer** (`src/api/`) - How requests flow from HTTP to router

### Try It Yourself

1. Add a new alias to `nexus.example.toml`:
   ```toml
   [routing.aliases]
   "my-model" = "llama3:8b"
   ```

2. Run with debug logging:
   ```bash
   RUST_LOG=debug cargo run -- serve
   ```

3. Make a request and watch the alias resolution in logs:
   ```bash
   curl -X POST http://localhost:8000/v1/chat/completions \
     -H "Content-Type: application/json" \
     -d '{"model": "my-model", "messages": [{"role": "user", "content": "Hi"}]}'
   ```

4. Try creating a circular alias and see the config validation error!
