# Contract: Reconciler Trait

**Module**: `src/routing/reconciler/mod.rs`  
**Status**: API Design  
**Version**: 1.0.0

## Interface Definition

```rust
use crate::routing::error::RoutingError;
use crate::routing::reconciler::intent::RoutingIntent;

/// Reconciler trait for pipeline stages.
/// Each reconciler annotates RoutingIntent without removing prior constraints.
pub trait Reconciler: Send + Sync {
    /// Returns reconciler identifier for logging and rejection reasons.
    fn name(&self) -> &'static str;
    
    /// Reconcile routing intent based on reconciler's domain.
    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError>;
}
```

## Behavioral Contract

### Reconciler::name()

**Postconditions**:
- Returns static string (no allocation)
- String is unique across all reconcilers
- String is valid UTF-8
- Used for logging and RejectionReason.reconciler field

**Examples**:
```rust
assert_eq!(privacy_reconciler.name(), "PrivacyReconciler");
assert_eq!(budget_reconciler.name(), "BudgetReconciler");
```

---

### Reconciler::reconcile()

**Preconditions**:
- `intent` is non-null and valid
- `intent.candidate_agents` contains at least one agent (or empty if prior reconcilers excluded all)
- `intent.requirements` is populated by RequestAnalyzer

**Postconditions**:
- Returns `Ok(())` if reconciliation succeeded (even if all agents excluded)
- Returns `Err(RoutingError)` only for catastrophic failures (e.g., missing config, internal error)
- `intent.rejection_reasons` MAY have new entries added (NEVER removed)
- `intent.candidate_agents` MAY have agents moved to `intent.excluded_agents` (NEVER restored)
- `intent` constraints (privacy_constraint, min_capability_tier, budget_status) MAY be updated (NEVER cleared)
- Reconciler MUST NOT remove constraints or rejection reasons from prior reconcilers

**Invariants**:
1. **Order Independence**: Reconcilers only add constraints, never remove
   - If reconciler A excludes agent X, reconciler B cannot restore agent X
   - If reconciler A sets privacy_constraint, reconciler B cannot unset it
   
2. **Idempotency**: Calling reconcile() twice on same intent has no additional effect
   - First call: excludes agents based on constraints
   - Second call: no agents to exclude (all already filtered)
   
3. **Consistency**: candidate_agents and excluded_agents are disjoint sets
   - `intent.candidate_agents ∩ intent.excluded_agents = ∅`
   - Every agent is in exactly one set (or neither if never considered)

**Error Conditions**:
```rust
// Fatal errors (return Err):
RoutingError::ConfigMissing("budget_config") // Required config not loaded
RoutingError::InternalError("metric_snapshot_unavailable") // System state inconsistent

// Non-fatal conditions (return Ok):
// - All agents excluded (intent.candidate_agents is empty) → Ok(())
// - No matching TrafficPolicy → Ok(()) with no constraints added
// - Agent metadata missing → Ok(()) with agent excluded via RejectionReason
```

**Performance Requirements** (FR-036):
- Total pipeline overhead: <1ms for 95% of requests
- Individual reconciler: <200µs typical, <500µs p99
- RequestAnalyzer: <500µs (FR-009)

**Thread Safety**:
- Reconcilers are `Send + Sync` (can be shared across threads)
- `reconcile()` takes `&self` (immutable borrow)
- All state mutations go through `&mut RoutingIntent` parameter
- No internal mutable state in reconciler structs (stateless design)

---

## Usage Example

```rust
// Pipeline execution
let reconcilers: Vec<Box<dyn Reconciler>> = vec![
    Box::new(RequestAnalyzer::new(aliases, fallbacks)),
    Box::new(PrivacyReconciler::new(policies)),
    Box::new(BudgetReconciler::new(config, spending)),
    Box::new(TierReconciler::new(policies)),
    Box::new(QualityReconciler::new()), // stub
    Box::new(SchedulerReconciler::new(weights)),
];

let mut intent = RoutingIntent::new(
    request_id,
    requested_model,
    resolved_model,
    requirements,
    all_agent_ids,
);

// Execute pipeline
for reconciler in &reconcilers {
    tracing::debug!(
        reconciler = reconciler.name(),
        candidates_before = intent.candidate_agents.len(),
        "executing reconciler"
    );
    
    reconciler.reconcile(&mut intent)?;
    
    tracing::debug!(
        reconciler = reconciler.name(),
        candidates_after = intent.candidate_agents.len(),
        excluded = intent.excluded_agents.len(),
        "reconciler complete"
    );
}

// Convert to decision
let decision = if !intent.candidate_agents.is_empty() {
    // SchedulerReconciler selected agent
    RoutingDecision::Route { ... }
} else {
    // All agents excluded
    RoutingDecision::Reject {
        rejection_reasons: intent.rejection_reasons,
    }
};
```

