//! Privacy zone enforcement reconciler

use crate::agent::types::PrivacyZone;
use crate::config::routing::{OverflowMode, RoutingConfig, TrafficPolicy};
use async_trait::async_trait;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Overflow decision for cross-zone routing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverflowDecision {
    /// Overflow allowed (fresh conversation, no history)
    AllowedFresh,
    /// Overflow blocked due to conversation history
    BlockedWithHistory,
    /// Overflow blocked by policy (BlockEntirely)
    BlockedByPolicy,
    /// Overflow not needed (sufficient backends in required zone)
    NotNeeded,
}

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
    /// Routing configuration for traffic policies
    routing_config: RoutingConfig,
}

impl PrivacyReconciler {
    pub fn new(default_constraint: PrivacyConstraint, routing_config: RoutingConfig) -> Self {
        Self {
            default_constraint,
            routing_config,
        }
    }

    /// Extract privacy constraint from request or TrafficPolicy
    fn get_constraint(
        &self,
        intent: &mut crate::control::intent::RoutingIntent,
    ) -> PrivacyConstraint {
        let model = &intent.request_requirements.model;

        // Find matching traffic policy (highest priority first)
        if let Some(policy) = self.find_matching_policy(model) {
            if let Some(privacy_zone) = policy.privacy {
                // Store applied policy name
                if intent.annotations.applied_policy.is_none() {
                    intent.annotations.applied_policy = Some(policy.model_pattern.clone());
                }
                return PrivacyConstraint::Zone(privacy_zone);
            }
        }

        // Fall back to request requirements or default
        intent
            .request_requirements
            .privacy_zone
            .map(PrivacyConstraint::Zone)
            .unwrap_or(self.default_constraint)
    }

    /// Find the highest-priority matching TrafficPolicy for a model
    fn find_matching_policy(&self, model: &str) -> Option<&TrafficPolicy> {
        let mut matching: Vec<&TrafficPolicy> = self
            .routing_config
            .policies
            .iter()
            .filter(|p| p.matches(model))
            .collect();

        // Sort by priority (highest first)
        matching.sort_by_key(|p| std::cmp::Reverse(p.priority()));

        matching.first().copied()
    }

    /// Check if backend satisfies privacy constraint
    fn check_backend(
        &self,
        backend: &crate::registry::Backend,
        constraint: PrivacyConstraint,
    ) -> Result<(), PrivacyViolation> {
        // Get backend's privacy zone
        // Priority: metadata > backend_type default
        let backend_zone = backend
            .metadata
            .get("privacy_zone")
            .and_then(|v| match v.as_str() {
                "restricted" | "Restricted" => Some(PrivacyZone::Restricted),
                "open" | "Open" => Some(PrivacyZone::Open),
                _ => None,
            })
            .unwrap_or_else(|| {
                // Default based on backend type
                // OpenAI and Anthropic are always Open (cloud providers)
                use crate::registry::BackendType;
                match backend.backend_type {
                    BackendType::OpenAI | BackendType::Anthropic => PrivacyZone::Open,
                    _ => PrivacyZone::Restricted, // Local backends default to Restricted
                }
            });

        if constraint.allows_backend(backend_zone) {
            Ok(())
        } else {
            Err(PrivacyViolation::new(backend_zone, constraint))
        }
    }

    /// Compute affinity key for sticky routing
    fn compute_affinity_key(
        &self,
        intent: &crate::control::intent::RoutingIntent,
    ) -> Option<String> {
        // Hash the model name for affinity
        let mut hasher = DefaultHasher::new();
        intent.request_requirements.model.hash(&mut hasher);
        Some(format!("{:x}", hasher.finish()))
    }

    /// Select backend with affinity (consistent hashing)
    #[allow(dead_code)]
    fn select_with_affinity(
        &self,
        backends: &[std::sync::Arc<crate::registry::Backend>],
        affinity_key: &str,
    ) -> Option<std::sync::Arc<crate::registry::Backend>> {
        if backends.is_empty() {
            return None;
        }

        // Simple consistent hashing: hash affinity_key mod backends.len()
        let mut hasher = DefaultHasher::new();
        affinity_key.hash(&mut hasher);
        let index = (hasher.finish() as usize) % backends.len();

        backends.get(index).cloned()
    }

    /// Check if cross-zone overflow is allowed by policy
    #[allow(dead_code)] // Will be used in future overflow implementation
    fn allows_cross_zone_overflow(
        &self,
        intent: &crate::control::intent::RoutingIntent,
        from_zone: PrivacyZone,
        to_zone: PrivacyZone,
    ) -> OverflowDecision {
        // Only consider overflow from Restricted to Open
        if from_zone != PrivacyZone::Restricted || to_zone != PrivacyZone::Open {
            return OverflowDecision::NotNeeded;
        }

        // Find matching policy for overflow_mode
        let model = &intent.request_requirements.model;
        let overflow_mode = self
            .find_matching_policy(model)
            .map(|p| p.overflow_mode)
            .unwrap_or(OverflowMode::BlockEntirely);

        match overflow_mode {
            OverflowMode::BlockEntirely => OverflowDecision::BlockedByPolicy,
            OverflowMode::FreshOnly => {
                if intent.request_requirements.has_conversation_history {
                    OverflowDecision::BlockedWithHistory
                } else {
                    OverflowDecision::AllowedFresh
                }
            }
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
                    tracing::debug!(
                        backend = %backend.name,
                        backend_zone = ?violation.backend_zone,
                        required_constraint = ?violation.required_constraint,
                        "Privacy zone mismatch"
                    );
                    excluded.insert(backend.name.clone(), violation);
                    false
                }
            });

        intent.annotations.privacy_excluded = excluded;

        // Compute and store affinity key for sticky routing
        if let Some(affinity_key) = self.compute_affinity_key(intent) {
            intent.annotations.affinity_key = Some(affinity_key);
        }

        // Log results
        let allowed = intent.candidate_backends.len();
        let blocked = intent.annotations.privacy_excluded.len();
        intent.trace(format!(
            "Privacy: {} allowed, {} blocked (constraint: {:?})",
            allowed, blocked, constraint
        ));

        Ok(())
    }

    fn error_policy(&self) -> crate::control::reconciler::ReconcileErrorPolicy {
        crate::control::reconciler::ReconcileErrorPolicy::FailClosed // Never compromise privacy
    }

    fn name(&self) -> &str {
        "PrivacyReconciler"
    }
}
