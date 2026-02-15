# Research: Control Plane Reconciler Pipeline

**Feature**: Control Plane — Reconciler Pipeline (RFC-001 Phase 2)  
**Date**: 2024-02-15  
**Research Phase**: Phase 0

## Executive Summary

This document captures research findings for replacing the imperative 1615-line `Router::select_backend()` function with a pipeline of independent Reconcilers that annotate shared routing state. The goal is to enable Privacy Zones (F13) and Budget Management (F14) features without O(n²) feature interaction complexity.

## Research Questions Addressed

1. **Architectural Pattern**: What's the best pattern for implementing independent policy evaluators in Rust?
2. **State Management**: How should the shared RoutingIntent structure be designed?
3. **Error Handling**: How should policy failures be handled (fail-open vs fail-closed)?
4. **Performance**: Can we achieve <1ms routing decisions with a pipeline?
5. **Integration**: How to maintain Router::select_backend() API compatibility?

---

## Decision 1: Chain of Responsibility Pattern

**Decision**: Use **Chain of Responsibility with Sequential Pipeline** pattern

**Rationale**:
- Deterministic ordering: Privacy → Budget → Capability → Selection
- Single mutable reference eliminates lock contention
- Achieves <1ms latency requirement
- Natural fit for Rust ownership model
- Clear mental model for debugging and observability

**Alternatives Considered**:
1. **Tower Middleware Pattern**: Rejected due to moderate latency overhead and complexity of request extensions
2. **Visitor Pattern**: Rejected due to tree-based traversal overhead
3. **Actor Pattern (message passing)**: Rejected due to poor latency characteristics (>>1ms)

**Implementation Pattern**:
```rust
pub struct ReconcilerPipeline {
    reconcilers: Vec<Arc<dyn Reconciler>>,
}

#[async_trait]
pub trait Reconciler: Send + Sync {
    async fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), ReconcileError>;
    fn error_policy(&self) -> ReconcileErrorPolicy { ReconcileErrorPolicy::FailOpen }
}
```

---

## Decision 2: RoutingIntent Structure

**Decision**: Use annotation-based shared state with optional fields

**Rationale**:
- Each Reconciler adds annotations without requiring knowledge of others
- Optional fields allow incremental building of routing decision
- Zero-copy via Arc<Backend> reduces allocation overhead
- Trace info enables observability without impacting performance

**Data Model**:
```rust
#[derive(Debug, Clone)]
pub struct RoutingIntent {
    // Immutable Input
    pub request_requirements: RequestRequirements,
    pub candidate_backends: Vec<Arc<Backend>>,
    
    // Mutable Annotations (sequential write)
    pub annotations: RoutingAnnotations,
    
    // Decision Output
    pub decision: Option<RoutingDecision>,
}

#[derive(Debug, Clone, Default)]
pub struct RoutingAnnotations {
    pub privacy_constraints: Option<PrivacyConstraint>,
    pub excluded_backends: HashMap<String, ExclusionReason>,
    pub estimated_cost: Option<f64>,
    pub budget_status: Option<BudgetStatus>,
    pub required_tier: Option<u8>,
    pub trace_info: Vec<String>,
}
```

**Alternatives Considered**:
1. **DashMap for concurrent access**: Rejected due to lock contention overhead
2. **Builder pattern with methods**: Rejected due to API surface complexity
3. **Immutable state with clones**: Rejected due to allocation overhead

---

## Decision 3: Error Handling Strategy

**Decision**: Use configurable fail-open vs fail-closed per Reconciler

**Rationale**:
- Privacy violations must never be allowed (fail-closed)
- Budget service downtime should not block routing (fail-open)
- Capability mismatches can degrade gracefully (fail-open)
- Each policy has different criticality requirements

**Implementation**:
```rust
pub enum ReconcileErrorPolicy {
    FailOpen,    // Skip reconciler, log warning, continue
    FailClosed,  // Stop pipeline, return error immediately
}

// Per-reconciler policy configuration:
// - PrivacyReconciler: FailClosed (non-negotiable)
// - BudgetReconciler: FailOpen (operational)
// - CapabilityReconciler: FailOpen (graceful degradation)
// - SelectionReconciler: FailClosed (must select something)
```

**Alternatives Considered**:
1. **Always fail-closed**: Rejected due to operational fragility
2. **Always fail-open**: Rejected due to privacy compliance risk
3. **Circuit breaker pattern**: Deferred to future enhancement

---

## Decision 4: Performance Optimization

**Decision**: Use synchronous blocking where appropriate, async only for I/O

**Rationale**:
- Most reconcilers are CPU-bound (filtering, scoring)
- Async overhead (state machines, context switches) adds latency
- Blocking reconcilers achieve <100μs execution time
- Reserve async for future external budget service integration

**Pattern**:
```rust
// CPU-bound: Blocking implementation
impl Reconciler for PrivacyReconciler {
    async fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), ReconcileError> {
        // Direct filtering, no await points
        intent.candidate_backends.retain(|backend| {
            self.check_privacy_zone(backend, &intent.request_requirements)
        });
        Ok(())
    }
}

// Future I/O-bound: spawn_blocking for external service
impl Reconciler for BudgetReconciler {
    async fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), ReconcileError> {
        let cost = tokio::task::spawn_blocking(move || {
            // External budget service call
        }).await??;
        intent.annotations.estimated_cost = Some(cost);
        Ok(())
    }
}
```

