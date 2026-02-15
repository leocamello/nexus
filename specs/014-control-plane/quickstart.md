# Quickstart: Control Plane Reconciler Pipeline

**Feature**: Control Plane — Reconciler Pipeline (RFC-001 Phase 2)  
**Date**: 2024-02-15  
**Audience**: Developers implementing or extending the reconciler pipeline

## Overview

This guide shows how to use and extend the reconciler pipeline for routing decisions. The pipeline replaces the imperative `Router::select_backend()` with a composable policy system.

## 5-Minute Quick Start

### 1. Understanding the Pipeline

The reconciler pipeline processes routing decisions in stages:

```text
Request → RoutingIntent → [Reconcilers] → RoutingDecision → Result

Reconcilers:
  1. PrivacyReconciler    (filter by zone)
  2. BudgetReconciler     (annotate cost)
  3. CapabilityReconciler (filter by features)
  4. SelectionReconciler  (pick final backend)
```

### 2. Using the Router (No Changes!)

The public API is unchanged:

```rust
use nexus::routing::{Router, RequestRequirements};

// Create router (pipeline built automatically)
let router = Router::new(registry, strategy, weights);

// Use exactly as before
let requirements = RequestRequirements {
    model: "llama3:8b".to_string(),
    estimated_tokens: 1000,
    needs_vision: false,
    needs_tools: false,
    needs_json_mode: false,
};

let result = router.select_backend(&requirements)?;
println!("Selected: {} ({})", result.backend.name, result.route_reason);
```

### 3. Understanding RoutingIntent

The shared state that reconcilers annotate:

```rust
use nexus::control::{RoutingIntent, RoutingAnnotations};

// Intent contains:
// - request_requirements (immutable input)
// - candidate_backends (filtered by reconcilers)
// - annotations (added by reconcilers)
// - decision (final output)

// After pipeline execution:
let intent = /* ... */;

// Check what happened:
println!("Trace: {:?}", intent.annotations.trace_info);
println!("Privacy excluded: {:?}", intent.annotations.privacy_excluded);
println!("Estimated cost: ${:.2}", intent.annotations.estimated_cost.unwrap());
```

## Creating a Custom Reconciler

### Step 1: Define Your Policy

Example: Reject backends with high latency

```rust
use nexus::control::{Reconciler, RoutingIntent, ReconcileError, ReconcileErrorPolicy};
use async_trait::async_trait;

pub struct LatencyReconciler {
    max_latency_ms: u32,
}

impl LatencyReconciler {
    pub fn new(max_latency_ms: u32) -> Self {
        Self { max_latency_ms }
    }
}
```

### Step 2: Implement Reconciler Trait

```rust
#[async_trait]
impl Reconciler for LatencyReconciler {
    async fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), ReconcileError> {
        // Filter backends by latency
        let mut excluded = HashMap::new();
        
        intent.candidate_backends.retain(|backend| {
            let latency = backend.avg_latency_ms.load(Ordering::Relaxed);
            
            if latency > self.max_latency_ms {
                excluded.insert(
                    backend.name.clone(),
                    format!("Latency {}ms exceeds limit {}ms", latency, self.max_latency_ms)
                );
                false
            } else {
                true
            }
        });
        
        // Add trace for observability
        intent.trace(format!(
            "Latency: {} backends within {}ms limit",
            intent.candidate_backends.len(),
            self.max_latency_ms
        ));
        
        Ok(())
    }
    
    fn error_policy(&self) -> ReconcileErrorPolicy {
        ReconcileErrorPolicy::FailOpen // Degrade gracefully
    }
    
    fn name(&self) -> &str {
        "LatencyReconciler"
    }
}
```

### Step 3: Add to Pipeline

