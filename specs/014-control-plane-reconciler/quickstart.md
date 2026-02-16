# Quickstart: Control Plane — Reconciler Pipeline

**Target Audience**: Developers implementing RFC-001 Phase 2  
**Prerequisites**: Familiarity with Nexus routing architecture  
**Estimated Time**: 30 minutes to understand, 8-12 hours to implement core pipeline

---

## Overview

This feature replaces the imperative `Router::select_backend()` god-function with a pipeline of 6 independent reconcilers:

```
Request → RequestAnalyzer → PrivacyReconciler → BudgetReconciler → 
          TierReconciler → QualityReconciler → SchedulerReconciler → Decision
```

**Key Principle**: Each reconciler only **adds constraints** to `RoutingIntent`, never removes them. This ensures order-independence and composability.

---

## Implementation Roadmap

### Phase 1: Core Infrastructure (4-5 hours)

**Goal**: Implement Reconciler trait and pipeline executor

```rust
// src/routing/reconciler/mod.rs
pub trait Reconciler: Send + Sync {
    fn name(&self) -> &'static str;
    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError>;
}

pub struct ReconcilerPipeline {
    reconcilers: Vec<Box<dyn Reconciler>>,
}

impl ReconcilerPipeline {
    pub fn new(reconcilers: Vec<Box<dyn Reconciler>>) -> Self {
        Self { reconcilers }
    }
    
    pub fn execute(&self, intent: &mut RoutingIntent) -> Result<RoutingDecision, RoutingError> {
        for reconciler in &self.reconcilers {
            reconciler.reconcile(intent)?;
        }
        
        // Convert intent to decision
        if intent.candidate_agents.is_empty() {
            Ok(RoutingDecision::Reject {
                rejection_reasons: intent.rejection_reasons.clone(),
            })
        } else {
            // SchedulerReconciler already selected agent
            Ok(RoutingDecision::Route { /* ... */ })
        }
    }
}
```

**Tests**:
```rust
#[test]
fn pipeline_executes_reconcilers_in_order() {
    let pipeline = ReconcilerPipeline::new(vec![
        Box::new(MockReconciler::new("R1")),
        Box::new(MockReconciler::new("R2")),
    ]);
    
    let mut intent = RoutingIntent::new(...);
    pipeline.execute(&mut intent).unwrap();
    
    // Verify execution order via logs or intent state
}
```

---

### Phase 2: RequestAnalyzer (1-2 hours)

**Goal**: Resolve aliases and populate requirements

```rust
// src/routing/reconciler/request_analyzer.rs
pub struct RequestAnalyzer {
    aliases: HashMap<String, String>,
    fallbacks: HashMap<String, Vec<String>>,
}

impl Reconciler for RequestAnalyzer {
    fn name(&self) -> &'static str {
        "RequestAnalyzer"
    }
    
    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        // 1. Resolve model aliases (max 3 levels)
        intent.resolved_model = self.resolve_alias(&intent.requested_model);
        
        // 2. Requirements already populated from RequestRequirements::from_request()
        // (RFC-001 Phase 1 complete)
        
        // 3. Filter candidates by model availability
        let registry = ...; // Injected via constructor
        intent.candidate_agents = registry
            .get_backends_for_model(&intent.resolved_model)
            .iter()
            .map(|b| b.id.clone())
            .collect();
        
        Ok(())
    }
}
```

**Performance target**: <500µs (FR-009)

---

### Phase 3: PrivacyReconciler (1-2 hours)

**Goal**: Enforce privacy zone constraints

```rust
// src/routing/reconciler/privacy.rs
pub struct PrivacyReconciler {
    policy_matcher: PolicyMatcher,  // Pre-compiled glob patterns
}

impl Reconciler for PrivacyReconciler {
    fn name(&self) -> &'static str {
        "PrivacyReconciler"
    }
    
    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        // 1. Find matching policy for model
        let policy = self.policy_matcher.find_policy(&intent.resolved_model);
        
        if let Some(policy) = policy {
            if policy.privacy == PrivacyConstraint::Restricted {
                // 2. Set constraint on intent
                intent.privacy_constraint = Some(PrivacyZone::Restricted);
                
                // 3. Exclude cloud agents
                let agents_to_exclude: Vec<_> = intent.candidate_agents
                    .iter()
                    .filter(|id| {
                        let profile = get_agent_profile(id);
                        profile.privacy_zone == PrivacyZone::Open
                    })
                    .cloned()
                    .collect();
                
                for agent_id in agents_to_exclude {
                    intent.exclude_agent(
                        agent_id,
                        self.name(),
                        "Agent privacy_zone=Open, required=Restricted".to_string(),
                        "Use agents with privacy_zone=Restricted or relax constraint".to_string(),
                    );
                }
            }
        }
        
        Ok(())
    }
}
```

**Key insight**: Use `globset` crate for pattern matching (~1.5µs overhead)

---

### Phase 4: BudgetReconciler + Background Loop (2-3 hours)

