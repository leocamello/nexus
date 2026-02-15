# Data Model: Control Plane Reconciler Pipeline

**Feature**: Control Plane — Reconciler Pipeline (RFC-001 Phase 2)  
**Date**: 2024-02-15  
**Status**: Design Phase

## Overview

This document defines the core data structures for the reconciler pipeline architecture. The model is designed to enable independent policy evaluation while maintaining sub-millisecond routing performance.

## Core Entities

### RoutingIntent

The central state object that flows through the reconciler pipeline. Each reconciler annotates this shared state without requiring knowledge of other reconcilers.

```rust
/// Shared routing state annotated by reconcilers
#[derive(Debug, Clone)]
pub struct RoutingIntent {
    /// Original request requirements (immutable)
    pub request_requirements: RequestRequirements,
    
    /// Candidate backends (filtered by reconcilers)
    pub candidate_backends: Vec<Arc<Backend>>,
    
    /// Policy annotations (written by reconcilers)
    pub annotations: RoutingAnnotations,
    
    /// Final routing decision (set by SelectionReconciler)
    pub decision: Option<RoutingDecision>,
}

impl RoutingIntent {
    /// Create new intent from request requirements and candidates
    pub fn new(
        request_requirements: RequestRequirements,
        candidate_backends: Vec<Arc<Backend>>,
    ) -> Self {
        Self {
            request_requirements,
            candidate_backends,
            annotations: RoutingAnnotations::default(),
            decision: None,
        }
    }
    
    /// Check if any backend is available after filtering
    pub fn has_candidates(&self) -> bool {
        !self.candidate_backends.is_empty()
    }
    
    /// Add trace information for observability
    pub fn trace(&mut self, message: impl Into<String>) {
        self.annotations.trace_info.push(message.into());
    }
}
```

**Fields**:
- `request_requirements`: Immutable input from original request (model, tokens, capabilities)
- `candidate_backends`: Mutable list filtered by reconcilers (privacy, budget, capability)
- `annotations`: Mutable policy annotations added by each reconciler
- `decision`: Final output set by SelectionReconciler

**Relationships**:
- Contains `RequestRequirements` (from routing/requirements.rs)
- Contains `Arc<Backend>` references (from registry/backend.rs)
- Produces `RoutingDecision` consumed by Router

**State Transitions**:
1. **Created**: With full candidate list from Registry
2. **Privacy Filtered**: PrivacyReconciler removes non-compliant backends
3. **Budget Annotated**: BudgetReconciler adds cost estimates
4. **Capability Filtered**: CapabilityReconciler removes insufficient backends
5. **Selected**: SelectionReconciler chooses final backend

---

### RoutingAnnotations

Policy-specific annotations written by reconcilers. All fields are optional to support independent reconciler operation.

```rust
/// Policy annotations added by reconcilers
#[derive(Debug, Clone, Default)]
pub struct RoutingAnnotations {
    // Privacy Policy
    /// Privacy constraints extracted from request or user profile
    pub privacy_constraints: Option<PrivacyConstraint>,
    
    /// Backends excluded due to privacy violations
    pub privacy_excluded: HashMap<String, PrivacyViolation>,
    
    // Budget Policy
    /// Estimated cost for this request
    pub estimated_cost: Option<f64>,
    
    /// Current budget status (normal, soft limit, hard limit)
    pub budget_status: Option<BudgetStatus>,
    
    /// Backends excluded due to budget constraints
    pub budget_excluded: HashMap<String, BudgetViolation>,
    
    // Capability Policy
    /// Required capability tier (if specified)
    pub required_tier: Option<u8>,
    
    /// Backends excluded due to capability mismatches
    pub capability_excluded: HashMap<String, CapabilityMismatch>,
    
    // Observability
    /// Trace messages for debugging and audit
    pub trace_info: Vec<String>,
    
    /// Whether fallback model was used
    pub fallback_used: bool,
}
```

**Design Principles**:
- All fields Optional to support reconciler independence
- HashMap for exclusions allows detailed per-backend reasons
- trace_info enables observability without affecting performance
- Default implementation provides clean initialization

---

### PrivacyConstraint

Represents privacy requirements for the request.

