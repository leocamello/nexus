# Research: Privacy Zones & Capability Tiers

**Feature Branch**: `015-privacy-zones`  
**Date**: 2025-02-16

## Phase 0: Technical Research & Decisions

### Overview

This document resolves all "NEEDS CLARIFICATION" items from the Technical Context and documents key design decisions for implementing Privacy Zones and Capability Tiers in Nexus.

---

## 1. Multi-Dimensional Capability Scoring

### Decision: Extend Backend Metadata Schema

**Chosen Approach**: Store capability scores in a structured HashMap in `Backend.metadata` with standardized keys.

**Implementation**:
```rust
// In BackendConfig (src/config/backend.rs)
pub struct CapabilityTier {
    pub reasoning: Option<u8>,    // 0-10 scale
    pub coding: Option<u8>,       // 0-10 scale
    pub context_window: Option<u32>, // tokens
    pub vision: bool,
    pub tools: bool,
}

// Serialized in TOML as:
// [backends.capability_tier]
// reasoning = 9
// coding = 8
// context_window = 128000
// vision = true
// tools = true
```

**Rationale**:
- Uses existing backend metadata pattern established in control plane (RFC-001)
- Allows optional capabilities (backends self-report what they support)
- Extensible: new capability dimensions can be added without breaking schema
- TOML-friendly: natural nested structure

**Alternatives Considered**:
1. **Single integer tier (0-4)**: Too coarse-grained, cannot distinguish reasoning vs. coding strengths
2. **Separate Tier table**: Over-engineered, adds unnecessary indirection
3. **JSON blob in metadata**: Type-unsafe, error-prone parsing

**Best Practices**:
- From LangChain's model registry: Multi-dimensional scoring enables better matching
- From Kubernetes resource requests: Optional fields with sensible defaults
- Scale to 0-10 for reasoning/coding matches common model benchmarks (HumanEval, MMLU)

---

## 2. TrafficPolicy Configuration Structure

### Decision: TOML Section-Based Policies with Pattern Matching

**Chosen Approach**: Add `[routing.policies]` sections to `nexus.toml` with glob pattern matching.

**Implementation**:
```toml
# In nexus.toml
[routing.policies."code-*"]
privacy = "restricted"
min_reasoning = 7
min_coding = 8
overflow_mode = "block-entirely"

[routing.policies."chat-*"]
privacy = "open"
min_reasoning = 5
overflow_mode = "fresh-only"

[routing.policies."vision-*"]
min_reasoning = 6
vision_required = true
```

**Rationale**:
- Follows existing TOML structure conventions (routing.aliases, routing.fallbacks)
- Pattern matching allows flexible route grouping without hardcoding
- Policies are optional: if no pattern matches, use backend defaults only
- Configuration hot-reload already supported by existing config loader

**Alternatives Considered**:
1. **Per-request headers**: Violates Principle IX (privacy is structural, not opt-in)
2. **Separate policy file**: Adds complexity, splits configuration
3. **Hardcoded policies in code**: Not flexible, requires recompilation

**Best Practices**:
- From Traefik: Pattern-based routing rules with priority ordering
- From Nginx: Declarative configuration over imperative code
- From Envoy: Policy evaluation at routing time, not request time

**Pattern Matching Algorithm**:
1. Sort policies by pattern specificity (exact match > glob > wildcard)
2. Apply first matching policy to request
3. If no match, use backend defaults only
4. Cache compiled glob patterns for performance

---

## 3. Request Header Handling (X-Nexus-Strict, X-Nexus-Flexible)

### Decision: Extract Headers in Completions Handler, Pass to RoutingIntent

**Chosen Approach**: Read headers early in request pipeline, store in `RequestRequirements`.

**Implementation**:
```rust
// In src/api/completions.rs
pub enum RoutingPreference {
    Strict,      // Default: only exact model
    Flexible,    // Allow tier-equivalent alternatives
}

impl RequestRequirements {
    pub fn from_request_with_headers(
        request: &ChatCompletionRequest,
        headers: &HeaderMap,
    ) -> Self {
        let routing_preference = if headers.get("x-nexus-flexible").is_some() {
            RoutingPreference::Flexible
        } else {
            RoutingPreference::Strict // Default
        };
        // ... rest of extraction
    }
}
```

