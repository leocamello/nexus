# Research: Model Aliases (F07)

**Date**: 2026-02-08
**Status**: Implemented (PR #94)

This document captures the technical decisions made during F07 implementation, alternatives considered, and rationale for each choice.

## Research Questions & Findings

### 1. Alias Storage Data Structure

**Question**: How should model aliases be stored and accessed?

**Decision**: Use `HashMap<String, String>` in the `Router` struct, populated from TOML config at startup.

**Rationale**:
- Aliases are a simple one-to-one mapping: alias name → target model name
- `HashMap` provides O(1) lookup — critical since alias resolution runs on every request
- Immutable after construction — no need for concurrent write access (`DashMap` would be over-engineering)
- Populated from `[routing.aliases]` TOML section during Router construction
- Small dataset (typically < 20 entries) — HashMap overhead is negligible

**Implementation**:
```rust
pub struct Router {
    // ...
    aliases: HashMap<String, String>,
    // ...
}

// Config (nexus.toml):
// [routing.aliases]
// "gpt-4" = "llama3:70b"
// "gpt-4-turbo" = "llama3:70b"
// "gpt-3.5-turbo" = "mistral:7b"
```

**Alternatives Considered**:
- **`DashMap<String, String>`**: Rejected — aliases are immutable after startup; concurrent write support adds unnecessary overhead
- **`Vec<(String, String)>` with linear scan**: Rejected — O(n) lookup per request; unacceptable for the hot path even with small datasets
- **Trie/prefix tree**: Rejected — over-engineering for < 20 entries; HashMap is simpler and equally fast for exact key lookup
- **Database/external store**: Rejected — violates single-binary and stateless principles; aliases are configuration, not runtime state

---

### 2. Alias Chaining Depth Limit

**Question**: Should aliases be allowed to chain (A → B → C), and if so, how deep?

**Decision**: Allow chaining up to 3 levels maximum. Resolved iteratively with a depth counter.

**Rationale**:
- Chaining enables layered abstractions: `"gpt-4"` → `"llama-large"` → `"llama3:70b"`
- 3 levels covers all practical use cases (user-facing name → team alias → actual model)
- Deeper chains suggest misconfiguration — better to stop and use the last resolved name
- Iterative loop prevents stack overflow risk that recursion would introduce
- Constant `MAX_DEPTH = 3` is explicit and easy to reason about

**Implementation**:
```rust
fn resolve_alias(&self, model: &str) -> String {
    let mut current = model.to_string();
    let mut depth = 0;
    const MAX_DEPTH: usize = 3;

    while depth < MAX_DEPTH {
        match self.aliases.get(&current) {
            Some(target) => {
                tracing::debug!(from = %current, to = %target, depth = depth + 1, "Resolved alias");
                current = target.clone();
                depth += 1;
            }
            None => break,
        }
    }
    current
}
```

**Alternatives Considered**:
- **No chaining (single-level only)**: Rejected — prevents useful layered abstractions; limits expressive power without significant simplification
- **Unlimited chaining**: Rejected — circular aliases would cause infinite loops; even without cycles, deeply nested chains indicate misconfiguration
- **5 or 10 levels**: Rejected — no practical use case requires more than 3; lower limit catches misconfigurations faster
- **Recursive resolution**: Rejected — stack overflow risk on deep/circular chains; iterative loop is safer and equally readable

---

### 3. Circular Alias Detection Algorithm

**Question**: How do we detect and prevent circular alias chains (A → B → C → A)?

**Decision**: Validate at config load time using a `HashSet<&String>` visited set. Walk each alias chain and check for revisited nodes. Fail with `ConfigError::CircularAlias` if detected.

**Rationale**:
- Config-time validation is fail-fast — circular aliases are caught before any request is processed
- O(n) per chain, O(n²) worst case for all chains — runs once at startup, not per-request
- `HashSet` membership check is O(1), making cycle detection efficient
- Error includes both the starting alias and the cycle point for actionable diagnostics
- Runtime resolution doesn't need cycle detection because config validation already guarantees acyclicity

**Implementation**:
```rust
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

**Alternatives Considered**:
- **Runtime cycle detection per request**: Rejected — adds overhead to every request; cycles are a config problem, not a runtime problem
- **Topological sort**: Rejected — more complex implementation for the same result; cycle detection with visited set is simpler and sufficient
- **Ignore cycles (let MAX_DEPTH handle it)**: Rejected — silent truncation at depth 3 would mask misconfiguration; explicit error is better
- **Graph library (petgraph)**: Rejected — adding a dependency for a 15-line algorithm is unnecessary

---

### 4. Alias Resolution Logging Strategy

**Question**: At what log level should alias resolution be logged?

**Decision**: Use `DEBUG` level for individual resolution steps and chain completion. No logging at `INFO` or higher for successful resolutions.

**Rationale**:
- Alias resolution happens on every request — `INFO` logging would be too noisy in production
- `DEBUG` level allows operators to trace alias chains when troubleshooting
- Two log points: per-hop resolution (`from → to, depth`) and chain completion (`original → resolved, chain_depth`)
- No log emitted when alias lookup misses (model name isn't an alias) — this is the common case

**Implementation**:
```rust
// Per-hop (only when alias found):
tracing::debug!(from = %current, to = %target, depth = depth + 1, "Resolved alias");

// Chain completion (only when at least one alias resolved):
if depth > 0 {
    tracing::debug!(original = %model, resolved = %current, chain_depth = depth,
        "Alias resolution complete");
}
```

**Alternatives Considered**:
- **INFO level**: Rejected — too noisy for production; alias resolution is routine, not noteworthy
- **TRACE level**: Rejected — too low; operators debugging routing issues need alias visibility without full trace output
- **No logging**: Rejected — alias resolution is invisible without logs; debugging "why did my request go to model X?" requires visibility into the resolution chain
- **Metrics instead of logs**: Rejected — metrics track counts, not individual resolution paths; logs are needed for per-request debugging

---

### 5. Alias Resolution Position in Routing Pipeline

**Question**: When in the routing pipeline should aliases be resolved?

**Decision**: Resolve aliases first, before any other routing logic (model lookup, capability filtering, fallback chains).

**Rationale**:
- Aliases are a naming indirection — they should be transparent to the rest of the routing system
- Resolving first means fallback chains can reference the resolved model name, enabling `alias → model → fallback` flows
- The resolved model name is stored in `RoutingResult.actual_model` for API layer use
- Single resolution point prevents confusion about whether to define fallbacks for alias names or target names

**Implementation**:
```rust
pub fn select_backend(&self, requirements: &RequestRequirements) -> Result<RoutingResult, _> {
    // Step 1: Resolve alias (always first)
    let model = self.resolve_alias(&requirements.model);

    // Step 2: Find candidates for resolved model
    let candidates = self.filter_candidates(&model, requirements);

    // Step 3: If no candidates, try fallback chain (uses resolved model name)
    let fallbacks = self.get_fallbacks(&model);
    // ...
}
```

**Alternatives Considered**:
- **Resolve after fallback lookup**: Rejected — would require defining fallbacks for alias names rather than actual model names, creating a confusing mapping layer
- **Resolve in the API layer**: Rejected — routing logic should be encapsulated in the Router; the API layer shouldn't know about aliases
- **Lazy resolution (only when needed)**: Rejected — adds complexity with no benefit; resolution is O(1) per hop and runs at most 3 times

---

### 6. Config-Time Validation Strategy

**Question**: When and how should alias configuration be validated?

**Decision**: Validate aliases during config loading via `validate_aliases()`. Circular references produce `ConfigError::CircularAlias` and prevent Nexus from starting.

**Rationale**:
- Fail-fast at startup catches misconfiguration before any requests are handled
- `ConfigError::CircularAlias` includes start and cycle point for clear error messages
- Validation runs once — no per-request cost
- Chaining depth isn't validated at config time (handled by `MAX_DEPTH` at runtime) because deep-but-acyclic chains are valid, just unusual

**Alternatives Considered**:
- **Runtime validation on first use**: Rejected — delays error discovery; a misconfigured alias might not be hit for hours
- **Warn but continue**: Rejected — circular aliases cause silent resolution truncation; better to fail explicitly
- **Validate in Router constructor**: Rejected — validation is a config concern; `validate_aliases()` lives in `config/routing.rs` where it belongs

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Circular alias causes infinite loop | High | Config-time validation with `HashSet` visited set |
| Deep chain silently truncates | Low | MAX_DEPTH=3 covers practical use cases; DEBUG logging shows chain |
| Alias target doesn't exist | Low | Results in `ModelNotFound` error, same as requesting a nonexistent model directly |
| Alias map grows very large | Low | HashMap handles thousands of entries; practical configs have < 20 |
| Alias + fallback interaction confusion | Medium | Documented: aliases resolve first, then fallbacks apply to resolved name |

---

## References

- [TOML inline table syntax for aliases](https://toml.io/en/v1.0.0#inline-table)
- [Nexus LEARNINGS.md - F07 section](../../docs/LEARNINGS.md)
- [OpenAI model naming conventions](https://platform.openai.com/docs/models)
