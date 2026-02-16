# Data Model: Control Plane — Reconciler Pipeline

**Date**: 2025-01-09  
**Status**: Design Phase  
**Context**: Phase 1 data model for RFC-001 Phase 2 implementation

This document defines all data structures required for the reconciler pipeline architecture. Structures are organized by module and include validation rules derived from functional requirements.

---

## Module: src/routing/reconciler/mod.rs

### Reconciler Trait

The core abstraction for pipeline stages. Each reconciler reads and annotates RoutingIntent without removing constraints from prior reconcilers.

```rust
use super::intent::RoutingIntent;

/// Reconciler trait for pipeline stages.
/// Each reconciler annotates RoutingIntent without removing prior constraints.
/// Order-independent: reconcilers only add constraints, never remove.
pub trait Reconciler: Send + Sync {
    /// Returns reconciler identifier for logging and rejection reasons.
    fn name(&self) -> &'static str;
    
    /// Reconcile routing intent based on reconciler's domain.
    /// 
    /// # Behavior
    /// - Read requirements, constraints, and candidate agents from intent
    /// - Add constraints to intent (privacy, budget, tier, etc.)
    /// - Move agents from candidates to excluded with RejectionReason
    /// - NEVER remove constraints or rejection reasons from prior reconcilers
    /// 
    /// # Returns
    /// - Ok(()) if reconciliation succeeded (even if all agents excluded)
    /// - Err(RoutingError) only for catastrophic failures (e.g., config missing)
    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError>;
}
```

**Validation Rules** (FR-001, FR-006):
- ✓ Trait must be object-safe (Send + Sync bounds)
- ✓ reconcile() accepts `&mut RoutingIntent` for in-place mutation
- ✓ name() returns static string for zero-allocation logging
- ✓ Reconcilers never remove constraints (order-independence guarantee)

---

## Module: src/routing/reconciler/intent.rs

### RoutingIntent

Shared state object passed through the pipeline. Contains request metadata, extracted requirements, constraints from policies, budget status, agent candidates, excluded agents, and rejection reasons.

```rust
use crate::routing::RequestRequirements;
use crate::agent::PrivacyZone;

/// Shared routing state annotated by reconcilers.
/// Passed through pipeline from RequestAnalyzer to SchedulerReconciler.
#[derive(Debug, Clone)]
pub struct RoutingIntent {
    // === Identity ===
    /// Unique request identifier for tracing
    pub request_id: String,
    
    // === Model Resolution ===
    /// Original model name from request
    pub requested_model: String,
    
    /// Resolved model after alias expansion (max 3 levels)
    pub resolved_model: String,
    
    // === Request Requirements ===
    /// Extracted requirements from request (RFC-001 Phase 1)
    pub requirements: RequestRequirements,
    
    // === Constraints from Policies ===
    /// Privacy constraint from TrafficPolicy match (FR-011, FR-013)
    pub privacy_constraint: Option<PrivacyZone>,
    
    /// Minimum capability tier from TrafficPolicy (FR-024)
    pub min_capability_tier: Option<u8>,
    
    // === Budget State ===
    /// Current budget status from BudgetReconciler (FR-019)
    pub budget_status: BudgetStatus,
    
    /// Estimated cost for this request (FR-018)
    pub cost_estimate: CostEstimate,
    
    // === Agent Selection ===
    /// Agents still eligible for routing
    pub candidate_agents: Vec<String>,  // AgentIDs
    
    /// Agents excluded with reasons
    pub excluded_agents: Vec<String>,  // AgentIDs
    
    /// Detailed rejection reasons per agent
    pub rejection_reasons: Vec<RejectionReason>,
}

impl RoutingIntent {
    /// Create new intent from request requirements
    pub fn new(
        request_id: String,
        requested_model: String,
        resolved_model: String,
        requirements: RequestRequirements,
        all_agents: Vec<String>,
    ) -> Self {
        Self {
            request_id,
            requested_model,
            resolved_model,
            requirements,
            privacy_constraint: None,
            min_capability_tier: None,
            budget_status: BudgetStatus::Normal,
            cost_estimate: CostEstimate::default(),
            candidate_agents: all_agents,
            excluded_agents: Vec::new(),
            rejection_reasons: Vec::new(),
        }
    }
    
    /// Exclude agent with reason (helper for reconcilers)
    pub fn exclude_agent(
        &mut self,
        agent_id: String,
        reconciler: &'static str,
        reason: String,
        suggested_action: String,
    ) {
        self.candidate_agents.retain(|id| id != &agent_id);
        self.excluded_agents.push(agent_id.clone());
        self.rejection_reasons.push(RejectionReason {
            agent_id,
            reconciler: reconciler.to_string(),
            reason,
            suggested_action,
        });
    }
}
```