---

## Testing Contract

### Unit Tests (per reconciler)

```rust
#[test]
fn reconciler_excludes_agents_matching_constraint() {
    let reconciler = PrivacyReconciler::new(...);
    let mut intent = RoutingIntent::new(...);
    intent.privacy_constraint = Some(PrivacyZone::Restricted);
    
    reconciler.reconcile(&mut intent).unwrap();
    
    // Verify cloud agents excluded
    assert!(!intent.candidate_agents.contains(&cloud_agent_id));
    assert!(intent.excluded_agents.contains(&cloud_agent_id));
    
    // Verify rejection reason added
    let reason = intent.rejection_reasons.iter()
        .find(|r| r.agent_id == cloud_agent_id)
        .expect("rejection reason");
    assert_eq!(reason.reconciler, "PrivacyReconciler");
}

#[test]
fn reconciler_never_removes_constraints() {
    let reconciler1 = PrivacyReconciler::new(...);
    let reconciler2 = BudgetReconciler::new(...);
    let mut intent = RoutingIntent::new(...);
    
    reconciler1.reconcile(&mut intent).unwrap();
    let constraint_after_r1 = intent.privacy_constraint;
    
    reconciler2.reconcile(&mut intent).unwrap();
    let constraint_after_r2 = intent.privacy_constraint;
    
    // Constraint from R1 must not be removed by R2
    assert_eq!(constraint_after_r1, constraint_after_r2);
}

#[test]
fn reconciler_is_idempotent() {
    let reconciler = TierReconciler::new(...);
    let mut intent = RoutingIntent::new(...);
    
    reconciler.reconcile(&mut intent).unwrap();
    let candidates_after_first = intent.candidate_agents.len();
    
    reconciler.reconcile(&mut intent).unwrap();
    let candidates_after_second = intent.candidate_agents.len();
    
    assert_eq!(candidates_after_first, candidates_after_second);
}
```

### Integration Tests (full pipeline)

```rust
#[test]
fn pipeline_executes_all_reconcilers_in_order() {
    let pipeline = ReconcilerPipeline::new(...);
    let mut intent = RoutingIntent::new(...);
    
    pipeline.execute(&mut intent).unwrap();
    
    // Verify all reconcilers executed (check logs or intent state)
    // Verify final decision is Route/Queue/Reject
}

#[test]
fn pipeline_respects_order_independence() {
    // Execute pipeline with reconcilers in order A→B→C
    let decision1 = execute_pipeline(vec![reconciler_a, reconciler_b, reconciler_c], ...);
    
    // Execute pipeline with reconcilers in order B→A→C
    let decision2 = execute_pipeline(vec![reconciler_b, reconciler_a, reconciler_c], ...);
    
    // Results should be equivalent (same agent selected or same rejection reasons)
    assert_decisions_equivalent(decision1, decision2);
}
```

---

## Migration Path

**From existing Router::select_backend()**:

1. **Phase 1**: Implement pipeline alongside existing router
   - New code in `src/routing/reconciler/`
   - Existing `Router::select_backend()` unchanged
   - Feature flag to enable pipeline: `[routing.use_pipeline = false]`

2. **Phase 2**: Integrate pipeline behind existing interface
   - `Router::select_backend()` calls pipeline internally
   - Existing tests pass without modification (FR-006, SC-002)
   - No API changes to callers

3. **Phase 3**: Remove old routing logic (future)
   - Delete imperative scoring code from Router
   - Pipeline is only routing path

---

## Version History

- **1.0.0** (2025-01-09): Initial contract definition for RFC-001 Phase 2
