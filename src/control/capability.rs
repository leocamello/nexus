//! Capability tier enforcement reconciler

use crate::config::backend::CapabilityTier;
use crate::config::routing::{CapabilityRequirements, RoutingConfig, TrafficPolicy};
use crate::routing::requirements::RoutingPreference;
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

    /// Create mismatch for specific capability dimension
    pub fn reasoning_mismatch(required: u8, actual: u8) -> Self {
        Self {
            required_tier: Some(required),
            backend_tier: Some(actual),
            missing_capabilities: vec!["reasoning".to_string()],
            message: format!(
                "Backend reasoning score {} is below required {}",
                actual, required
            ),
        }
    }

    pub fn coding_mismatch(required: u8, actual: u8) -> Self {
        Self {
            required_tier: Some(required),
            backend_tier: Some(actual),
            missing_capabilities: vec!["coding".to_string()],
            message: format!(
                "Backend coding score {} is below required {}",
                actual, required
            ),
        }
    }

    pub fn context_mismatch(required: u32, actual: u32) -> Self {
        Self {
            required_tier: None,
            backend_tier: None,
            missing_capabilities: vec!["context_window".to_string()],
            message: format!(
                "Backend context window {} is below required {}",
                actual, required
            ),
        }
    }
}

/// Reconciler for capability tier enforcement
pub struct CapabilityReconciler {
    /// Routing configuration for traffic policies
    routing_config: RoutingConfig,
}

impl CapabilityReconciler {
    pub fn new(routing_config: RoutingConfig) -> Self {
        Self { routing_config }
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

    /// Get capability requirements from TrafficPolicy or request
    fn get_requirements(
        &self,
        intent: &mut crate::control::intent::RoutingIntent,
    ) -> Option<CapabilityRequirements> {
        let model = &intent.request_requirements.model;

        // Check TrafficPolicy first
        if let Some(policy) = self.find_matching_policy(model) {
            if let Some(ref caps) = policy.capabilities {
                // Store applied policy name if not already set
                if intent.annotations.applied_policy.is_none() {
                    intent.annotations.applied_policy = Some(policy.model_pattern.clone());
                }
                return Some(caps.clone());
            }
        }

        // No explicit requirements
        None
    }

    /// Check if backend capabilities meet requirements (multi-dimensional)
    fn check_backend_capabilities(
        &self,
        backend: &crate::registry::Backend,
        requirements: &CapabilityRequirements,
        routing_preference: RoutingPreference,
    ) -> Result<(), CapabilityMismatch> {
        // Get backend's capability tier from metadata
        // In a real implementation, this would be stored when backend is registered
        let backend_tier = self.get_backend_capability_tier(backend);

        // If no backend tier specified, allow (FailOpen policy)
        let Some(tier) = backend_tier else {
            return Ok(());
        };

        // Check reasoning score
        if let Some(required_reasoning) = requirements.min_reasoning {
            if let Some(actual) = tier.reasoning {
                if actual < required_reasoning {
                    return Err(CapabilityMismatch::reasoning_mismatch(
                        required_reasoning,
                        actual,
                    ));
                }
            } else if routing_preference == RoutingPreference::Strict {
                return Err(CapabilityMismatch::missing_features(vec![
                    "reasoning".to_string()
                ]));
            }
        }

        // Check coding score
        if let Some(required_coding) = requirements.min_coding {
            if let Some(actual) = tier.coding {
                if actual < required_coding {
                    return Err(CapabilityMismatch::coding_mismatch(required_coding, actual));
                }
            } else if routing_preference == RoutingPreference::Strict {
                return Err(CapabilityMismatch::missing_features(vec![
                    "coding".to_string()
                ]));
            }
        }

        // Check context window
        if let Some(required_context) = requirements.min_context_window {
            if let Some(actual) = tier.context_window {
                if actual < required_context {
                    return Err(CapabilityMismatch::context_mismatch(
                        required_context,
                        actual,
                    ));
                }
            } else if routing_preference == RoutingPreference::Strict {
                return Err(CapabilityMismatch::missing_features(vec![
                    "context_window".to_string()
                ]));
            }
        }

        // Check vision capability
        if requirements.vision_required && !tier.vision {
            return Err(CapabilityMismatch::missing_features(vec![
                "vision".to_string()
            ]));
        }

        // Check tools capability
        if requirements.tools_required && !tier.tools {
            return Err(CapabilityMismatch::missing_features(vec![
                "tools".to_string()
            ]));
        }

        Ok(())
    }

    /// Extract CapabilityTier from backend metadata
    fn get_backend_capability_tier(
        &self,
        backend: &crate::registry::Backend,
    ) -> Option<CapabilityTier> {
        // Try to parse JSON from metadata
        // For now, return None (backends don't have this yet)
        // In production, this would be stored during backend registration
        backend
            .metadata
            .get("capability_tier")
            .and_then(|json| serde_json::from_str(json).ok())
    }
}

#[async_trait]
impl crate::control::reconciler::Reconciler for CapabilityReconciler {
    async fn reconcile(
        &self,
        intent: &mut crate::control::intent::RoutingIntent,
    ) -> Result<(), crate::control::reconciler::ReconcileError> {
        let routing_preference = intent.request_requirements.routing_preference;

        // Get capability requirements from policy or request
        let requirements = self.get_requirements(intent);

        if let Some(ref reqs) = requirements {
            // Filter backends based on multi-dimensional capabilities
            let mut excluded = HashMap::new();
            intent.candidate_backends.retain(|backend| {
                match self.check_backend_capabilities(backend, reqs, routing_preference) {
                    Ok(()) => true,
                    Err(mismatch) => {
                        // Log tier rejection with dimension details
                        tracing::debug!(
                            backend = %backend.name,
                            required = ?reqs,
                            mismatch = %mismatch.message,
                            "Capability tier mismatch"
                        );
                        excluded.insert(backend.name.clone(), mismatch);
                        false
                    }
                }
            });

            intent.annotations.capability_excluded = excluded;

            let allowed = intent.candidate_backends.len();
            let blocked = intent.annotations.capability_excluded.len();
            intent.trace(format!(
                "Capability: requirements checked, {} allowed, {} blocked (preference: {:?})",
                allowed, blocked, routing_preference
            ));
        } else {
            // Legacy: Check simple min_capability_tier from request
            let required_tier = intent.request_requirements.min_capability_tier;

            if let Some(tier) = required_tier {
                intent.annotations.required_tier = Some(tier);

                // Filter backends
                let mut excluded = HashMap::new();
                intent.candidate_backends.retain(|backend| {
                    // Get backend tier from metadata (simple numeric tier)
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
                            // No tier specified, allow (FailOpen policy)
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