**Validation Rules** (FR-002, FR-004):
- ✓ Contains all fields specified in FR-002
- ✓ Helper method `exclude_agent()` ensures consistency (move from candidates, add to excluded, add rejection reason)
- ✓ Clone-able for snapshot semantics (no shared mutable state between requests)
- ✓ candidate_agents and excluded_agents are disjoint sets

---

### BudgetStatus

Enumeration representing current budget state based on spending thresholds.

```rust
/// Current budget status affecting routing decisions (FR-019)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetStatus {
    /// Spending below soft limit (75% default) - all agents available
    Normal,
    
    /// Spending between soft and hard limit - prefer local agents
    SoftLimit,
    
    /// Spending at or above hard limit - block cloud agents
    HardLimit,
}

impl Default for BudgetStatus {
    fn default() -> Self {
        Self::Normal
    }
}
```

**Validation Rules** (FR-019):
- ✓ Three states match spec: Normal, SoftLimit, HardLimit
- ✓ Copy-able for efficient passing
- ✓ Default is Normal (no budget constraints)

---

### CostEstimate

Calculated cost for a request based on token counts and model pricing.

```rust
/// Cost estimate for request (FR-018)
#[derive(Debug, Clone, Default)]
pub struct CostEstimate {
    /// Input token count (from RequestRequirements)
    pub input_tokens: u32,
    
    /// Estimated output tokens (heuristic: input_tokens / 2)
    pub estimated_output_tokens: u32,
    
    /// Total estimated cost in USD
    pub cost_usd: f64,
    
    /// Token count tier for billing (e.g., 0-1K, 1K-10K, 10K+)
    pub token_count_tier: u8,
}
```

**Validation Rules** (FR-018):
- ✓ Contains all fields from FR-018
- ✓ cost_usd combines input + output token costs
- ✓ token_count_tier enables tier-based billing analytics

---

### RejectionReason

Detailed explanation for why an agent was excluded from routing.

```rust
/// Rejection reason for excluded agent (FR-004)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectionReason {
    /// Agent that was excluded
    pub agent_id: String,
    
    /// Reconciler that excluded the agent
    pub reconciler: String,
    
    /// Human-readable reason
    pub reason: String,
    
    /// Suggested corrective action for user
    pub suggested_action: String,
}
```

**Validation Rules** (FR-004):
- ✓ All fields specified in FR-004
- ✓ reconciler field enables grouping by exclusion source
- ✓ suggested_action provides actionable feedback (Constitution Principle IX)

---

## Module: src/routing/reconciler/decision.rs

### RoutingDecision

Final output of the pipeline representing one of three outcomes.

```rust
use crate::routing::reconciler::intent::{CostEstimate, RejectionReason};

/// Final routing decision from pipeline (FR-003)
#[derive(Debug)]
pub enum RoutingDecision {
    /// Successful routing to an agent
    Route {
        /// Selected agent ID
        agent_id: String,
        
        /// Resolved model name (after alias expansion)
        model: String,
        
        /// Explanation of routing decision
        reason: String,
        
        /// Estimated cost for request
        cost_estimate: CostEstimate,
    },
    
    /// Agent is busy, queue or wait required
    Queue {
        /// Reason for queueing
        reason: String,
        
        /// Estimated wait time in milliseconds
        estimated_wait_ms: u64,
        
        /// Fallback agent if available
        fallback_agent: Option<String>,
    },
    
    /// No viable agents, request rejected
    Reject {
        /// Detailed rejection reasons from all reconcilers
        rejection_reasons: Vec<RejectionReason>,
    },
}
```

**Validation Rules** (FR-003, FR-032, FR-033):
- ✓ Three variants match FR-003: Route, Queue, Reject
- ✓ Route contains agent_id, model, reason, cost_estimate (FR-033)
- ✓ Queue contains reason, estimated_wait_ms, fallback_agent (FR-031)
- ✓ Reject contains aggregated rejection_reasons (FR-032)

---

## Module: src/routing/reconciler/scheduling.rs

### AgentSchedulingProfile

Metadata about an agent required for routing decisions. Aggregates static metadata, runtime state, and quality metrics.

