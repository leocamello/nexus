# Contract: RoutingIntent Struct

**Module**: `src/routing/reconciler/intent.rs`  
**Status**: Internal API  
**Version**: 1.0.0

## Struct Definition

```rust
pub struct RoutingIntent {
    // Identity
    pub request_id: String,
    
    // Model resolution
    pub requested_model: String,
    pub resolved_model: String,
    
    // Requirements (from RFC-001 Phase 1)
    pub requirements: RequestRequirements,
    
    // Constraints from policies
    pub privacy_constraint: Option<PrivacyZone>,
    pub min_capability_tier: Option<u8>,
    
    // Budget state
    pub budget_status: BudgetStatus,
    pub cost_estimate: CostEstimate,
    
    // Agent selection
    pub candidate_agents: Vec<String>,
    pub excluded_agents: Vec<String>,
    pub rejection_reasons: Vec<RejectionReason>,
}
```

## Invariants

1. **Disjoint Sets**: `candidate_agents ∩ excluded_agents = ∅`
2. **Rejection Consistency**: For each agent in `excluded_agents`, there exists at least one `RejectionReason` with matching `agent_id`
3. **Model Resolution**: `resolved_model` is never empty after RequestAnalyzer
4. **Budget Status**: Always initialized to `BudgetStatus::Normal`

## State Transitions

```
Initial State:
  candidate_agents = [all agents]
  excluded_agents = []
  rejection_reasons = []

After each reconciler:
  candidate_agents -= excluded_by_reconciler
  excluded_agents += excluded_by_reconciler
  rejection_reasons += new_reasons

Final State (SchedulerReconciler):
  If candidate_agents.is_empty():
    → RoutingDecision::Reject
  Else:
    → RoutingDecision::Route (with selected agent)
```

## Helper Methods

### exclude_agent()

```rust
pub fn exclude_agent(
    &mut self,
    agent_id: String,
    reconciler: &'static str,
    reason: String,
    suggested_action: String,
)
```

**Behavior**:
- Removes `agent_id` from `candidate_agents`
- Adds `agent_id` to `excluded_agents`
- Appends `RejectionReason` to `rejection_reasons`
- Idempotent: excluding same agent twice has no additional effect

**Example**:
```rust
intent.exclude_agent(
    "agent-123".to_string(),
    "PrivacyReconciler",
    "Agent privacy_zone=Open, required=Restricted".to_string(),
    "Use agents with privacy_zone=Restricted or relax constraint".to_string(),
);
```

## Testing Contract

```rust
#[test]
fn exclude_agent_maintains_disjoint_sets() {
    let mut intent = RoutingIntent::new(...);
    intent.exclude_agent("agent-1", "Test", "reason", "action");
    
    assert!(!intent.candidate_agents.contains(&"agent-1".to_string()));
    assert!(intent.excluded_agents.contains(&"agent-1".to_string()));
}

#[test]
fn exclude_agent_is_idempotent() {
    let mut intent = RoutingIntent::new(...);
    intent.exclude_agent("agent-1", "Test", "reason", "action");
    let reasons_after_first = intent.rejection_reasons.len();
    
    intent.exclude_agent("agent-1", "Test", "reason", "action");
    let reasons_after_second = intent.rejection_reasons.len();
    
    assert_eq!(reasons_after_first, reasons_after_second);
}
```

---

**Version History**:
- **1.0.0** (2025-01-09): Initial contract for RFC-001 Phase 2
