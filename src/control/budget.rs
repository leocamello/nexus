//! Budget tracking and enforcement reconciler

use async_trait::async_trait;
use std::collections::HashMap;

/// Budget status for cost-aware routing
#[derive(Debug, Clone, Copy, PartialEq)]
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
            BudgetStatus::HardLimit { current, limit } => current + estimated_cost <= *limit,
        }
    }

    /// Should prefer lower-cost options
    pub fn prefer_cheaper(&self) -> bool {
        matches!(self, BudgetStatus::SoftLimit { .. })
    }
}

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

/// Reconciler for budget tracking and enforcement
pub struct BudgetReconciler {
    /// Cost per token by backend type
    cost_model: HashMap<String, f64>,

    /// Monthly budget limit (if configured)
    monthly_limit: Option<f64>,
}

impl BudgetReconciler {
    /// Create new budget reconciler
    pub fn new(cost_model: HashMap<String, f64>, monthly_limit: Option<f64>) -> Self {
        Self {
            cost_model,
            monthly_limit,
        }
    }

    /// Estimate cost for request on specific backend
    fn estimate_cost(&self, backend: &crate::registry::Backend, tokens: u32) -> f64 {
        let backend_type_str = format!("{:?}", backend.backend_type); // Use Debug format
        let cost_per_token = self
            .cost_model
            .get(&backend_type_str)
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
impl crate::control::reconciler::Reconciler for BudgetReconciler {
    async fn reconcile(
        &self,
        intent: &mut crate::control::intent::RoutingIntent,
    ) -> Result<(), crate::control::reconciler::ReconcileError> {
        let tokens = intent.request_requirements.estimated_tokens;

        // Estimate cost for request
        let estimated_cost = intent
            .candidate_backends
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
                        BudgetViolation::new(
                            backend_cost,
                            current_usage,
                            if let BudgetStatus::HardLimit { limit, .. } = budget_status {
                                limit
                            } else {
                                0.0
                            },
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

    fn error_policy(&self) -> crate::control::reconciler::ReconcileErrorPolicy {
        crate::control::reconciler::ReconcileErrorPolicy::FailOpen // Can estimate locally
    }

    fn name(&self) -> &str {
        "BudgetReconciler"
    }
}