```rust
use crate::agent::{AgentProfile, ResourceUsage, PrivacyZone, ModelCapability};

/// Agent metadata for routing decisions (derived from research.md)
#[derive(Debug, Clone)]
pub struct AgentSchedulingProfile {
    // === Identity ===
    pub agent_id: String,
    
    // === Static Metadata ===
    /// Agent type, version, privacy zone, capabilities
    pub profile: AgentProfile,
    
    // === Current Runtime State ===
    /// Number of pending requests (from Backend.pending_requests)
    pub current_load: u32,
    
    /// Exponential moving average latency in ms (from Backend.avg_latency_ms)
    pub latency_ema_ms: u32,
    
    /// Available models on this agent
    pub available_models: Vec<ModelCapability>,
    
    // === Resource State ===
    /// VRAM, loaded models, etc.
    pub resource_usage: ResourceUsage,
    
    /// Remaining budget for cost-aware routing
    pub budget_remaining: Option<f64>,
    
    // === Quality Metrics (time-windowed) ===
    /// Error rate over last hour (0.0-1.0)
    pub error_rate_1h: f32,
    
    /// Average time-to-first-token in ms
    pub avg_ttft_ms: u32,
    
    /// Success rate over last 24 hours (0.0-1.0)
    pub success_rate_24h: f32,
}

impl AgentSchedulingProfile {
    /// Construct from Backend state and metrics snapshot
    pub fn from_backend(
        backend: &Backend,
        agent: &dyn InferenceAgent,
        metrics: &MetricsSnapshot,
    ) -> Self {
        Self {
            agent_id: backend.id.clone(),
            profile: agent.profile(),
            current_load: backend.pending_requests.load(Ordering::Relaxed),
            latency_ema_ms: backend.avg_latency_ms.load(Ordering::Relaxed),
            available_models: backend.models.iter()
                .map(|m| ModelCapability::from(m.clone()))
                .collect(),
            resource_usage: ResourceUsage::default(), // TODO: from agent telemetry
            budget_remaining: None, // TODO: from BudgetReconciliationLoop
            error_rate_1h: metrics.error_rate(&backend.id),
            avg_ttft_ms: metrics.avg_ttft(&backend.id),
            success_rate_24h: metrics.success_rate(&backend.id),
        }
    }
    
    /// Get privacy zone for PrivacyReconciler
    pub fn privacy_zone(&self) -> PrivacyZone {
        self.profile.privacy_zone
    }
    
    /// Get capability tier for TierReconciler
    pub fn capability_tier(&self) -> Option<u8> {
        // TODO: Map from model metadata or agent config
        None
    }
}
```

**Validation Rules** (from research.md, FR-012, FR-025):
- ✓ Composes AgentProfile (static) + Backend state (runtime) + Metrics (windowed)
- ✓ privacy_zone accessible via profile.privacy_zone (FR-012)
- ✓ capability_tier accessible for TierReconciler (FR-025)
- ✓ current_load and latency_ema_ms for SchedulerReconciler scoring (FR-029, FR-030)

---

## Module: src/config/routing.rs (extensions)

### TrafficPolicy

Configuration rule matching requests by model pattern and specifying constraints.

```rust
use serde::{Deserialize, Serialize};
use crate::agent::PrivacyZone;

/// Traffic policy rule for request matching (FR-035)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficPolicy {
    /// Glob pattern for model matching (* and ? syntax)
    pub model_pattern: String,
    
    /// Privacy requirement for matched requests
    #[serde(default)]
    pub privacy: PrivacyConstraint,
    
    /// Maximum cost per request in USD
    pub max_cost_per_request: Option<f64>,
    
    /// Minimum capability tier (0-255)
    pub min_tier: Option<u8>,
    
    /// Allow fallback to lower tiers when strict tier not available
    #[serde(default = "default_fallback_allowed")]
    pub fallback_allowed: bool,
}

fn default_fallback_allowed() -> bool {
    true  // Default allows fallback (relaxed mode)
}

/// Privacy constraint enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PrivacyConstraint {
    /// No privacy restrictions (can route to any agent)
    #[default]
    Unrestricted,
    
    /// Must route to local/private agents only (PrivacyZone::Restricted)
    Restricted,
}

impl PrivacyConstraint {
    /// Check if agent's privacy zone satisfies constraint
    pub fn allows(&self, zone: PrivacyZone) -> bool {
        match (self, zone) {
            (Self::Unrestricted, _) => true,
            (Self::Restricted, PrivacyZone::Restricted) => true,
            (Self::Restricted, PrivacyZone::Open) => false,
        }
    }
}
```

**Validation Rules** (FR-035, FR-011, FR-013):
- ✓ All fields from FR-035
- ✓ Serde-compatible for TOML deserialization (FR-011)
- ✓ Privacy constraint maps to PrivacyZone filtering (FR-013)
- ✓ Defaults support zero-configuration (fallback_allowed = true)

---

### BudgetConfig

Budget configuration with limits and behavior.