**Rationale**:
- Headers checked once at API boundary (O(1)), not repeatedly in reconcilers
- Default to strict mode aligns with Principle IX (explicit contracts)
- Flexible mode still prevents downgrades (only allows lateral tier-equivalent substitution)
- Privacy zone enforcement is NEVER flexible (no header can override)

**Alternatives Considered**:
1. **Pass HeaderMap to reconciler pipeline**: Couples HTTP layer to control plane
2. **Extract in RoutingIntent constructor**: Mixes API concern with routing logic
3. **Make flexible the default**: Violates constitution (never surprise developers)

**Best Practices**:
- From HTTP proxies: Extract headers at edge, pass as structured data
- From Kubernetes admission controllers: Headers → request attributes → policy evaluation
- From AWS ALB: Request-scoped routing hints evaluated early

**Edge Cases**:
- Both headers present: X-Nexus-Strict takes precedence (fail-safe)
- Invalid header values: Ignore and use default (strict)
- Flexible mode with no tier-equivalent alternatives: Still return 503 (never downgrade)

---

## 4. Cross-Zone Overflow & Conversation History

### Decision: Block History Forwarding, Allow Fresh Conversations

**Chosen Approach**: Detect conversation history in request, reject overflow if history present.

**Implementation**:
```rust
// In PrivacyReconciler
fn has_conversation_history(request: &RequestRequirements) -> bool {
    // Check if messages array has > 1 message or has "assistant" role
    request.messages.len() > 1 || 
    request.messages.iter().any(|m| m.role == "assistant")
}

fn allows_cross_zone_overflow(&self, policy: &TrafficPolicy) -> bool {
    match policy.overflow_mode {
        OverflowMode::BlockEntirely => false,
        OverflowMode::FreshOnly => !self.has_conversation_history(request),
    }
}
```

**Rationale**:
- Simple heuristic: 1 user message = fresh, multiple messages or assistant role = history
- Aligns with Principle VIII (stateless by design: no KV-cache tracking)
- Fails safe: If unsure, block overflow to prevent accidental data leakage
- Documented limitation: Metadata (request timing, pattern) still visible to cloud backend

**Alternatives Considered**:
1. **Scrub history before overflow**: Complex, error-prone, trust boundary issues
2. **Track conversation IDs**: Violates stateless principle, requires persistence
3. **Always allow overflow**: Violates privacy guarantees

**Best Practices**:
- From OAuth: Clear trust boundaries, explicit scope definitions
- From GDPR: Minimize data transfer across boundaries
- From API gateways: Route-level data classification

**Operational Guidance**:
- For strict compliance (HIPAA, PCI-DSS): Use `overflow_mode = "block-entirely"`
- For hybrid deployments: Use `overflow_mode = "fresh-only"` with documented metadata risk
- Log all cross-zone overflow events for audit trails

---

## 5. Backend Affinity (Sticky Routing) Strategy

### Decision: Best-Effort Affinity Based on Conversation Hash

**Chosen Approach**: Hash conversation context → backend index, no persistent state.

**Implementation**:
```rust
// In SelectionReconciler
fn compute_affinity_key(request: &RequestRequirements) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    // Hash first user message content for stable routing
    if let Some(first_msg) = request.messages.first() {
        first_msg.content.hash(&mut hasher);
    }
    hasher.finish()
}

fn select_with_affinity(backends: &[Backend], key: u64) -> &Backend {
    let index = (key % backends.len() as u64) as usize;
    &backends[index]
}
```

**Rationale**:
- No session state required (stateless principle)
- Deterministic: same conversation → same backend (within same backend pool)
- Breaks on backend failure: acceptable per design (return 503, client retries)
- Simple consistent hashing: O(1) computation, <1μs overhead

**Alternatives Considered**:
1. **Session cookies/tokens**: Requires state, violates Principle VIII
2. **Persistent conversation ID mapping**: Requires database, adds latency
3. **No affinity (random)**: Poor UX for multi-turn conversations on restricted backends

**Best Practices**:
- From Redis: Consistent hashing for distributed caching
- From HAProxy: Server affinity based on request attributes
- From Kubernetes: StatefulSet pod affinity patterns

**Failure Modes**:
- Backend pool changes → affinity breaks → new backend selected
- Backend health degrades → excluded from pool → affinity shifts
- Expected behavior: Client receives 503, retries when backend recovers