```rust
use nexus::control::ReconcilerPipeline;

let pipeline = ReconcilerPipeline::new(vec![
    Arc::new(PrivacyReconciler::new(PrivacyConstraint::Unrestricted)),
    Arc::new(BudgetReconciler::new(/* config */)),
    Arc::new(LatencyReconciler::new(200)), // Your custom reconciler!
    Arc::new(CapabilityReconciler::new()),
    Arc::new(SelectionReconciler::new(strategy, weights)),
]);

let router = Router::with_pipeline(registry, pipeline);
```

## Common Patterns

### Pattern 1: Filtering Backends

```rust
// Remove backends that don't meet criteria
intent.candidate_backends.retain(|backend| {
    self.check_policy(backend)
});
```

### Pattern 2: Annotating Metadata

```rust
// Add information for downstream reconcilers
intent.annotations.estimated_cost = Some(calculate_cost());
intent.annotations.custom_data = Some(my_data);
```

### Pattern 3: Recording Exclusions

```rust
// Track why backends were excluded (for error messages)
let mut excluded = HashMap::new();
intent.candidate_backends.retain(|backend| {
    if !self.check(backend) {
        excluded.insert(backend.name.clone(), "Reason: ...");
        false
    } else {
        true
    }
});
intent.annotations.my_excluded = excluded;
```

### Pattern 4: Observability

```rust
// Always add trace messages
intent.trace(format!(
    "{}: {} candidates after filtering",
    self.name(),
    intent.candidate_backends.len()
));
```

## Configuration Examples

### Example 1: Strict Privacy Mode

```rust
let privacy = PrivacyReconciler::new(PrivacyConstraint::Restricted);
// Only local backends allowed
```

### Example 2: Budget-Aware Routing

```rust
let mut cost_model = HashMap::new();
cost_model.insert("openai".to_string(), 0.03 / 1000.0); // $0.03 per 1K tokens
cost_model.insert("ollama".to_string(), 0.0);           // Free

let budget = BudgetReconciler::new(
    cost_model,
    Some(100.0), // $100 monthly limit
);
```

### Example 3: Capability Tiers

```rust
// Ensure requests go to vision-capable backends
let requirements = RequestRequirements {
    model: "gpt-4-vision".to_string(),
    needs_vision: true, // CapabilityReconciler will enforce this
    // ...
};
```

## Debugging

### Trace Output

Enable trace logging to see reconciler execution:

```rust
// After routing
let result = router.select_backend(&requirements)?;

// Check annotations
if let Some(annotations) = /* get annotations */ {
    for trace in &annotations.trace_info {
        println!("[TRACE] {}", trace);
    }
}

// Output:
// [TRACE] ✓ PrivacyReconciler: 3 candidates allowed
// [TRACE] ✓ BudgetReconciler: $0.15 estimated
// [TRACE] ✓ CapabilityReconciler: 2 vision-capable
// [TRACE] ✓ SelectionReconciler: selected backend-1
```

### Understanding Exclusions

When routing fails, check exclusion reasons:

```rust
match router.select_backend(&requirements) {
    Err(RoutingError::ModelNotFound { model }) => {
        // Check which policies excluded backends
        println!("Privacy excluded: {:?}", annotations.privacy_excluded);
        println!("Budget excluded: {:?}", annotations.budget_excluded);
        println!("Capability excluded: {:?}", annotations.capability_excluded);
    }
    Err(e) => eprintln!("Error: {}", e),
    Ok(result) => { /* success */ }
}
```

## Performance Tips

### 1. Keep Reconcilers Fast

Target <100μs per reconciler:

```rust
// ✅ Good: Direct filtering (fast)
intent.candidate_backends.retain(|b| check_local(b));

// ❌ Bad: External API calls (slow)
for backend in &intent.candidate_backends {
    let status = fetch_external_api(backend).await; // Adds latency!
}
```

### 2. Use Synchronous Logic When Possible

```rust
// Most reconcilers should be synchronous
#[async_trait]
impl Reconciler for MyReconciler {
    async fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), ReconcileError> {
        // No .await calls = fast!
        intent.candidate_backends.retain(|b| self.check(b));
        Ok(())
    }
}
```

### 3. Benchmark Your Reconciler