**Goal**: Track spending and enforce budget limits

```rust
// src/routing/reconciler/budget.rs
pub struct BudgetReconciler {
    config: BudgetConfig,
    spending: Arc<DashMap<String, BudgetMetrics>>, // Shared with reconciliation loop
}

impl Reconciler for BudgetReconciler {
    fn name(&self) -> &'static str {
        "BudgetReconciler"
    }
    
    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        // 1. Estimate cost
        let cost_estimate = self.estimate_cost(&intent.requirements, &intent.resolved_model);
        intent.cost_estimate = cost_estimate;
        
        // 2. Calculate budget status
        let snapshot = self.spending.get("monthly").map(|v| v.clone());
        let status = self.calculate_status(&snapshot, cost_estimate.cost_usd);
        intent.budget_status = status;
        
        // 3. Filter based on status
        match status {
            BudgetStatus::SoftLimit => {
                // Prefer local agents (increase priority in scoring)
                // SchedulerReconciler will handle this
            }
            BudgetStatus::HardLimit => {
                // Exclude cloud agents
                let cloud_agents: Vec<_> = intent.candidate_agents
                    .iter()
                    .filter(|id| is_cloud_agent(id))
                    .cloned()
                    .collect();
                
                for agent_id in cloud_agents {
                    intent.exclude_agent(
                        agent_id,
                        self.name(),
                        format!("Budget hard limit reached ({}%)", 100),
                        "Increase budget or use local agents".to_string(),
                    );
                }
            }
            _ => {}
        }
        
        Ok(())
    }
}

// Background reconciliation loop (follows health checker pattern)
pub struct BudgetReconciliationLoop {
    registry: Arc<Registry>,
    config: BudgetConfig,
    spending: Arc<DashMap<String, BudgetMetrics>>,
}

impl BudgetReconciliationLoop {
    pub fn start(self, cancel_token: CancellationToken) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            
            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => break,
                    _ = interval.tick() => self.reconcile_spending().await,
                }
            }
        })
    }
    
    async fn reconcile_spending(&self) {
        // Aggregate telemetry from all agents
        let total_spent: f64 = self.registry
            .get_all_backends()
            .iter()
            .filter_map(|b| get_agent_spending(b))
            .sum();
        
        self.spending.insert(
            "monthly".to_string(),
            BudgetMetrics {
                total_spent,
                request_count: get_request_count(),
                timestamp: Utc::now(),
            },
        );
    }
}
```

---

### Phase 5: TierReconciler (1 hour)

**Goal**: Enforce capability tier requirements

```rust
// src/routing/reconciler/tier.rs
pub struct TierReconciler {
    policy_matcher: PolicyMatcher,
}

impl Reconciler for TierReconciler {
    fn name(&self) -> &'static str {
        "TierReconciler"
    }
    
    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        let policy = self.policy_matcher.find_policy(&intent.resolved_model);
        
        if let Some(policy) = policy {
            if let Some(min_tier) = policy.min_tier {
                intent.min_capability_tier = Some(min_tier);
                
                // Exclude agents below tier
                let low_tier_agents: Vec<_> = intent.candidate_agents
                    .iter()
                    .filter(|id| {
                        let profile = get_agent_profile(id);
                        profile.capability_tier().unwrap_or(0) < min_tier
                    })
                    .cloned()
                    .collect();
                
                for agent_id in low_tier_agents {
                    intent.exclude_agent(
                        agent_id,
                        self.name(),
                        format!("Agent tier below required (min: {})", min_tier),
                        "Use higher-tier agents or relax constraint".to_string(),
                    );
                }
            }
        }
        
        Ok(())
    }
}
```

---

### Phase 6: SchedulerReconciler (2 hours)

**Goal**: Score candidates and select best agent

```rust
// src/routing/reconciler/scheduler.rs
pub struct SchedulerReconciler {
    weights: ScoringWeights,
}

impl Reconciler for SchedulerReconciler {
    fn name(&self) -> &'static str {
        "SchedulerReconciler"
    }
    
    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        if intent.candidate_agents.is_empty() {
            // All agents excluded by prior reconcilers
            return Ok(());
        }
        
        // Score each candidate
        let mut scores: Vec<_> = intent.candidate_agents
            .iter()
            .map(|id| {
                let backend = get_backend(id);
                let score = score_backend(
                    backend.priority,
                    backend.pending_requests.load(Ordering::Relaxed),
                    backend.avg_latency_ms.load(Ordering::Relaxed),
                    &self.weights,
                );
                
                // Adjust score for budget status
                let adjusted_score = if intent.budget_status == BudgetStatus::SoftLimit
                    && !is_cloud_agent(id)
                {
                    score * 1.5  // Prefer local agents
                } else {
                    score
                };
                
                (id.clone(), adjusted_score)
            })
            .collect();
        
        // Sort by score descending
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        // Best agent is first
        let best_agent = &scores[0].0;
        
        // Check if agent is loading
        if agent_is_loading(best_agent) {
            // Will become RoutingDecision::Queue in pipeline executor
            return Ok(());
        }
        
        // Selected agent remains in candidate_agents
        // Pipeline executor will convert to RoutingDecision::Route
        
        Ok(())
    }
}
```