```rust
/// Privacy requirements for routing decision
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyConstraint {
    /// No privacy restrictions (can use any backend)
    Unrestricted,
    
    /// Must use local backends only (no cloud)
    Restricted,
    
    /// Custom zone (for future organization-specific zones)
    Zone(PrivacyZone),
}

impl PrivacyConstraint {
    /// Check if backend is allowed under this constraint
    pub fn allows_backend(&self, backend_zone: PrivacyZone) -> bool {
        match (self, backend_zone) {
            (PrivacyConstraint::Unrestricted, _) => true,
            (PrivacyConstraint::Restricted, PrivacyZone::Restricted) => true,
            (PrivacyConstraint::Restricted, PrivacyZone::Open) => false,
            (PrivacyConstraint::Zone(required), zone) => required == zone,
        }
    }
}
```

**Validation Rules**:
- Restricted constraint → only PrivacyZone::Restricted backends allowed
- Unrestricted constraint → all backends allowed
- Zone constraint → only matching zone allowed

---

### PrivacyViolation

Detailed reason why a backend was excluded by privacy policy.

```rust
/// Reason a backend was excluded by privacy policy
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyViolation {
    /// Backend's privacy zone
    pub backend_zone: PrivacyZone,
    
    /// Required privacy constraint
    pub required_constraint: PrivacyConstraint,
    
    /// Human-readable explanation
    pub message: String,
}

impl PrivacyViolation {
    pub fn new(
        backend_zone: PrivacyZone,
        required_constraint: PrivacyConstraint,
    ) -> Self {
        let message = format!(
            "Backend zone {:?} does not satisfy constraint {:?}",
            backend_zone, required_constraint
        );
        Self {
            backend_zone,
            required_constraint,
            message,
        }
    }
}
```

---

### BudgetStatus

Current budget state for routing decisions.

```rust
/// Budget status for cost-aware routing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetStatus {
    /// Normal operation (under soft limit)
    Normal,
    
    /// Approaching limit (prefer cheaper options)
    SoftLimit {
        /// Percentage of budget used (0-100)
        usage_percent: u8,
    },
    
    /// Hard limit reached (block expensive operations)
    HardLimit {
        /// Current spend
        current: f64,
        /// Maximum allowed
        limit: f64,
    },
}

impl BudgetStatus {
    /// Check if backend is affordable under current budget
    pub fn allows_cost(&self, estimated_cost: f64) -> bool {
        match self {
            BudgetStatus::Normal => true,
            BudgetStatus::SoftLimit { .. } => true, // Prefer cheaper but allow
            BudgetStatus::HardLimit { current, limit } => {
                current + estimated_cost <= *limit
            }
        }
    }
    
    /// Should prefer lower-cost options
    pub fn prefer_cheaper(&self) -> bool {
        matches!(self, BudgetStatus::SoftLimit { .. })
    }
}
```

**State Transitions**:
- **Normal**: usage < 75% of budget
- **SoftLimit**: usage >= 75% and < 100%
- **HardLimit**: usage >= 100%

---

### BudgetViolation

Reason a backend was excluded by budget policy.

```rust
/// Reason a backend was excluded by budget policy
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetViolation {
    /// Estimated cost for this request
    pub estimated_cost: f64,
    
    /// Current budget usage
    pub current_usage: f64,
    
    /// Budget limit
    pub limit: f64,
    
    /// Human-readable explanation
    pub message: String,
}

impl BudgetViolation {
    pub fn new(estimated_cost: f64, current_usage: f64, limit: f64) -> Self {
        let message = format!(
            "Request cost ${:.2} would exceed budget (${:.2}/${:.2})",
            estimated_cost,
            current_usage + estimated_cost,
            limit
        );
        Self {
            estimated_cost,
            current_usage,
            limit,
            message,
        }
    }
}
```

---

### CapabilityMismatch

Reason a backend was excluded by capability policy.

```rust
/// Reason a backend was excluded by capability policy
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityMismatch {
    /// Required capability tier (if tiered routing enabled)
    pub required_tier: Option<u8>,
    
    /// Backend's capability tier
    pub backend_tier: Option<u8>,
    
    /// Specific missing capabilities
    pub missing_capabilities: Vec<String>,
    
    /// Human-readable explanation
    pub message: String,
}

impl CapabilityMismatch {
    pub fn tier_mismatch(required: u8, backend: u8) -> Self {
        Self {
            required_tier: Some(required),
            backend_tier: Some(backend),
            missing_capabilities: vec![],
            message: format!(
                "Backend tier {} does not meet required tier {}",
                backend, required
            ),
        }
    }
    
    pub fn missing_features(missing: Vec<String>) -> Self {
        let message = format!("Missing capabilities: {}", missing.join(", "));
        Self {
            required_tier: None,
            backend_tier: None,
            missing_capabilities: missing,
            message,
        }
    }
}
```

---

### RoutingDecision

Final routing decision produced by SelectionReconciler.

