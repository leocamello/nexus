# Contract: RoutingDecision Enum

**Module**: `src/routing/reconciler/decision.rs`  
**Status**: Public API (internal to routing)  
**Version**: 1.0.0

## Enum Definition

```rust
pub enum RoutingDecision {
    Route {
        agent_id: String,
        model: String,
        reason: String,
        cost_estimate: CostEstimate,
    },
    Queue {
        reason: String,
        estimated_wait_ms: u64,
        fallback_agent: Option<String>,
    },
    Reject {
        rejection_reasons: Vec<RejectionReason>,
    },
}
```

## Variant Semantics

### Route

**Meaning**: Routing succeeded, agent selected

**When returned**:
- `intent.candidate_agents` is non-empty after pipeline
- SchedulerReconciler selected highest-scoring agent

**Fields**:
- `agent_id`: Selected agent (from candidate_agents)
- `model`: Resolved model name (after alias expansion)
- `reason`: Human-readable routing explanation
  - Examples: `"highest_score:agent-123:0.95"`, `"only_healthy_backend"`
- `cost_estimate`: Populated by BudgetReconciler

**Conversion to RoutingResult**:
```rust
impl From<RoutingDecision> for Result<RoutingResult, RoutingError> {
    fn from(decision: RoutingDecision) -> Self {
        match decision {
            RoutingDecision::Route { agent_id, model, reason, cost_estimate } => {
                Ok(RoutingResult {
                    backend: registry.get_backend(&agent_id)?,
                    actual_model: model,
                    fallback_used: false,
                    route_reason: reason,
                    cost_estimated: Some(cost_estimate.cost_usd),
                })
            }
            // ... other variants
        }
    }
}
```

---

### Queue

**Meaning**: Agents exist but busy, request should queue

**When returned**:
- Highest-scoring agent has `HealthStatus::Loading` (FR-031)
- No other healthy agents available

**Fields**:
- `reason`: Why queueing required
  - Examples: `"agent_loading:agent-123:llama3:70b:45%"`
- `estimated_wait_ms`: From `HealthStatus::Loading.eta_ms`
- `fallback_agent`: Alternative agent if user doesn't want to wait

**HTTP Response**:
```json
{
  "error": {
    "message": "All agents busy",
    "type": "queue_required",
    "code": 503,
    "context": {
      "reason": "agent_loading:agent-123:llama3:70b:45%",
      "estimated_wait_ms": 30000,
      "fallback_agent": "agent-456"
    }
  }
}
```

---

### Reject

**Meaning**: No viable agents, request cannot be routed

**When returned**:
- `intent.candidate_agents` is empty after pipeline
- All agents excluded by one or more reconcilers

**Fields**:
- `rejection_reasons`: Aggregated reasons from all reconcilers
  - Grouped by reconciler type
  - Contains suggested_action per reason

**HTTP Response** (FR-007):
```json
{
  "error": {
    "message": "No agents available for request",
    "type": "no_viable_agents",
    "code": 503,
    "context": {
      "rejection_reasons": [
        {
          "agent_id": "agent-123",
          "reconciler": "PrivacyReconciler",
          "reason": "Agent privacy_zone=Open, required=Restricted",
          "suggested_action": "Use agents with privacy_zone=Restricted or relax constraint"
        },
        {
          "agent_id": "agent-456",
          "reconciler": "BudgetReconciler",
          "reason": "Monthly budget hard limit reached (100%)",
          "suggested_action": "Increase budget or retry with privacy=restricted agents"
        }
      ]
    }
  }
}
```

## Decision Logic

```rust
fn make_decision(intent: &RoutingIntent) -> RoutingDecision {
    if intent.candidate_agents.is_empty() {
        // All agents excluded
        return RoutingDecision::Reject {
            rejection_reasons: intent.rejection_reasons.clone(),
        };
    }
    
    // Score candidates (SchedulerReconciler logic)
    let best_agent = select_best(&intent.candidate_agents);
    
    if agent_is_loading(best_agent) {
        return RoutingDecision::Queue {
            reason: format!("agent_loading:{}", best_agent),
            estimated_wait_ms: get_eta(best_agent),
            fallback_agent: find_fallback(&intent.candidate_agents, best_agent),
        };
    }
    
    RoutingDecision::Route {
        agent_id: best_agent.clone(),
        model: intent.resolved_model.clone(),
        reason: format!("highest_score:{}", best_agent),
        cost_estimate: intent.cost_estimate.clone(),
    }
}
```

## Testing Contract

```rust
#[test]
fn route_decision_contains_selected_agent() {
    let decision = RoutingDecision::Route {
        agent_id: "agent-123".to_string(),
        model: "gpt-4".to_string(),
        reason: "test".to_string(),
        cost_estimate: CostEstimate::default(),
    };
    
    match decision {
        RoutingDecision::Route { agent_id, .. } => {
            assert_eq!(agent_id, "agent-123");
        }
        _ => panic!("expected Route"),
    }
}

#[test]
fn reject_decision_contains_all_rejection_reasons() {
    let reasons = vec![
        RejectionReason { ... },
        RejectionReason { ... },
    ];
    
    let decision = RoutingDecision::Reject {
        rejection_reasons: reasons.clone(),
    };
    
    match decision {
        RoutingDecision::Reject { rejection_reasons } => {
            assert_eq!(rejection_reasons.len(), 2);
        }
        _ => panic!("expected Reject"),
    }
}
```

---

**Version History**:
- **1.0.0** (2025-01-09): Initial contract for RFC-001 Phase 2
