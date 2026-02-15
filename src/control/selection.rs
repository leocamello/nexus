//! Selection reconciler - final backend selection

use crate::control::decision::RoutingDecision;
use crate::control::intent::RoutingIntent;
use crate::control::reconciler::{ReconcileError, ReconcileErrorPolicy, Reconciler};
use crate::registry::Backend;
use crate::routing::scoring::{score_backend, ScoringWeights};
use crate::routing::strategies::RoutingStrategy;
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Reconciler that selects final backend from candidates
pub struct SelectionReconciler {
    strategy: RoutingStrategy,
    weights: ScoringWeights,
    round_robin_counter: AtomicU64,
}

impl SelectionReconciler {
    /// Create new selection reconciler
    pub fn new(strategy: RoutingStrategy, weights: ScoringWeights) -> Self {
        Self {
            strategy,
            weights,
            round_robin_counter: AtomicU64::new(0),
        }
    }

    /// Select backend using smart strategy
    fn select_smart(&self, candidates: &[Arc<Backend>]) -> (Arc<Backend>, f64) {
        let mut best = candidates[0].clone();
        let mut best_score = score_backend(
            best.priority as u32,
            best.pending_requests.load(Ordering::Relaxed),
            best.avg_latency_ms.load(Ordering::Relaxed),
            &self.weights,
        );

        for backend in &candidates[1..] {
            let score = score_backend(
                backend.priority as u32,
                backend.pending_requests.load(Ordering::Relaxed),
                backend.avg_latency_ms.load(Ordering::Relaxed),
                &self.weights,
            );

            if score > best_score {
                best = backend.clone();
                best_score = score;
            }
        }

        (best, best_score as f64)
    }

    /// Select backend using round-robin strategy
    fn select_round_robin(&self, candidates: &[Arc<Backend>]) -> Arc<Backend> {
        let idx =
            self.round_robin_counter.fetch_add(1, Ordering::Relaxed) as usize % candidates.len();
        candidates[idx].clone()
    }

    /// Select backend using priority-only strategy
    fn select_priority_only(&self, candidates: &[Arc<Backend>]) -> Arc<Backend> {
        candidates
            .iter()
            .min_by_key(|b| b.priority)
            .unwrap()
            .clone()
    }

    /// Select backend using random strategy
    fn select_random(&self, candidates: &[Arc<Backend>]) -> Arc<Backend> {
        use std::collections::hash_map::RandomState;
        use std::hash::BuildHasher;

        let random_state = RandomState::new();
        let idx = (random_state.hash_one(std::time::SystemTime::now()) as usize) % candidates.len();
        candidates[idx].clone()
    }
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
                let (backend, score) = self.select_smart(&intent.candidate_backends);
                let reason = if intent.candidate_backends.len() == 1 {
                    "only_healthy_backend".to_string()
                } else {
                    format!("highest_score:{}:{:.2}", backend.name, score)
                };
                (backend, reason, Some(score))
            }
            RoutingStrategy::RoundRobin => {
                let backend = self.select_round_robin(&intent.candidate_backends);
                let idx = (self.round_robin_counter.load(Ordering::Relaxed) - 1) as usize
                    % intent.candidate_backends.len();
                let reason = if intent.candidate_backends.len() == 1 {
                    "only_healthy_backend".to_string()
                } else {
                    format!("round_robin:index_{}", idx)
                };
                (backend, reason, None)
            }
            RoutingStrategy::PriorityOnly => {
                let backend = self.select_priority_only(&intent.candidate_backends);
                let reason = if intent.candidate_backends.len() == 1 {
                    "only_healthy_backend".to_string()
                } else {
                    format!("priority:{}:{}", backend.name, backend.priority)
                };
                (backend, reason, None)
            }
            RoutingStrategy::Random => {
                let backend = self.select_random(&intent.candidate_backends);
                let reason = if intent.candidate_backends.len() == 1 {
                    "only_healthy_backend".to_string()
                } else {
                    format!("random:{}", backend.name)
                };
                (backend, reason, None)
            }
        };

        // Set decision
        intent.decision = Some(if let Some(s) = score {
            RoutingDecision::with_score(backend, reason, s)
        } else {
            RoutingDecision::new(backend, reason)
        });

        intent.trace("Selection complete".to_string());
        Ok(())
    }

    fn error_policy(&self) -> ReconcileErrorPolicy {
        ReconcileErrorPolicy::FailClosed // Must select
    }

    fn name(&self) -> &str {
        "SelectionReconciler"
    }
}