```rust
/// Final routing decision after pipeline evaluation
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    /// Selected backend
    pub backend: Arc<Backend>,
    
    /// Reason for selection (for observability)
    pub reason: String,
    
    /// Score used for selection (if applicable)
    pub score: Option<f64>,
}

impl RoutingDecision {
    /// Create decision from backend and reason
    pub fn new(backend: Arc<Backend>, reason: impl Into<String>) -> Self {
        Self {
            backend,
            reason: reason.into(),
            score: None,
        }
    }
    
    /// Create decision with score
    pub fn with_score(
        backend: Arc<Backend>,
        reason: impl Into<String>,
        score: f64,
    ) -> Self {
        Self {
            backend,
            reason: reason.into(),
            score: Some(score),
        }
    }
}
```

**Conversion to RoutingResult**:
```rust
impl From<RoutingDecision> for RoutingResult {
    fn from(decision: RoutingDecision) -> Self {
        Self {
            backend: decision.backend,
            actual_model: /* from intent */,
            fallback_used: /* from annotations */,
            route_reason: decision.reason,
        }
    }
}
```

---

## Pipeline Entities

### Reconciler Trait

Base trait for all policy reconcilers.

```rust
use async_trait::async_trait;

/// Policy reconciler that annotates routing intent
#[async_trait]
pub trait Reconciler: Send + Sync {
    /// Reconcile policy with routing intent
    /// 
    /// Reconcilers can:
    /// - Filter candidate_backends
    /// - Add annotations
    /// - Set decision (SelectionReconciler only)
    /// 
    /// # Errors
    /// 
    /// Returns Err if policy evaluation fails critically
    async fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), ReconcileError>;
    
    /// Error handling policy for this reconciler
    fn error_policy(&self) -> ReconcileErrorPolicy {
        ReconcileErrorPolicy::FailOpen
    }
    
    /// Name for logging and debugging
    fn name(&self) -> &str;
}
```

**Contract**:
- Must be Send + Sync (thread-safe)
- Must not panic (return ReconcileError instead)
- Should complete in <100μs for CPU-bound operations
- Can be async but most implementations will be synchronous

---

### ReconcilerPipeline

Orchestrates sequential execution of reconcilers.

```rust
/// Pipeline of reconcilers executed sequentially
pub struct ReconcilerPipeline {
    reconcilers: Vec<Arc<dyn Reconciler>>,
}

impl ReconcilerPipeline {
    /// Create new pipeline with reconcilers
    pub fn new(reconcilers: Vec<Arc<dyn Reconciler>>) -> Self {
        Self { reconcilers }
    }
    
    /// Execute pipeline on routing intent
    pub async fn execute(&self, intent: &mut RoutingIntent) -> Result<(), ReconcileError> {
        for reconciler in &self.reconcilers {
            // Execute reconciler
            let result = reconciler.reconcile(intent).await;
            
            // Handle errors based on policy
            if let Err(err) = result {
                match reconciler.error_policy() {
                    ReconcileErrorPolicy::FailOpen => {
                        tracing::warn!(
                            reconciler = reconciler.name(),
                            error = %err,
                            "Reconciler failed, continuing pipeline"
                        );
                        intent.trace(format!(
                            "⚠️  {} failed: {}",
                            reconciler.name(),
                            err
                        ));
                        continue;
                    }
                    ReconcileErrorPolicy::FailClosed => {
                        tracing::error!(
                            reconciler = reconciler.name(),
                            error = %err,
                            "Reconciler failed, stopping pipeline"
                        );
                        return Err(err);
                    }
                }
            }
            
            // Log success
            intent.trace(format!("✓ {}", reconciler.name()));
        }
        
        Ok(())
    }
}
```

---

### ReconcileError

Errors during reconciler execution.

```rust
use thiserror::Error;

/// Errors during reconciler execution
#[derive(Debug, Error)]
pub enum ReconcileError {
    #[error("No backends available after filtering")]
    NoCandidates,
    
    #[error("Privacy policy violation: {0}")]
    PrivacyViolation(String),
    
    #[error("Budget service unavailable: {0}")]
    BudgetServiceError(String),
    
    #[error("Required capability not available: {0}")]
    CapabilityUnavailable(String),
    
    #[error("Selection failed: {0}")]
    SelectionFailed(String),
    
    #[error("Internal error: {0}")]
    Internal(String),
}
```

---

### ReconcileErrorPolicy

Error handling strategy for reconcilers.

```rust
/// Error handling policy for reconcilers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileErrorPolicy {
    /// Log warning and continue pipeline (graceful degradation)
    FailOpen,
    
    /// Stop pipeline and return error (strict enforcement)
    FailClosed,
}
```

