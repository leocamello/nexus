//! Privacy zone enforcement reconciler

use crate::agent::types::PrivacyZone;
use async_trait::async_trait;
use std::collections::HashMap;

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
            (PrivacyConstraint::Zone(required), zone) => *required == zone,
        }
    }
}

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
    pub fn new(backend_zone: PrivacyZone, required_constraint: PrivacyConstraint) -> Self {
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
    fn get_constraint(&self, _intent: &crate::control::intent::RoutingIntent) -> PrivacyConstraint {
        // Future: check request headers for X-Nexus-Privacy-Zone
        // For now: use default
        self.default_constraint
    }

    /// Check if backend satisfies privacy constraint
    fn check_backend(
        &self,
        backend: &crate::registry::Backend,
        constraint: PrivacyConstraint,
    ) -> Result<(), PrivacyViolation> {
        // Get backend's privacy zone from agent profile
        let backend_zone = backend
            .metadata
            .get("privacy_zone")
            .and_then(|v| match v.as_str() {
                "restricted" => Some(PrivacyZone::Restricted),
                "open" => Some(PrivacyZone::Open),
                _ => None,
            })
            .unwrap_or(PrivacyZone::Open);

        if constraint.allows_backend(backend_zone) {
            Ok(())
        } else {
            Err(PrivacyViolation::new(backend_zone, constraint))
        }
    }
}

#[async_trait]
impl crate::control::reconciler::Reconciler for PrivacyReconciler {
    async fn reconcile(
        &self,
        intent: &mut crate::control::intent::RoutingIntent,
    ) -> Result<(), crate::control::reconciler::ReconcileError> {
        let constraint = self.get_constraint(intent);
        intent.annotations.privacy_constraints = Some(constraint);

        // Filter backends
        let mut excluded = HashMap::new();
        intent
            .candidate_backends
            .retain(|backend| match self.check_backend(backend, constraint) {
                Ok(()) => true,
                Err(violation) => {
                    excluded.insert(backend.name.clone(), violation);
                    false
                }
            });

        intent.annotations.privacy_excluded = excluded;

        // Log results
        let allowed = intent.candidate_backends.len();
        let blocked = intent.annotations.privacy_excluded.len();
        intent.trace(format!("Privacy: {} allowed, {} blocked", allowed, blocked));

        Ok(())
    }

    fn error_policy(&self) -> crate::control::reconciler::ReconcileErrorPolicy {
        crate::control::reconciler::ReconcileErrorPolicy::FailClosed // Never compromise privacy
    }

    fn name(&self) -> &str {
        "PrivacyReconciler"
    }
}
