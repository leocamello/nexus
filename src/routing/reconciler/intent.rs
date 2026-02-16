//! Routing intent and related data structures
//!
//! Shared state object passed through the reconciler pipeline.

use crate::agent::PrivacyZone;
use crate::routing::RequestRequirements;
use serde::Serialize;

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::RequestRequirements;

    fn default_requirements() -> RequestRequirements {
        RequestRequirements {
            model: "llama3:8b".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
        }
    }

    #[test]
    fn new_intent_has_all_candidates() {
        let agents = vec!["a1".to_string(), "a2".to_string(), "a3".to_string()];
        let intent = RoutingIntent::new(
            "req-1".to_string(),
            "llama3:8b".to_string(),
            "llama3:8b".to_string(),
            default_requirements(),
            agents.clone(),
        );

        assert_eq!(intent.candidate_agents, agents);
        assert!(intent.excluded_agents.is_empty());
        assert!(intent.rejection_reasons.is_empty());
    }

    #[test]
    fn new_intent_defaults() {
        let intent = RoutingIntent::new(
            "req-1".to_string(),
            "model-a".to_string(),
            "model-b".to_string(),
            default_requirements(),
            vec![],
        );

        assert_eq!(intent.request_id, "req-1");
        assert_eq!(intent.requested_model, "model-a");
        assert_eq!(intent.resolved_model, "model-b");
        assert!(intent.privacy_constraint.is_none());
        assert!(intent.min_capability_tier.is_none());
        assert_eq!(intent.tier_enforcement_mode, TierEnforcementMode::Strict);
        assert_eq!(intent.budget_status, BudgetStatus::Normal);
        assert!(intent.route_reason.is_none());
    }

    #[test]
    fn exclude_agent_removes_from_candidates() {
        let mut intent = RoutingIntent::new(
            "req-1".to_string(),
            "llama3:8b".to_string(),
            "llama3:8b".to_string(),
            default_requirements(),
            vec!["a1".to_string(), "a2".to_string(), "a3".to_string()],
        );

        intent.exclude_agent(
            "a2".to_string(),
            "TestReconciler",
            "test reason".to_string(),
            "test action".to_string(),
        );

        assert_eq!(intent.candidate_agents, vec!["a1", "a3"]);
        assert_eq!(intent.excluded_agents, vec!["a2"]);
        assert_eq!(intent.rejection_reasons.len(), 1);
        assert_eq!(intent.rejection_reasons[0].agent_id, "a2");
        assert_eq!(intent.rejection_reasons[0].reconciler, "TestReconciler");
    }

    #[test]
    fn exclude_multiple_agents() {
        let mut intent = RoutingIntent::new(
            "req-1".to_string(),
            "llama3:8b".to_string(),
            "llama3:8b".to_string(),
            default_requirements(),
            vec!["a1".to_string(), "a2".to_string(), "a3".to_string()],
        );

        intent.exclude_agent(
            "a1".to_string(),
            "R1",
            "reason1".to_string(),
            "action1".to_string(),
        );
        intent.exclude_agent(
            "a3".to_string(),
            "R2",
            "reason2".to_string(),
            "action2".to_string(),
        );

        assert_eq!(intent.candidate_agents, vec!["a2"]);
        assert_eq!(intent.excluded_agents.len(), 2);
        assert_eq!(intent.rejection_reasons.len(), 2);
    }

    #[test]
    fn exclude_all_agents_leaves_empty_candidates() {
        let mut intent = RoutingIntent::new(
            "req-1".to_string(),
            "llama3:8b".to_string(),
            "llama3:8b".to_string(),
            default_requirements(),
            vec!["a1".to_string()],
        );

        intent.exclude_agent(
            "a1".to_string(),
            "R1",
            "reason".to_string(),
            "action".to_string(),
        );

        assert!(intent.candidate_agents.is_empty());
    }

    #[test]
    fn exclude_nonexistent_agent_adds_to_excluded() {
        let mut intent = RoutingIntent::new(
            "req-1".to_string(),
            "llama3:8b".to_string(),
            "llama3:8b".to_string(),
            default_requirements(),
            vec!["a1".to_string()],
        );

        intent.exclude_agent(
            "nonexistent".to_string(),
            "R1",
            "reason".to_string(),
            "action".to_string(),
        );

        // a1 still in candidates, nonexistent added to excluded
        assert_eq!(intent.candidate_agents, vec!["a1"]);
        assert_eq!(intent.excluded_agents, vec!["nonexistent"]);
    }

    #[test]
    fn budget_status_default_is_normal() {
        assert_eq!(BudgetStatus::default(), BudgetStatus::Normal);
    }

    #[test]
    fn tier_enforcement_mode_default_is_strict() {
        assert_eq!(TierEnforcementMode::default(), TierEnforcementMode::Strict);
    }

    #[test]
    fn cost_estimate_default_is_zero() {
        let est = CostEstimate::default();
        assert_eq!(est.input_tokens, 0);
        assert_eq!(est.estimated_output_tokens, 0);
        assert_eq!(est.cost_usd, 0.0);
        assert_eq!(est.token_count_tier, 0);
    }

    #[test]
    fn rejection_reason_serialization() {
        let reason = RejectionReason {
            agent_id: "b1".to_string(),
            reconciler: "Privacy".to_string(),
            reason: "Cloud not allowed".to_string(),
            suggested_action: "Use local backend".to_string(),
        };

        let json = serde_json::to_string(&reason).unwrap();
        assert!(json.contains("b1"));
        assert!(json.contains("Privacy"));
        assert!(json.contains("Cloud not allowed"));

        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["agent_id"], "b1");
        assert_eq!(value["reconciler"], "Privacy");
    }
}