**Policy Assignments**:
- **PrivacyReconciler**: FailClosed (never compromise data residency)
- **BudgetReconciler**: FailOpen (operational, can estimate locally)
- **CapabilityReconciler**: FailOpen (graceful degradation)
- **SelectionReconciler**: FailClosed (must select something)

---

## Concrete Reconcilers

### PrivacyReconciler

Filters backends based on privacy zone constraints.

```rust
/// Reconciler for privacy zone enforcement
pub struct PrivacyReconciler {
    /// Default privacy constraint (from config)
    default_constraint: PrivacyConstraint,
}

impl PrivacyReconciler {
    pub fn new(default_constraint: PrivacyConstraint) -> Self {
        Self { default_constraint }
    }
    
    /// Extract privacy constraint from request or use default
    fn get_constraint(&self, intent: &RoutingIntent) -> PrivacyConstraint {
        // Future: check request headers for X-Nexus-Privacy-Zone
        // For now: use default
        self.default_constraint
    }
    
    /// Check if backend satisfies privacy constraint
    fn check_backend(
        &self,
        backend: &Backend,
        constraint: PrivacyConstraint,
    ) -> Result<(), PrivacyViolation> {
        // Get backend's privacy zone from agent profile
        let backend_zone = backend
            .agent
            .as_ref()
            .map(|a| a.profile().privacy_zone)
            .unwrap_or(PrivacyZone::Open);
        
        if constraint.allows_backend(backend_zone) {
            Ok(())
        } else {
            Err(PrivacyViolation::new(backend_zone, constraint))
        }
    }
}

#[async_trait]
impl Reconciler for PrivacyReconciler {
    async fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), ReconcileError> {
        let constraint = self.get_constraint(intent);
        intent.annotations.privacy_constraints = Some(constraint);
        
        // Filter backends
        let mut excluded = HashMap::new();
        intent.candidate_backends.retain(|backend| {
            match self.check_backend(backend, constraint) {
                Ok(()) => true,
                Err(violation) => {
                    excluded.insert(backend.name.clone(), violation);
                    false
                }
            }
        });
        
        intent.annotations.privacy_excluded = excluded;
        
        // Log results
        let allowed = intent.candidate_backends.len();
        let blocked = intent.annotations.privacy_excluded.len();
        intent.trace(format!(
            "Privacy: {} allowed, {} blocked",
            allowed, blocked
        ));
        
        Ok(())
    }
    
    fn error_policy(&self) -> ReconcileErrorPolicy {
        ReconcileErrorPolicy::FailClosed // Never compromise privacy
    }
    
    fn name(&self) -> &str {
        "PrivacyReconciler"
    }
}
```

---

### BudgetReconciler

Annotates with cost estimates and budget status.

```rust
/// Reconciler for budget tracking and enforcement
pub struct BudgetReconciler {
    /// Cost per token by backend type
    cost_model: HashMap<String, f64>,
    
    /// Monthly budget limit (if configured)
    monthly_limit: Option<f64>,
}

impl BudgetReconciler {
    /// Estimate cost for request on specific backend
    fn estimate_cost(&self, backend: &Backend, tokens: u32) -> f64 {
        let cost_per_token = self.cost_model
            .get(&backend.backend_type.to_string())
            .copied()
            .unwrap_or(0.0);
        
        tokens as f64 * cost_per_token
    }
    
    /// Get current budget status
    fn get_budget_status(&self, current_usage: f64) -> BudgetStatus {
        match self.monthly_limit {
            Some(limit) => {
                let usage_percent = ((current_usage / limit) * 100.0) as u8;
                
                if usage_percent >= 100 {
                    BudgetStatus::HardLimit {
                        current: current_usage,
                        limit,
                    }
                } else if usage_percent >= 75 {
                    BudgetStatus::SoftLimit { usage_percent }
                } else {
                    BudgetStatus::Normal
                }
            }
            None => BudgetStatus::Normal,
        }
    }
}

#[async_trait]
impl Reconciler for BudgetReconciler {
    async fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), ReconcileError> {
        let tokens = intent.request_requirements.estimated_tokens;
        
        // Estimate cost for request
        let estimated_cost = intent.candidate_backends
            .first()
            .map(|b| self.estimate_cost(b, tokens))
            .unwrap_or(0.0);
        
        intent.annotations.estimated_cost = Some(estimated_cost);
        
        // Get budget status (from metrics service in future)
        let current_usage = 0.0; // TODO: Query metrics service
        let budget_status = self.get_budget_status(current_usage);
        intent.annotations.budget_status = Some(budget_status);
        
        // Filter backends if hard limit reached
        if matches!(budget_status, BudgetStatus::HardLimit { .. }) {
            let mut excluded = HashMap::new();
            intent.candidate_backends.retain(|backend| {
                let backend_cost = self.estimate_cost(backend, tokens);
                if !budget_status.allows_cost(backend_cost) {
                    excluded.insert(
                        backend.name.clone(),
                        BudgetViolation::new(backend_cost, current_usage, 
                            if let BudgetStatus::HardLimit { limit, .. } = budget_status { 
                                limit 
                            } else { 
                                0.0 
                            }
                        ),
                    );
                    false
                } else {
                    true
                }
            });
            intent.annotations.budget_excluded = excluded;
        }
        
        intent.trace(format!(
            "Budget: ${:.2} estimated, status {:?}",
            estimated_cost, budget_status
        ));
        
        Ok(())
    }
    
    fn error_policy(&self) -> ReconcileErrorPolicy {
        ReconcileErrorPolicy::FailOpen // Can estimate locally
    }
    
    fn name(&self) -> &str {
        "BudgetReconciler"
    }
}
```