---

## 6. Actionable 503 Error Responses

### Decision: Extend OpenAI Error Format with Context Object

**Chosen Approach**: Include structured context in `error.context` field (preserves API compatibility).

**Implementation**:
```rust
// In src/api/error.rs
#[derive(Serialize)]
pub struct RoutingErrorContext {
    pub rejection_reason: String,  // "privacy_zone_mismatch" | "tier_insufficient"
    pub required_zone: Option<String>,
    pub required_tier: Option<u8>,
    pub available_backends: Vec<String>,
    pub retry_after_seconds: u32,
}

// Example 503 response:
{
  "error": {
    "message": "No backends available matching privacy and capability requirements",
    "type": "insufficient_capacity",
    "code": 503,
    "context": {
      "rejection_reason": "tier_insufficient_reasoning",
      "required_tier": 8,
      "available_backends": ["local-ollama"],
      "retry_after_seconds": 30
    }
  }
}
```

**Rationale**:
- OpenAI API allows arbitrary fields in error object (tested with ChatGPT, Claude)
- Structured context enables programmatic error handling by clients
- Retry-After header follows HTTP spec (RFC 7231)
- Backwards compatible: clients ignore unknown fields

**Alternatives Considered**:
1. **Custom X-Nexus-Error headers**: Not visible in error response body
2. **Error message string parsing**: Fragile, not programmatic
3. **Separate /debug endpoint**: Extra request, adds latency

**Best Practices**:
- From Stripe API: Rich error objects with type, code, context
- From AWS APIs: Actionable error messages with resolution hints
- From Kubernetes: Structured status reasons in response objects

**Client Integration**:
```typescript
// Example client handling
try {
  const response = await openai.chat.completions.create({...});
} catch (error) {
  if (error.status === 503 && error.context?.rejection_reason === "tier_insufficient") {
    console.log(`Required tier ${error.context.required_tier} not available`);
    await sleep(error.context.retry_after_seconds * 1000);
    // Retry or fallback logic
  }
}
```

---

## 7. Performance Optimization Strategies

### Decision: Early Filtering + Memoization

**Target Latencies**:
- PrivacyReconciler: <50μs (simple zone check per backend)
- CapabilityReconciler: <100μs (parse tier metadata, compare scores)
- Total pipeline overhead: <500μs (meets constitution <1ms routing target)

**Optimization Techniques**:

1. **Parse Capability Tier Once at Backend Registration**:
   ```rust
   // Cache parsed CapabilityTier in Backend struct
   pub struct Backend {
       pub metadata: HashMap<String, String>,
       pub parsed_tier: Option<CapabilityTier>,  // Pre-parsed
   }
   ```

2. **Short-Circuit Reconcilers on Empty Candidate Pool**:
   ```rust
   async fn reconcile(&self, intent: &mut RoutingIntent) -> Result<()> {
       if intent.candidate_backends.is_empty() {
           return Ok(()); // No-op if already filtered out
       }
       // ... reconcile logic
   }
   ```

3. **Avoid Allocations in Hot Path**:
   - Reuse `excluded` HashMap with pre-allocated capacity
   - Use `retain()` for in-place filtering (no clone)
   - Zero-copy metadata access via references

**Benchmarking Strategy**:
```rust
// benches/privacy_reconciler.rs
#[bench]
fn bench_privacy_reconciler_10_backends(b: &mut Bencher) {
    let backends = create_test_backends(10);
    let reconciler = PrivacyReconciler::new(PrivacyConstraint::Restricted);
    b.iter(|| {
        let mut intent = create_intent(backends.clone());
        reconciler.reconcile(&mut intent).await
    });
}
```

**Best Practices**:
- From envoy-proxy: Profile hot paths, optimize allocations
- From HAProxy: Pre-compute routing tables, cache decisions
- From Rust performance book: Measure first, optimize second

---

## 8. Observability & Metrics

### Decision: Prometheus-Style Counters with Label Dimensions

**Key Metrics**:
```rust
// Privacy zone rejections
privacy_zone_rejections_total{zone="restricted", backend="local-ollama"}

// Capability tier rejections
tier_rejections_total{backend="local-ollama", dimension="reasoning", required="8", actual="6"}

// Cross-zone overflow events
cross_zone_overflow_total{from_zone="restricted", to_zone="open", has_history="false"}

// Affinity breaks (backend unavailable)
affinity_break_total{backend="local-ollama", reason="backend_unhealthy"}
```

