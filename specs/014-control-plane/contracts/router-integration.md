# Router Integration Contract

**Feature**: Control Plane Reconciler Pipeline  
**Contract Type**: API Integration  
**Date**: 2024-02-15

## Overview

This contract defines how the ReconcilerPipeline integrates with the existing Router without breaking the public API.

## Router API Contract (Unchanged)

```rust
impl Router {
    /// Select the best backend for the given requirements
    ///
    /// # Contract
    ///
    /// **Signature**: MUST remain unchanged (backward compatibility)
    /// **Behavior**: Internally uses pipeline, externally identical
    /// **Performance**: <1ms routing decision (constitutional requirement)
    ///
    pub fn select_backend(
        &self,
        requirements: &RequestRequirements,
    ) -> Result<RoutingResult, RoutingError> {
        // Implementation uses pipeline internally
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(self.select_backend_async(requirements))
        })
    }
    
    /// Internal async implementation (new)
    async fn select_backend_async(
        &self,
        requirements: &RequestRequirements,
    ) -> Result<RoutingResult, RoutingError> {
        // 1. Alias resolution
        let model = self.resolve_alias(&requirements.model);
        
        // 2. Get candidates
        let candidates = self.filter_candidates(&model, requirements);
        
        // 3. Create intent
        let mut intent = RoutingIntent::new(
            requirements.clone(),
            candidates.into_iter().map(Arc::new).collect(),
        );
        
        // 4. Execute pipeline
        self.pipeline.execute(&mut intent).await?;
        
        // 5. Convert to result
        let decision = intent.decision.ok_or(RoutingError::NoSelection)?;
        Ok(RoutingResult {
            backend: decision.backend,
            actual_model: model,
            fallback_used: intent.annotations.fallback_used,
            route_reason: decision.reason,
        })
    }
}
```

## Router Construction Contract

```rust
impl Router {
    /// Create router with reconciler pipeline
    pub fn new(
        registry: Arc<Registry>,
        strategy: RoutingStrategy,
        weights: ScoringWeights,
    ) -> Self {
        // Build default pipeline
        let pipeline = ReconcilerPipeline::new(vec![
            Arc::new(PrivacyReconciler::new(PrivacyConstraint::Unrestricted)),
            Arc::new(BudgetReconciler::new(/* config */)),
            Arc::new(CapabilityReconciler::new()),
            Arc::new(SelectionReconciler::new(strategy, weights)),
        ]);
        
        Self {
            registry,
            strategy,
            weights,
            aliases: HashMap::new(),
            fallbacks: HashMap::new(),
            round_robin_counter: AtomicU64::new(0),
            pipeline, // New field
        }
    }
}
```

## Error Mapping Contract

```rust
/// Map ReconcileError to RoutingError
impl From<ReconcileError> for RoutingError {
    fn from(err: ReconcileError) -> Self {
        match err {
            ReconcileError::NoCandidates => RoutingError::ModelNotFound { /* ... */ },
            ReconcileError::PrivacyViolation(msg) => RoutingError::PolicyViolation(msg),
            ReconcileError::SelectionFailed(msg) => RoutingError::NoHealthyBackends,
            _ => RoutingError::Internal(err.to_string()),
        }
    }
}
```

## Backward Compatibility Contract

### Phase 1: All Existing Tests Pass

**Requirement**: Zero breaking changes to external API

**Test Coverage**:
- `tests/routing/*.rs` - All routing tests pass without modification
- `benches/routing.rs` - Performance within constitutional limits
- Integration tests - Router behavior identical

### Phase 2: Gradual Migration

**Strategy**: Feature flag for pipeline vs legacy

```rust
pub struct Router {
    // ...existing fields...
    pipeline: Option<ReconcilerPipeline>, // None = legacy path
}

impl Router {
    pub fn select_backend(&self, requirements: &RequestRequirements) 
        -> Result<RoutingResult, RoutingError> 
    {
        if let Some(pipeline) = &self.pipeline {
            // New path: use pipeline
            self.select_backend_async_with_pipeline(requirements)
        } else {
            // Legacy path: use imperative logic
            self.select_backend_legacy(requirements)
        }
    }
}
```

## Testing Contract

### Unit Tests

**Router must test**:
1. Pipeline integration (intent creation, result conversion)
2. Error mapping (ReconcileError → RoutingError)
3. Fallback chain handling
4. Alias resolution before pipeline

### Integration Tests

**Must verify**:
1. All existing routing tests pass
2. New privacy filtering works
3. New budget tracking works
4. Error messages are actionable

### Performance Tests

**Must benchmark**:
1. `select_backend()` with pipeline <1ms
2. No regression vs baseline
3. Memory usage stable

---

**Contract Version**: 1.0  
**Date**: 2024-02-15  
**Status**: Active
