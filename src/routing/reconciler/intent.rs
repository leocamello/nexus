//! Routing intent and related data structures
//!
//! Shared state object passed through the reconciler pipeline.

use crate::agent::PrivacyZone;
use crate::routing::RequestRequirements;

/// Tier enforcement mode from request headers (FR-027, FR-028)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TierEnforcementMode {
    /// Default: strict enforcement — reject agents below min_tier (FR-027)
    #[default]
    Strict,

    /// Flexible: allow fallback to lower tiers when no capable agents remain (FR-028)
    Flexible,
}

/// Current budget status affecting routing decisions (FR-019)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BudgetStatus {
    /// Spending below soft limit (75% default) - all agents available
    #[default]
    Normal,

    /// Spending between soft and hard limit - prefer local agents
    SoftLimit,

    /// Spending at or above hard limit - block cloud agents
    HardLimit,
}

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

    /// Tier enforcement mode from request headers (FR-027, FR-028)
    pub tier_enforcement_mode: TierEnforcementMode,

    // === Budget State ===
    /// Current budget status from BudgetReconciler (FR-019)
    pub budget_status: BudgetStatus,

    /// Estimated cost for this request (FR-018)
    pub cost_estimate: CostEstimate,

    // === Agent Selection ===
    /// Agents still eligible for routing
    pub candidate_agents: Vec<String>, // AgentIDs

    /// Agents excluded with reasons
    pub excluded_agents: Vec<String>, // AgentIDs

    /// Detailed rejection reasons per agent
    pub rejection_reasons: Vec<RejectionReason>,

    /// Route reason set by SchedulerReconciler (for RoutingResult compatibility)
    pub route_reason: Option<String>,
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
            tier_enforcement_mode: TierEnforcementMode::default(),
            budget_status: BudgetStatus::Normal,
            cost_estimate: CostEstimate::default(),
            candidate_agents: all_agents,
            excluded_agents: Vec::new(),
            rejection_reasons: Vec::new(),
            route_reason: None,
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