**Performance Budget**:
- Privacy filtering: <50μs
- Budget annotation: <100μs (CPU-bound estimate)
- Capability matching: <50μs
- Backend selection: <200μs (scoring)
- **Total: <500μs** (well under 1ms requirement)

---

## Decision 5: API Compatibility

**Decision**: Wrap async pipeline in synchronous Router::select_backend()

**Rationale**:
- Existing callers expect synchronous API
- `tokio::task::block_in_place` provides zero-cost blocking
- Future async callers can use new async method directly
- Zero breaking changes to external API

**Implementation**:
```rust
impl Router {
    // Public API (unchanged signature)
    pub fn select_backend(
        &self,
        requirements: &RequestRequirements,
    ) -> Result<RoutingResult, RoutingError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(self.select_backend_async(requirements))
        })
    }

    // Internal async implementation
    async fn select_backend_async(
        &self,
        requirements: &RequestRequirements,
    ) -> Result<RoutingResult, RoutingError> {
        let mut intent = self.create_intent(requirements)?;
        self.pipeline.execute(&mut intent).await?;
        self.convert_to_result(intent)
    }
}
```

---

## Research Questions Resolved

### Q1: Integration with Existing Router Code

**Resolution**: Router maintains high-level orchestration, pipeline handles policy evaluation

- Router responsibilities: alias resolution, candidate filtering, result conversion
- Pipeline responsibilities: policy evaluation, backend selection logic
- Clean separation enables gradual migration

### Q2: Handling Fallback Chains

**Resolution**: FallbackReconciler as final reconciler in pipeline

- Operates on filtered candidate set from prior reconcilers
- Attempts fallback models only after primary model filtered
- Maintains existing fallback chain behavior

### Q3: Privacy Zone Integration with NII Agents

**Resolution**: Use existing AgentProfile.privacy_zone from agents

- No changes required to agent code
- PrivacyReconciler queries backend.agent.profile().privacy_zone
- Local backends (Ollama, vLLM) = PrivacyZone::Restricted
- Cloud backends (OpenAI, Anthropic) = PrivacyZone::Open

### Q4: Budget Estimation Without External Service

**Resolution**: Phase 1 uses in-memory token count * cost model

- Leverage existing RequestRequirements.estimated_tokens
- Simple cost model: cost_per_token * estimated_tokens
- Configurable per-backend cost rates
- Future Phase 2: External budget service integration

---

## Technology Stack Integration

### Rust Async Ecosystem

**async-trait**: Required for trait object compatibility
- All Reconcilers use #[async_trait] despite being mostly synchronous
- Enables future I/O-bound reconcilers without API changes

**tokio**: Existing runtime, no additional dependencies
- Use block_in_place for API compatibility
- Use spawn_blocking for future external service calls

**dashmap**: Not needed for RoutingIntent (single-threaded)
- Continue using for Registry backend storage
- RoutingIntent uses standard Vec/HashMap

### Existing Types Integration

**RequestRequirements**: Input to RoutingIntent
- Already contains estimated_tokens, needs_vision, needs_tools
- Add optional privacy_zone and budget_limit fields

**Backend/Arc<Backend>**: Candidate representation
- Continue using Arc to avoid clones
- Access privacy_zone via backend.agent.profile()

**RoutingResult**: Output from pipeline
- No changes required
- Convert RoutingDecision → RoutingResult in Router

---

## Implementation Phasing

### Phase 0: Infrastructure (This Research)
✅ Research complete - architecture decided

### Phase 1: Core Pipeline (Next)
- Implement ReconcilerPipeline and Reconciler trait
- Create RoutingIntent and annotation types
- Implement basic reconcilers: Privacy, Selection
- Integrate into Router::select_backend()
- All existing tests must pass

### Phase 2: Policy Reconcilers
- Implement BudgetReconciler with in-memory estimation
- Implement CapabilityTierReconciler
- Add detailed error messages with exclusion reasons
- Add observability (trace_info logging)

### Phase 3: Future Enhancements (Out of Scope)
- External budget service integration
- Circuit breaker for resilience
- Dynamic reconciler configuration
- Custom user-defined reconcilers

---

## Risks and Mitigations

### Risk 1: Performance Regression
**Mitigation**: Comprehensive benchmarking before merging
- Add routing.rs benchmark for pipeline path
- Target: <500μs for pipeline execution
- Continuous performance monitoring

### Risk 2: Complex Debugging
**Mitigation**: Rich observability through trace_info
- Each reconciler logs its actions
- Detailed exclusion reasons in annotations
- Structured logging with tracing crate

### Risk 3: Policy Interaction Bugs
**Mitigation**: Independent reconciler testing
- Unit tests for each reconciler in isolation
- Integration tests for common policy combinations
- Property-based tests for exclusion logic

---

## References

- **Tower HTTP**: Middleware pattern study
- **Tokio Tower**: Service abstraction patterns
- **RFC-001**: Control plane architecture specification
- **Nexus Constitution**: Performance standards (<1ms routing)

## Appendix: Benchmark Target

```rust
// benches/routing.rs
#[bench]
fn bench_select_backend_with_pipeline(b: &mut Bencher) {
    let router = setup_router_with_pipeline();
    let requirements = RequestRequirements {
        model: "llama3:8b".to_string(),
        estimated_tokens: 1000,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
    };
    
    b.iter(|| {
        black_box(router.select_backend(&requirements))
    });
}
// Target: <500μs per iteration
```

---

**Research Complete**: 2024-02-15  
**Next Phase**: Phase 1 - Design & Contracts
