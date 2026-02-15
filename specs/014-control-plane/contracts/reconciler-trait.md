# Reconciler Contract

**Feature**: Control Plane Reconciler Pipeline  
**Contract Type**: Trait Definition  
**Date**: 2024-02-15

## Overview

The `Reconciler` trait defines the contract for all policy reconcilers in the routing pipeline. Reconcilers are independent components that annotate shared `RoutingIntent` state to influence routing decisions.

## Trait Definition

```rust
use async_trait::async_trait;

/// Policy reconciler that annotates routing intent
///
/// # Contract Requirements
///
/// 1. **Independence**: Reconcilers must not depend on execution order or other reconcilers
/// 2. **Idempotence**: Multiple calls with same input should produce same output
/// 3. **Performance**: Must complete in <100μs for CPU-bound operations
/// 4. **Safety**: Must not panic - return ReconcileError instead
/// 5. **Thread Safety**: Must be Send + Sync for concurrent pipeline execution
///
#[async_trait]
pub trait Reconciler: Send + Sync {
    async fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), ReconcileError>;
    
    fn error_policy(&self) -> ReconcileErrorPolicy {
        ReconcileErrorPolicy::FailOpen
    }
    
    fn name(&self) -> &str;
}
```

## Standard Reconciler Implementations

### 1. PrivacyReconciler
- Filters backends based on PrivacyZone compatibility
- **Error Policy**: FailClosed (privacy is non-negotiable)

### 2. BudgetReconciler
- Estimates request cost using token count
- **Error Policy**: FailOpen (can estimate locally)

### 3. CapabilityReconciler
- Checks backends for required capabilities
- **Error Policy**: FailOpen (degrade gracefully)

### 4. SelectionReconciler
- Selects single backend from filtered candidates
- **Error Policy**: FailClosed (must select something)

## Performance Contract

| Component | Target | Maximum |
|-----------|--------|---------|
| PrivacyReconciler | <50μs | 100μs |
| BudgetReconciler | <100μs | 200μs |
| CapabilityReconciler | <50μs | 100μs |
| SelectionReconciler | <200μs | 500μs |
| **Total Pipeline** | **<500μs** | **<1ms** |

---

**Contract Version**: 1.0  
**Date**: 2024-02-15  
**Status**: Active
