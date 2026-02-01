# Backend Registry - Code Walkthrough

**Feature**: F02 - Backend Registry  
**Audience**: Junior developers joining the project  
**Last Updated**: 2026-02-01

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: lib.rs - The Front Door](#file-1-librs---the-front-door)
4. [File 2: error.rs - What Can Go Wrong](#file-2-errorrs---what-can-go-wrong)
5. [File 3: backend.rs - The Data Structures](#file-3-backendrs---the-data-structures)
6. [File 4: mod.rs - The Registry Operations](#file-4-modrs---the-registry-operations)
7. [Understanding the Tests](#understanding-the-tests)
8. [Key Rust Concepts](#key-rust-concepts)
9. [Common Patterns in This Codebase](#common-patterns-in-this-codebase)

---

## The Big Picture

Think of the Backend Registry as a **phone book for AI servers**. When Nexus receives a request like "chat with llama3", it needs to know:
- Which servers have that model?
- Are they healthy?
- How busy are they?

The Registry answers all these questions.

### How It Fits in Nexus

```
┌─────────────────────────────────────────────────────────────────┐
│                         Nexus                                   │
│                                                                 │
│  ┌──────────┐     ┌──────────┐     ┌──────────────────────┐    │
│  │   API    │────▶│  Router  │────▶│  Backend Registry    │    │
│  │ Gateway  │     │          │     │  (you are here!)     │    │
│  └──────────┘     └──────────┘     └──────────────────────┘    │
│                                              │                  │
│                         ┌────────────────────┘                  │
│                         ▼                                       │
│                  ┌──────────────┐                               │
│                  │Health Checker│                               │
│                  │ (updates     │                               │
│                  │  registry)   │                               │
│                  └──────────────┘                               │
└─────────────────────────────────────────────────────────────────┘
```

---

## File Structure

```
src/
├── lib.rs                  # Library entry point
└── registry/
    ├── mod.rs              # Registry struct and all operations
    ├── backend.rs          # Backend, Model, BackendView structs
    ├── error.rs            # RegistryError enum
    └── tests.rs            # 54 unit tests + 4 property tests
```

---

## File 1: lib.rs - The Front Door

```rust
//! Nexus - Distributed LLM model serving orchestrator
//!
//! This library provides the core functionality for managing and routing requests
//! to heterogeneous LLM inference backends.

pub mod registry;
```

**What's happening here:**
- `//!` comments are **documentation** for the whole module
- `pub mod registry;` says "there's a folder called `registry/` and it's public"

This is the entry point when someone writes `use nexus::registry::*;`

---

## File 2: error.rs - What Can Go Wrong

```rust
/// Errors that can occur during registry operations
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("backend already exists: {0}")]
    DuplicateBackend(String),

    #[error("backend not found: {0}")]
    BackendNotFound(String),
}
```

**Breaking it down:**

| Part | Meaning |
|------|---------|
| `#[derive(Debug, thiserror::Error)]` | Auto-generate Debug and Error implementations |
| `pub enum RegistryError` | A public enum listing all possible errors |
| `#[error("backend already exists: {0}")]` | The error message; `{0}` is filled with the String |
| `DuplicateBackend(String)` | Variant when adding a backend with existing ID |
| `BackendNotFound(String)` | Variant when accessing a backend that doesn't exist |

**Key Concept - Enums for Errors:** Instead of throwing generic exceptions like in other languages, Rust uses enums to list every possible error. This forces callers to handle each case explicitly.

---

## File 3: backend.rs - The Data Structures

### Part 1: The Enums

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendType {
    Ollama,
    VLLM,
    LlamaCpp,
    Exo,
    OpenAI,
    Generic,
}
```

**What the `#[derive(...)]` means:**

| Trait | What It Does |
|-------|--------------|
| `Debug` | You can print it with `{:?}` for debugging |
| `Clone, Copy` | You can make copies cheaply (it's just a number internally) |
| `PartialEq, Eq` | You can compare: `if backend_type == BackendType::Ollama` |
| `Serialize, Deserialize` | Can convert to/from JSON |

**What `#[serde(rename_all = "lowercase")]` means:**
- When converting to JSON, `Ollama` becomes `"ollama"` (lowercase)

### Part 2: The Model struct

```rust
pub struct Model {
    pub id: String,              // "llama3:70b"
    pub context_length: u32,     // 8192 tokens
    pub supports_vision: bool,   // Can it see images?
    pub supports_tools: bool,    // Can it call functions?
    pub supports_json_mode: bool,
    pub max_output_tokens: Option<u32>,
}
```

This describes what a model can do. When someone asks "I need a model with vision support and 32K context", the Router can filter models by these fields.

### Part 3: The Backend struct

```rust
pub struct Backend {
    // Regular fields (simple data)
    pub id: String,
    pub url: String,
    pub status: BackendStatus,
    pub models: Vec<Model>,
    
    // Atomic fields (thread-safe counters)
    pub pending_requests: AtomicU32,   // In-flight requests RIGHT NOW
    pub total_requests: AtomicU64,     // Lifetime total
    pub avg_latency_ms: AtomicU32,     // Average response time (EMA)
}
```

**Why Atomic?** 

Imagine 100 requests hitting Nexus at once. 100 threads might try to increment `pending_requests` simultaneously. 

With regular `u32`:
```
Thread A reads: 5
Thread B reads: 5
Thread A writes: 6
Thread B writes: 6  // WRONG! Should be 7
```

With `AtomicU32`:
```
Thread A: atomic_add(1) → returns 5, value is now 6
Thread B: atomic_add(1) → returns 6, value is now 7  // CORRECT!
```

### Part 4: BackendView (for JSON serialization)

```rust
pub struct BackendView {
    pub pending_requests: u32,  // Regular u32, NOT Atomic
    // ... all fields as regular types
}

impl From<&Backend> for BackendView {
    fn from(backend: &Backend) -> Self {
        Self {
            pending_requests: backend.pending_requests.load(Ordering::SeqCst),
            // ...
        }
    }
}
```

**Why do we need this?** 

Atomic types can't be serialized to JSON directly. So when we need to return JSON:
1. Convert `Backend` → `BackendView` (reads the atomic values)
2. Serialize `BackendView` to JSON

---

## File 4: mod.rs - The Registry Operations

### The Registry struct

```rust
pub struct Registry {
    backends: DashMap<String, Backend>,        // id -> Backend
    model_index: DashMap<String, Vec<String>>, // model_id -> [backend_ids]
}
```

**What's DashMap?** 

It's like a `HashMap` but thread-safe. Multiple threads can read/write simultaneously without explicit locks.

**Why two maps?**
- `backends`: The main storage - find a backend by its ID
- `model_index`: A shortcut - "which backends have model X?" 

Without `model_index`, we'd have to scan ALL backends to find ones with a model. With it, it's instant (O(1)).

### Adding a Backend

```rust
pub fn add_backend(&self, backend: Backend) -> Result<(), RegistryError> {
    let id = backend.id.clone();

    // Step 1: Check for duplicate
    if self.backends.contains_key(&id) {
        return Err(RegistryError::DuplicateBackend(id));
    }

    // Step 2: Update model index
    for model in &backend.models {
        self.model_index
            .entry(model.id.clone())  // Get or create entry for this model
            .or_default()              // If new, start with empty Vec
            .push(id.clone());         // Add this backend's ID
    }

    // Step 3: Insert backend
    self.backends.insert(id, backend);
    Ok(())
}
```

### The Atomic Operations

**Increment Pending (simple):**
```rust
pub fn increment_pending(&self, id: &str) -> Result<u32, RegistryError> {
    let backend = self.backends.get(id).ok_or_else(/* error */)?;
    
    let new_val = backend.pending_requests.fetch_add(1, Ordering::SeqCst) + 1;
    //            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    //            "Add 1 atomically and return the OLD value"
    //            So we add 1 to get the new value
    
    Ok(new_val)
}
```

**Decrement Pending (uses compare-exchange):**
```rust
pub fn decrement_pending(&self, id: &str) -> Result<u32, RegistryError> {
    let backend = self.backends.get(id).ok_or_else(/* error */)?;

    loop {  // Keep trying until we succeed
        let current = backend.pending_requests.load(Ordering::SeqCst);
        
        if current == 0 {
            tracing::warn!("Attempted to decrement when already at 0");
            return Ok(0);  // Can't go negative!
        }

        let new_val = current - 1;
        
        // Try to update: "If the value is still `current`, change it to `new_val`"
        match backend.pending_requests.compare_exchange(current, new_val, ...) {
            Ok(_) => return Ok(new_val),  // Success!
            Err(_) => continue,            // Another thread changed it, try again
        }
    }
}
```

**Why the loop?** 

Imagine:
1. Thread A reads `current = 5`
2. Thread B reads `current = 5`
3. Thread A sets it to 4 ✓
4. Thread B tries to set 5→4, but it's already 4! The compare fails.
5. Thread B loops, reads 4, sets to 3 ✓

This guarantees correctness without locks.

### The EMA Latency Update

```rust
// EMA formula: new = (sample + 4*old) / 5
// This is α=0.2 exponential moving average using integer math
let new_val = (latency_ms + 4 * current) / 5;
```

**What's EMA?** 

Exponential Moving Average - a way to track "average" that gives more weight to recent values.

**Example:** If current average is 100ms and new sample is 50ms:
```
new = (50 + 4*100) / 5 = (50 + 400) / 5 = 450 / 5 = 90ms
```

The average moves toward the new sample, but slowly (20% of the way).

---

## Understanding the Tests

### Test Categories

| Category | Count | Purpose |
|----------|-------|---------|
| Serialization | 5 | Verify JSON round-trip works |
| CRUD Operations | 9 | Add, remove, get backends |
| Model Index | 9 | Query backends by model |
| Status Updates | 8 | Health status changes |
| Atomic Counters | 8 | Thread-safe counter operations |
| Property Tests | 4 | Random inputs (proptest) |
| Stress Tests | 4 | Concurrent access (10K operations) |

### Example: Basic Test

```rust
#[test]
fn test_add_backend_success() {
    // ARRANGE: Create a registry and a backend
    let registry = Registry::new();
    let backend = Backend::new(
        "backend-1".to_string(),
        "Backend 1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );

    // ACT: Add the backend
    let result = registry.add_backend(backend);

    // ASSERT: It should succeed
    assert!(result.is_ok());
    assert_eq!(registry.backend_count(), 1);

    // ASSERT: We can retrieve it
    let retrieved = registry.get_backend("backend-1");
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().id, "backend-1");
}
```

**The Pattern:** Arrange → Act → Assert

### Example: Error Test

```rust
#[test]
fn test_add_backend_duplicate_error() {
    let registry = Registry::new();

    // Add first backend
    let backend1 = Backend::new("backend-1".to_string(), /* ... */);
    registry.add_backend(backend1).unwrap();

    // Try to add duplicate
    let backend2 = Backend::new("backend-1".to_string(), /* ... */);
    let result = registry.add_backend(backend2);

    // Should fail with specific error
    assert!(result.is_err());
    match result {
        Err(RegistryError::DuplicateBackend(id)) => assert_eq!(id, "backend-1"),
        _ => panic!("Expected DuplicateBackend error"),
    }
}
```

**Key Pattern:** `match` forces you to handle specific error variants.

### Example: Property-Based Test

```rust
proptest! {
    #[test]
    fn prop_increment_decrement_balanced(n in 1u32..100) {
        // For ANY n between 1 and 100...
        let registry = Registry::new();
        let backend = Backend::new("backend-1".to_string(), /* ... */);
        registry.add_backend(backend).unwrap();

        // Increment n times
        for _ in 0..n {
            registry.increment_pending("backend-1").unwrap();
        }

        // Decrement n times
        for _ in 0..n {
            registry.decrement_pending("backend-1").unwrap();
        }

        // Should ALWAYS be back at 0
        let backend = registry.get_backend("backend-1").unwrap();
        prop_assert_eq!(
            backend.pending_requests.load(Ordering::SeqCst), 
            0
        );
    }
}
```

**Why Property Tests?**

Instead of testing with one value (e.g., `n = 5`), proptest runs with MANY random values. If any fail, it finds the smallest failing case.

### Example: Concurrent Stress Test

```rust
#[tokio::test]
async fn test_concurrent_reads_no_deadlock() {
    let registry = Arc::new(Registry::new());
    
    // Add a backend
    registry.add_backend(/* ... */).unwrap();

    // Spawn 10,000 concurrent reads
    let mut handles = vec![];
    for _ in 0..10_000 {
        let reg = Arc::clone(&registry);
        let handle = tokio::spawn(async move { 
            reg.get_backend("backend-1") 
        });
        handles.push(handle);
    }

    // All reads should complete within 5 seconds
    let result = timeout(Duration::from_secs(5), async {
        for handle in handles {
            handle.await.unwrap();
        }
    }).await;

    assert!(result.is_ok(), "Should complete without deadlock");
}
```

**What This Tests:**

- `Arc::new()` - Shared ownership across threads
- `tokio::spawn()` - Async task that runs concurrently
- `timeout()` - Fails if deadlock occurs (tasks never complete)

---

## Key Rust Concepts

| Concept | What It Means |
|---------|---------------|
| `Result<T, E>` | Either success (`Ok(T)`) or error (`Err(E)`) |
| `Option<T>` | Either `Some(value)` or `None` (null-safe) |
| `DashMap` | Thread-safe HashMap (no explicit locks needed) |
| `AtomicU32` | Thread-safe counter that won't corrupt |
| `Ordering::SeqCst` | Strongest memory ordering - all threads see same order |
| `compare_exchange` | "If value is X, change to Y" - fails if changed |
| `Arc<T>` | Shared ownership across threads |
| `Clone` | Create a deep copy of data |
| `&self` | Immutable borrow (read-only access) |
| `&mut self` | Mutable borrow (read-write access) |

---

## Common Patterns in This Codebase

### Pattern 1: Error Handling with `?`

```rust
// Instead of:
let backend = match self.backends.get(id) {
    Some(b) => b,
    None => return Err(RegistryError::BackendNotFound(id.to_string())),
};

// We write:
let backend = self.backends
    .get(id)
    .ok_or_else(|| RegistryError::BackendNotFound(id.to_string()))?;
```

The `?` operator returns early if there's an error.

### Pattern 2: Builder-Style Method Chaining

```rust
self.model_index
    .entry(model.id.clone())  // Get or create entry
    .or_default()              // Initialize if new
    .push(id.clone());         // Modify the value
```

### Pattern 3: Iterator Transformations

```rust
self.backends
    .iter()                           // Iterate over all entries
    .filter(|e| e.value().status == BackendStatus::Healthy)  // Keep only healthy
    .map(|e| clone_backend(e.value()))  // Transform to new Backend
    .collect()                         // Collect into Vec
```

### Pattern 4: Atomic Compare-Exchange Loop

```rust
loop {
    let current = atomic.load(Ordering::SeqCst);
    let new_val = transform(current);
    
    match atomic.compare_exchange(current, new_val, ...) {
        Ok(_) => return Ok(new_val),  // Success!
        Err(_) => continue,            // Retry
    }
}
```

---

## Next Steps

Now that you understand the Backend Registry, explore:

1. **Health Checker** (`src/health/`) - Uses Registry to update backend status
2. **Router** (`src/routing/`) - Uses Registry to find backends for a model
3. **API Gateway** (`src/api/`) - Uses Router to handle HTTP requests

Each builds on the Registry as the source of truth for all backend state.