**Rationale**:
- Dimensional metrics enable rich querying (e.g., "which backend rejects most often?")
- Counters are cheap: ~10ns per increment
- Prometheus format supported by Grafana, CloudWatch, Datadog

**Logging Strategy**:
```rust
// In PrivacyReconciler
tracing::info!(
    backend = %backend.name,
    zone = ?backend.zone,
    required = ?constraint,
    "Backend excluded due to privacy zone mismatch"
);
```

**Best Practices**:
- From OpenTelemetry: Structured logs with trace context
- From Prometheus: Metric naming conventions (suffix with `_total`, `_seconds`)
- From SRE Workbook: Alert on rate of change, not absolute values

---

## 9. Testing Strategy

### Test Pyramid:

**Unit Tests (Fast, Isolated)**:
- PrivacyConstraint::allows_backend() for all zone combinations
- CapabilityTier score comparisons
- TrafficPolicy pattern matching
- Header extraction edge cases

**Integration Tests (Mock Backends)**:
- Full reconciler pipeline with test backends
- 503 error response formatting
- Affinity routing consistency

**Contract Tests (Real Backends)**:
- Compatibility with Ollama, vLLM, OpenAI formats
- Configuration hot-reload
- Error response parsing by OpenAI clients

**Property-Based Tests (proptest)**:
- Affinity key distribution (no hash collisions)
- Policy precedence ordering
- Overflow mode combinations

### Test Data:
```rust
// tests/fixtures/backends.rs
pub fn restricted_high_tier_backend() -> Backend { /* ... */ }
pub fn open_low_tier_backend() -> Backend { /* ... */ }
pub fn cloud_backend() -> Backend { /* ... */ }

// tests/fixtures/policies.rs
pub fn code_policy() -> TrafficPolicy { /* ... */ }
pub fn chat_policy() -> TrafficPolicy { /* ... */ }
```

---

## 10. Configuration Migration Path

### Backwards Compatibility:

**Old Configuration (still works)**:
```toml
[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
# No zone/tier specified → defaults apply
```

**New Configuration (opt-in)**:
```toml
[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
zone = "Restricted"  # Explicit privacy zone
tier = 3             # Deprecated: use capability_tier

[backends.local-ollama.capability_tier]
reasoning = 7
coding = 8
context_window = 8192
vision = false
tools = true
```

**Deprecation Plan**:
1. Support both `tier` (integer) and `capability_tier` (struct) for 2 releases
2. Log warning if `tier` is used: "Deprecated: use capability_tier for multi-dimensional scoring"
3. Remove `tier` in v1.0.0

---

## Summary of Resolved Clarifications

| Technical Context Item | Resolution |
|------------------------|------------|
| Capability scoring dimensions | Multi-dimensional struct: reasoning, coding, context_window, vision, tools |
| TrafficPolicy configuration | TOML sections with glob pattern matching |
| Request header extraction | Early extraction in completions handler, passed to RoutingIntent |
| Cross-zone overflow | Block history, allow fresh conversations (configurable) |
| Backend affinity | Consistent hashing on conversation content (best-effort) |
| Error response format | Extend OpenAI error object with structured context |
| Performance targets | <50μs privacy, <100μs tier, <500μs total |
| Observability | Prometheus counters with dimensional labels |
| Testing approach | Unit + Integration + Contract + Property-based |
| Configuration migration | Backwards compatible, deprecate single `tier` field |

---

## Implementation Risks & Mitigations

**Risk**: Backend affinity may cause load imbalance  
**Mitigation**: Monitor backend load distribution, use consistent hashing with virtual nodes if needed

**Risk**: Complex TrafficPolicy matching may add latency  
**Mitigation**: Pre-compile glob patterns at config load, cache match results per request

**Risk**: Clients may not understand new 503 error format  
**Mitigation**: Include human-readable message in `error.message`, keep structured context optional

**Risk**: Privacy zone defaults may be surprising to users  
**Mitigation**: Log loudly at startup which backends are restricted vs. open, document in quickstart

---

**Phase 0 Complete**: All technical unknowns resolved. Ready for Phase 1 design artifacts.