---

### SelectionReconciler

Final reconciler that selects backend from filtered candidates.

```rust
/// Reconciler that selects final backend from candidates
pub struct SelectionReconciler {
    strategy: RoutingStrategy,
    weights: ScoringWeights,
    round_robin_counter: AtomicU64,
}

#[async_trait]
impl Reconciler for SelectionReconciler {
    async fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), ReconcileError> {
        if intent.candidate_backends.is_empty() {
            return Err(ReconcileError::NoCandidates);
        }
        
        // Apply routing strategy
        let (backend, reason, score) = match self.strategy {
            RoutingStrategy::Smart => {
                let backend = self.select_smart(&intent.candidate_backends);
                let score = score_backend(/* ... */);
                (backend, format!("highest_score"), Some(score))
            }
            RoutingStrategy::RoundRobin => {
                let idx = self.round_robin_counter
                    .fetch_add(1, Ordering::Relaxed) as usize 
                    % intent.candidate_backends.len();
                let backend = intent.candidate_backends[idx].clone();
                (backend, format!("round_robin:index_{}", idx), None)
            }
            // ... other strategies
        };
        
        // Set decision
        intent.decision = Some(if let Some(score) = score {
            RoutingDecision::with_score(backend, reason, score)
        } else {
            RoutingDecision::new(backend, reason)
        });
        
        intent.trace("Selection complete");
        Ok(())
    }
    
    fn error_policy(&self) -> ReconcileErrorPolicy {
        ReconcileErrorPolicy::FailClosed // Must select
    }
    
    fn name(&self) -> &str {
        "SelectionReconciler"
    }
}
```

---

## Type Conversions

### RequestRequirements Extensions

Add optional fields to existing type:

```rust
// In routing/requirements.rs
pub struct RequestRequirements {
    // Existing fields
    pub model: String,
    pub estimated_tokens: u32,
    pub needs_vision: bool,
    pub needs_tools: bool,
    pub needs_json_mode: bool,
    
    // New optional fields for Phase 2
    pub privacy_zone: Option<PrivacyZone>,
    pub budget_limit: Option<f64>,
    pub min_capability_tier: Option<u8>,
}
```

---

## Summary

### Entity Count
- **Core Types**: 3 (RoutingIntent, RoutingAnnotations, RoutingDecision)
- **Policy Types**: 6 (PrivacyConstraint, PrivacyViolation, BudgetStatus, BudgetViolation, CapabilityMismatch, ReconcileError)
- **Infrastructure Types**: 3 (Reconciler trait, ReconcilerPipeline, ReconcileErrorPolicy)
- **Concrete Reconcilers**: 3 (Privacy, Budget, Selection)

### Relationships
- RoutingIntent **contains** RoutingAnnotations
- RoutingIntent **contains** Arc<Backend> (from Registry)
- RoutingIntent **produces** RoutingDecision
- Reconciler **annotates** RoutingIntent
- ReconcilerPipeline **orchestrates** Reconcilers

### Validation Rules
- PrivacyConstraint::Restricted → only PrivacyZone::Restricted backends
- BudgetStatus::HardLimit → block requests exceeding limit
- CapabilityTier → backend tier >= required tier
- SelectionReconciler → must produce decision or error

---

**Data Model Complete**: 2024-02-15  
**Next Phase**: Contracts & API Definitions