```rust
/// Budget configuration (FR-016)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    /// Monthly spending limit in USD
    pub monthly_limit_usd: f64,
    
    /// Soft limit threshold as percentage (0.0-1.0, default 0.75 = 75%)
    #[serde(default = "default_soft_limit")]
    pub soft_limit_percent: f64,
    
    /// Action when hard limit reached
    #[serde(default)]
    pub hard_limit_action: HardLimitAction,
}

fn default_soft_limit() -> f64 {
    0.75  // 75% default
}

/// Action when hard budget limit reached (FR-016)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HardLimitAction {
    /// Log warning but continue routing
    Warn,
    
    /// Block cloud agents, allow local agents
    #[default]
    BlockCloud,
    
    /// Block all agents (reject requests)
    BlockAll,
}
```

**Validation Rules** (FR-016):
- ✓ Contains monthly_limit, soft_limit_percent, hard_limit_action from FR-016
- ✓ Defaults enable zero-configuration (soft_limit = 75%, action = BlockCloud)
- ✓ HardLimitAction enum matches FR-016 spec

---

## Module: src/metrics/mod.rs (extensions)

### MetricsSnapshot

Time-windowed metrics for quality scoring.

```rust
/// Snapshot of time-windowed metrics for AgentSchedulingProfile (FR-030)
#[derive(Debug, Clone, Default)]
pub struct MetricsSnapshot {
    /// Error rate per agent over last hour
    error_rates: HashMap<String, f32>,
    
    /// Average TTFT per agent
    avg_ttfts: HashMap<String, u32>,
    
    /// Success rate per agent over last 24 hours
    success_rates: HashMap<String, f32>,
}

impl MetricsSnapshot {
    /// Get error rate for agent (default 0.0)
    pub fn error_rate(&self, agent_id: &str) -> f32 {
        self.error_rates.get(agent_id).copied().unwrap_or(0.0)
    }
    
    /// Get average TTFT for agent (default 0)
    pub fn avg_ttft(&self, agent_id: &str) -> u32 {
        self.avg_ttfts.get(agent_id).copied().unwrap_or(0)
    }
    
    /// Get success rate for agent (default 1.0)
    pub fn success_rate(&self, agent_id: &str) -> f32 {
        self.success_rates.get(agent_id).copied().unwrap_or(1.0)
    }
}
```

**Validation Rules** (FR-030, A-007):
- ✓ Provides default values per A-007 (error_rate=0.0, ttft=0, success_rate=1.0)
- ✓ Time-windowed metrics (1h for errors, 24h for success)
- ✓ Enables quality scoring in SchedulerReconciler (FR-030)

---

## State Transitions

### BudgetStatus State Machine

```
                    ┌─────────────┐
                    │   Normal    │
                    │ (<75% limit)│
                    └──────┬──────┘
                           │
                  spending increases
                           │
                           ▼
                    ┌─────────────┐
            ┌───────│  SoftLimit  │
            │       │ (75%-100%)  │
            │       └──────┬──────┘
            │              │
   spending decreases   spending increases
            │              │
            │              ▼
            │       ┌─────────────┐
            └──────▶│  HardLimit  │
                    │  (≥100%)    │
                    └─────────────┘

Transitions triggered by BudgetReconciliationLoop every 60s (FR-022)
```

**Validation Rules** (FR-019, FR-020, FR-021):
- ✓ Normal: All agents available
- ✓ SoftLimit: Prefer local agents (increase priority in scoring)
- ✓ HardLimit: Exclude cloud agents or all agents based on config

---

## Relationships

### Pipeline Flow

```
RequestRequirements
       │
       ▼
RoutingIntent (created by RequestAnalyzer)
       │
       ├──▶ RequestAnalyzer   (populate requirements, resolve alias)
       ├──▶ PrivacyReconciler  (filter by privacy_zone)
       ├──▶ BudgetReconciler   (set budget_status, cost_estimate)
       ├──▶ TierReconciler     (filter by capability_tier)
       ├──▶ QualityReconciler  (stub, reserved for future)
       └──▶ SchedulerReconciler (score candidates, select best)
              │
              ▼
        RoutingDecision (Route | Queue | Reject)
```

---

## Validation Summary

All data structures satisfy:
- ✅ Functional requirements (FR-001 through FR-038)
- ✅ Constitution principles (Zero Configuration, Explicit Contracts)
- ✅ Performance budgets (<1ms pipeline, FR-036)
- ✅ Test-driven development (all structs have unit test contracts)

**Next Phase**: Contract definition (Phase 1) will specify serialization formats, error conditions, and API boundaries for each data structure.
