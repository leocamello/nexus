//! Capability tier enforcement reconciler

use async_trait::async_trait;
use std::collections::HashMap;

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

/// Reconciler for capability tier enforcement
pub struct CapabilityReconciler {
    // Future: Add configuration for tier requirements
}

impl CapabilityReconciler {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for CapabilityReconciler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl crate::control::reconciler::Reconciler for CapabilityReconciler {
    async fn reconcile(
        &self,
        intent: &mut crate::control::intent::RoutingIntent,
    ) -> Result<(), crate::control::reconciler::ReconcileError> {
        // Extract required tier from request requirements
        let required_tier = intent.request_requirements.min_capability_tier;

        if let Some(tier) = required_tier {
            intent.annotations.required_tier = Some(tier);

            // Filter backends
            let mut excluded = HashMap::new();
            intent.candidate_backends.retain(|backend| {
                // Get backend tier from metadata
                let backend_tier = backend
                    .metadata
                    .get("capability_tier")
                    .and_then(|v| v.parse::<u8>().ok());

                match backend_tier {
                    Some(bt) if bt >= tier => true,
                    Some(bt) => {
                        excluded.insert(
                            backend.name.clone(),
                            CapabilityMismatch::tier_mismatch(tier, bt),
                        );
                        false
                    }
                    None => {
                        // No tier specified, allow for now
                        true
                    }
                }
            });

            intent.annotations.capability_excluded = excluded;

            let allowed = intent.candidate_backends.len();
            let blocked = intent.annotations.capability_excluded.len();
            intent.trace(format!(
                "Capability: tier {} required, {} allowed, {} blocked",
                tier, allowed, blocked
            ));
        } else {
            intent.trace("Capability: no tier requirement".to_string());
        }

        Ok(())
    }

    fn error_policy(&self) -> crate::control::reconciler::ReconcileErrorPolicy {
        crate::control::reconciler::ReconcileErrorPolicy::FailOpen // Graceful degradation
    }

    fn name(&self) -> &str {
        "CapabilityReconciler"
    }
}