```rust
#[bench]
fn bench_my_reconciler(b: &mut Bencher) {
    let reconciler = MyReconciler::new();
    let mut intent = setup_test_intent();
    
    b.iter(|| {
        reconciler.reconcile(&mut intent)
    });
}
// Target: <100μs per iteration
```

## Testing

### Unit Test Template

```rust
#[tokio::test]
async fn my_reconciler_filters_correctly() {
    let reconciler = MyReconciler::new(/* config */);
    
    let mut intent = RoutingIntent::new(
        test_requirements(),
        vec![
            good_backend(),  // Should keep
            bad_backend(),   // Should filter
        ],
    );
    
    reconciler.reconcile(&mut intent).await.unwrap();
    
    assert_eq!(intent.candidate_backends.len(), 1);
    assert_eq!(intent.candidate_backends[0].name, "good_backend");
    assert!(intent.annotations.trace_info.contains(&"MyReconciler"));
}
```

### Integration Test Template

```rust
#[tokio::test]
async fn pipeline_with_my_reconciler() {
    let pipeline = ReconcilerPipeline::new(vec![
        Arc::new(MyReconciler::new(/* config */)),
        Arc::new(SelectionReconciler::new(/* config */)),
    ]);
    
    let mut intent = RoutingIntent::new(test_requirements(), test_backends());
    pipeline.execute(&mut intent).await.unwrap();
    
    assert!(intent.decision.is_some());
}
```

## Migration Guide

### From Imperative Router to Pipeline

**Before** (imperative):
```rust
// In Router::select_backend()
let candidates = self.filter_candidates(&model, requirements);

// Privacy check (hardcoded)
let filtered = candidates.into_iter()
    .filter(|b| b.privacy_zone == PrivacyZone::Restricted)
    .collect();

// Budget check (hardcoded)
let affordable = filtered.into_iter()
    .filter(|b| estimate_cost(b) < budget)
    .collect();

// Selection (hardcoded)
let backend = self.apply_strategy(&affordable);
```

**After** (pipeline):
```rust
// In Router::select_backend_async()
let mut intent = RoutingIntent::new(requirements, candidates);
self.pipeline.execute(&mut intent).await?;
let decision = intent.decision.ok_or(RoutingError::NoSelection)?;
```

Each policy is now an independent reconciler!

## FAQ

### Q: Can reconcilers access external services?

A: Yes, but use `tokio::task::spawn_blocking` for I/O to avoid blocking:

```rust
async fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), ReconcileError> {
    let budget_status = tokio::task::spawn_blocking(move || {
        self.budget_service.get_status() // Blocking I/O
    }).await??;
    
    intent.annotations.budget_status = Some(budget_status);
    Ok(())
}
```

### Q: What happens if a reconciler removes all candidates?

A: The SelectionReconciler will return `ReconcileError::NoCandidates`, which maps to `RoutingError::ModelNotFound` with detailed exclusion reasons.

### Q: Can reconcilers depend on each other?

A: No! Reconcilers must be independent. They can *read* annotations from prior reconcilers but should not *require* them.

### Q: How do I add a new policy without modifying existing code?

A: Just implement `Reconciler` and add to pipeline:

```rust
let pipeline = ReconcilerPipeline::new(vec![
    // Existing reconcilers
    Arc::new(PrivacyReconciler::new(/* ... */)),
    Arc::new(BudgetReconciler::new(/* ... */)),
    
    // Your new policy!
    Arc::new(MyNewReconciler::new(/* ... */)),
    
    // Selection must be last
    Arc::new(SelectionReconciler::new(/* ... */)),
]);
```

## Next Steps

1. **Read**: [data-model.md](./data-model.md) for complete type reference
2. **Read**: [contracts/reconciler-trait.md](./contracts/reconciler-trait.md) for implementation contract
3. **Explore**: `src/control/` for implementation examples
4. **Test**: `tests/control/` for test patterns

---

**Last Updated**: 2024-02-15  
**Status**: Ready for Implementation