---

### Phase 7: Integration into Router (1-2 hours)

**Goal**: Call pipeline from Router::select_backend()

```rust
// src/routing/mod.rs
impl Router {
    pub fn select_backend(
        &self,
        requirements: &RequestRequirements,
    ) -> Result<RoutingResult, RoutingError> {
        // Build pipeline
        let pipeline = ReconcilerPipeline::new(vec![
            Box::new(RequestAnalyzer::new(self.aliases.clone(), self.fallbacks.clone())),
            Box::new(PrivacyReconciler::new(self.policy_matcher.clone())),
            Box::new(BudgetReconciler::new(self.budget_config.clone(), self.spending.clone())),
            Box::new(TierReconciler::new(self.policy_matcher.clone())),
            Box::new(QualityReconciler::new()),  // Stub for now
            Box::new(SchedulerReconciler::new(self.weights)),
        ]);
        
        // Create intent
        let mut intent = RoutingIntent::new(
            uuid::Uuid::new_v4().to_string(),
            requirements.model.clone(),
            requirements.model.clone(),  // Will be resolved by RequestAnalyzer
            requirements.clone(),
            self.registry.get_all_backend_ids(),
        );
        
        // Execute pipeline
        let decision = pipeline.execute(&mut intent)?;
        
        // Convert decision to RoutingResult
        match decision {
            RoutingDecision::Route { agent_id, model, reason, cost_estimate } => {
                Ok(RoutingResult {
                    backend: self.registry.get_backend(&agent_id)?,
                    actual_model: model,
                    fallback_used: false,
                    route_reason: reason,
                    cost_estimated: Some(cost_estimate.cost_usd),
                })
            }
            RoutingDecision::Reject { rejection_reasons } => {
                Err(RoutingError::NoViableAgents { rejection_reasons })
            }
            RoutingDecision::Queue { reason, estimated_wait_ms, fallback_agent } => {
                Err(RoutingError::AllAgentsBusy { reason, estimated_wait_ms, fallback_agent })
            }
        }
    }
}
```

**Validation**: All existing Router tests pass without modification (FR-006, SC-002)

---

## Configuration Example

```toml
# nexus.toml
[routing]
strategy = "smart"

# Traffic policies (optional)
[[routing.policies]]
model_pattern = "gpt-4-*"
privacy = "restricted"
min_tier = 3

[[routing.policies]]
model_pattern = "claude-*"
privacy = "unrestricted"

# Budget configuration (optional)
[routing.budget]
monthly_limit_usd = 1000.00
soft_limit_percent = 0.75
hard_limit_action = "block_cloud"
```

---

## Testing Strategy

1. **Unit tests**: Each reconciler in isolation with mock RoutingIntent
2. **Integration tests**: Full pipeline with mock backends
3. **Regression tests**: Existing Router tests must pass
4. **Property tests**: Order-independence, idempotency

```rust
#[test]
fn privacy_reconciler_excludes_cloud_agents() {
    let reconciler = PrivacyReconciler::new(...);
    let mut intent = RoutingIntent::new(...);
    intent.privacy_constraint = Some(PrivacyZone::Restricted);
    
    reconciler.reconcile(&mut intent).unwrap();
    
    assert!(!intent.candidate_agents.contains(&cloud_agent_id));
    assert!(intent.excluded_agents.contains(&cloud_agent_id));
}
```

---

## Performance Checklist

- [ ] Pipeline completes in <1ms for 95% of requests (FR-036)
- [ ] RequestAnalyzer completes in <500µs (FR-009)
- [ ] PolicyMatcher uses pre-compiled globset (~1.5µs per match)
- [ ] BudgetReconciler uses snapshot (no async I/O in hot path)
- [ ] RoutingIntent is stack-allocated (no heap per request)

---

## Troubleshooting

**Issue**: Pipeline slower than 1ms

- Profile with `cargo flamegraph` on real workload
- Check PolicyMatcher is pre-compiled (not compiling on each request)
- Ensure AgentSchedulingProfile construction is cached

**Issue**: Existing tests failing

- Verify Router::select_backend() signature unchanged
- Check RoutingResult mapping from RoutingDecision
- Ensure fallback chains still work

**Issue**: Rejection reasons missing

- Verify each reconciler calls `intent.exclude_agent()` when excluding
- Check pipeline executor aggregates rejection_reasons correctly

---

## Next Steps

After implementation:
1. Run `/speckit.tasks` to generate task breakdown
2. Implement TDD: Write tests first, verify RED, then implement
3. Profile performance with benches/routing.rs
4. Update documentation in docs/ARCHITECTURE.md

**Estimated total implementation time**: 8-12 hours for core pipeline + reconcilers
