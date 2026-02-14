# Data Model: Model Aliases (F07)

**Date**: 2025-01-10  
**Phase**: Phase 1 - Design & Contracts

This document defines the data entities and their relationships for the Model Aliases feature.

## Core Entities

### 1. Alias Map

**Purpose**: A mapping from familiar model names (e.g., `"gpt-4"`) to actual local model names (e.g., `"llama3:70b"`), enabling drop-in OpenAI client compatibility.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `aliases` | `HashMap<String, String>` | Stored on `Router`; keys are alias names, values are target model names |

**Entry Constraints**:

| Constraint | Rule |
|------------|------|
| Key uniqueness | HashMap enforces; last config entry wins |
| Circular references | Validated at config load by `validate_aliases()` |
| Chain depth | Max 3 levels at runtime (resolve stops at depth 3) |
| Case sensitivity | Keys are case-sensitive (exact match) |
| Empty values | Ignored implicitly (empty string won't match any model) |

**Responsibilities**:
- Map client-facing model names to actual backend model names
- Support chained aliases (e.g., `"gpt-4"` → `"llama-large"` → `"llama3:70b"`)
- Provide O(1) lookup per resolution level

**Lifecycle**: Loaded from `[routing.aliases]` config section at startup. Immutable after `Router` construction. Requires server restart to modify.

**Thread Safety**: Immutable `HashMap` on `Router`; safe for concurrent reads without synchronization.

---

### 2. Alias Resolution Function

**Purpose**: Resolves a model name through the alias chain, returning the final target model name.

**Signature**:
```rust
fn resolve_alias(&self, model: &str) -> String
```

**Attributes**:

| Parameter | Type | Constraints |
|-----------|------|-------------|
| `model` (input) | `&str` | The model name from the client request |
| return value | `String` | The resolved model name (may be unchanged if no alias) |
| `MAX_DEPTH` | `usize` (const) | `3`; maximum chain levels |

**Algorithm**:
1. Set `current = model`, `depth = 0`
2. While `depth < 3`:
   - Look up `current` in `aliases` HashMap
   - If found: log at DEBUG level, set `current = target`, increment `depth`
   - If not found: break
3. If `depth > 0`: log final resolution at DEBUG level
4. Return `current`

**Responsibilities**:
- Chain through up to 3 alias levels
- Log each alias hop at DEBUG level with `from`, `to`, and `depth` fields
- Log final resolution with `original`, `resolved`, and `chain_depth` fields
- Return input unchanged if no alias exists

**Lifecycle**: Called per-request as the first step of `select_backend()`.

**Thread Safety**: Reads only from immutable `HashMap`; no mutation, no synchronization needed.

---

### 3. Circular Alias Validation

**Purpose**: Detects circular alias references at config load time to prevent infinite loops at runtime.

**Signature**:
```rust
pub fn validate_aliases(aliases: &HashMap<String, String>) -> Result<(), ConfigError>
```

**Attributes**:

| Parameter | Type | Constraints |
|-----------|------|-------------|
| `aliases` (input) | `&HashMap<String, String>` | The alias map to validate |
| return value | `Result<(), ConfigError>` | `Ok(())` if valid; `Err(CircularAlias)` if cycle detected |

**Algorithm**:
1. For each key `start` in aliases:
   - Set `current = start`, create `visited = {start}`
   - While `current` has a target in aliases:
     - If target is in `visited`: return `CircularAlias { start, cycle: target }`
     - Add `current` to visited, advance `current = target`
2. Return `Ok(())`

**Detected Patterns**:

| Pattern | Example | Detection |
|---------|---------|-----------|
| Self-referential | `"a" → "a"` | `start="a"`, `cycle="a"` |
| Direct circular | `"a" → "b"`, `"b" → "a"` | `start="a"`, `cycle="a"` |
| Three-way circular | `"a" → "b"`, `"b" → "c"`, `"c" → "a"` | `start="a"`, `cycle="a"` |
| Valid chain | `"a" → "b"`, `"b" → "c"` | No error |

**Lifecycle**: Called once during config loading. If validation fails, the server does not start.

**Thread Safety**: Pure function operating on immutable reference; no shared state.

---

### 4. ConfigError::CircularAlias

**Purpose**: Error variant indicating a circular alias was detected during config validation.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `start` | `String` | The alias key where cycle detection started |
| `cycle` | `String` | The alias target that was already visited |

**Error Message**: `"Circular alias detected: '{start}' eventually points back to '{cycle}'"`

**Lifecycle**: Created by `validate_aliases()`, propagated to startup error handler. Prevents server start.

---

## Entity Relationships

```
┌──────────────────────────────┐
│     Config File (TOML)       │
│                              │
│  [routing.aliases]           │
│  "gpt-4" = "llama3:70b"     │
│  "gpt-4-turbo" = "llama3:70b│
│  "fast" = "mistral:7b"      │
└──────────────────────────────┘
            │
            │ parsed into
            ▼
┌──────────────────────────────┐
│  RoutingConfig.aliases       │
│  HashMap<String, String>     │
│                              │
│  ┌────────┬───────────────┐  │
│  │ Key    │ Value         │  │
│  ├────────┼───────────────┤  │
│  │ gpt-4  │ llama3:70b    │  │
│  │ fast   │ mistral:7b    │  │
│  └────────┴───────────────┘  │
└──────────────────────────────┘
            │
            │ validate_aliases()
            │ (rejects circular refs)
            ▼
┌──────────────────────────────┐
│          Router              │
│                              │
│  aliases: HashMap            │──── resolve_alias("gpt-4")
│                              │          │
│  select_backend()            │          │ chain resolution
│    1. resolve_alias()        │          ▼
│    2. filter_candidates()    │     "gpt-4" → "llama3:70b"
│    3. apply strategy         │     (depth=1)
│    4. try fallbacks          │
└──────────────────────────────┘
            │
            │ resolved model name
            ▼
┌──────────────────────────────┐
│        Registry              │
│                              │
│  get_backends_for_model(     │
│    "llama3:70b"              │
│  )                           │
└──────────────────────────────┘
```

---

## State Transitions

### Alias Resolution States

```
                    ┌────────────────────────┐
                    │  Input: "gpt-4"        │
                    └────────────────────────┘
                                │
                                │ lookup in aliases
                                ▼
                    ┌────────────────────────┐
              ┌─────│ Found in aliases?      │─────┐
              │     └────────────────────────┘     │
              │ Yes                                │ No
              ▼                                    ▼
┌────────────────────────┐          ┌────────────────────────┐
│ current = target       │          │ Return current         │
│ depth++                │          │ (no alias applied)     │
│ Log DEBUG: from → to   │          └────────────────────────┘
└────────────────────────┘
              │
              │ depth < 3?
              ▼
        ┌───────────┐
   Yes  │           │  No (max depth)
   ┌────┤  depth<3  ├────┐
   │    │           │    │
   │    └───────────┘    │
   │                     ▼
   │         ┌────────────────────────┐
   │         │ Return current         │
   │         │ (chain stopped at 3)   │
   │         └────────────────────────┘
   │
   ▼
   (loop back to alias lookup)
```

### Config Loading with Alias Validation

```
Config parsed from TOML
    ↓
validate_aliases(aliases)
    ↓
┌──────────────────────┐
│ Circular detected?   │
│                      │
│ No  → Continue       │──▶ Router created with aliases
│                      │
│ Yes → ConfigError    │──▶ Server fails to start
│       CircularAlias  │    with error message
└──────────────────────┘
```

---

## Validation & Constraints

### Alias Map Constraints

**Rule**: No circular references allowed. Validated exhaustively at config load.

**Implementation**: `validate_aliases()` in `src/config/routing.rs` walks the full chain for every alias key, using a `HashSet<&String>` to track visited nodes.

### Chain Depth Limit

**Rule**: Alias resolution stops after 3 levels regardless of further aliases.

**Implementation**: `resolve_alias()` uses a `while depth < MAX_DEPTH` loop with `MAX_DEPTH = 3`. If chain exceeds 3 levels, the current resolved value is returned without error.

### Direct Match Priority

**Rule**: Direct model matches in the registry take priority over aliases. The router first resolves aliases, then looks up the resolved name in the registry. If a backend has a model named `"gpt-4"` directly, the alias `"gpt-4" → "llama3:70b"` is still applied — the alias always fires. To use the direct model, do not configure an alias for it.

**Implementation**: `resolve_alias()` is called unconditionally before registry lookup. Alias resolution does not check whether the original name exists as a real model.

### Alias + Fallback Interaction

**Rule**: After alias resolution, if no backend has the resolved model, the fallback chain for the **resolved** model name is checked (not the original alias name).

**Example**:
- Alias: `"gpt-4"` → `"llama3:70b"`
- Fallback: `"llama3:70b"` → `["mistral:7b"]`
- Request for `"gpt-4"` → resolves to `"llama3:70b"` → no backends → falls back to `"mistral:7b"`

---

## Thread Safety

**Requirement**: Alias resolution must be lock-free and safe for concurrent access.

**Implementation**:
- `aliases` is an immutable `HashMap<String, String>` on the `Router` struct
- `resolve_alias()` performs only `HashMap::get()` calls (read-only)
- No `Mutex`, `RwLock`, or atomic operations needed
- Multiple threads can resolve aliases concurrently without contention

---

## Performance Characteristics

| Operation | Target Latency | Implementation |
|-----------|----------------|----------------|
| Single alias lookup | < 50ns | `HashMap::get()` — O(1) amortized |
| Full chain resolution (1 level) | < 100ns | 1 lookup + log check |
| Full chain resolution (3 levels) | < 300ns | 3 lookups + 3 DEBUG logs |
| Circular validation (startup) | < 1ms | O(n × m) where n=aliases, m=avg chain length |
| Memory per alias entry | ~100 bytes | Two `String` allocations (key + value) |
| Max aliases supported | 10,000+ | HashMap scales linearly in memory |

**Total Alias Overhead Per Request**: < 300ns (negligible compared to 1ms routing budget).

---

## Testing Strategy

### Unit Tests

1. **Basic alias resolution**: Single-level alias returns target model
2. **Alias chaining (2 levels)**: `"a" → "b" → "c"` resolves to `"c"`
3. **Alias chaining (3 levels)**: Max depth resolves correctly
4. **Chain exceeds max depth**: Stops at level 3, returns current value
5. **No alias (passthrough)**: Unknown model returned unchanged
6. **DEBUG logging**: Verify log output at each chain level

### Config Validation Tests

1. **Self-referential detection**: `"a" → "a"` returns `CircularAlias` error
2. **Direct circular detection**: `"a" → "b"`, `"b" → "a"` returns error
3. **Three-way circular detection**: `"a" → "b" → "c" → "a"` returns error
4. **Valid chain accepted**: `"a" → "b" → "c"` passes validation
5. **Empty aliases accepted**: No aliases is valid
6. **Disconnected aliases accepted**: `"a" → "b"`, `"c" → "d"` passes

### Integration Tests

1. **End-to-end routing with alias**: Request `"gpt-4"` routes to `"llama3:70b"` backend
2. **Alias + fallback combination**: Alias resolves, primary unavailable, fallback used
3. **Alias target not found**: Returns `ModelNotFound` with original requested name

---

## Future Extensions

### Not in Scope

1. **Dynamic alias management via API**: Aliases are config-file only
2. **Per-client aliases**: All clients share the same alias map
3. **Wildcard or regex aliases**: Only exact string matching
4. **Case-insensitive aliases**: Keys are case-sensitive
5. **Environment variable override for aliases**: Not supported (use config file)
