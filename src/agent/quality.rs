//! Quality metrics store for tracking agent reliability.
//!
//! Provides a shared store for recording request outcomes and computing
//! rolling-window quality metrics per agent.

use crate::agent::AgentQualityMetrics;
use crate::config::QualityConfig;
use dashmap::DashMap;
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// A single request outcome for the rolling window.
#[derive(Debug, Clone)]
pub struct RequestOutcome {
    /// When the request completed
    pub timestamp: Instant,
    /// Whether the request succeeded
    pub success: bool,
    /// Time to first token (or full response) in milliseconds
    pub ttft_ms: u32,
}

/// Thread-safe store for quality metrics, shared across handlers and loops.
///
/// Indexed by agent_id. Stores both raw outcomes (rolling window) and
/// computed aggregate metrics.
#[derive(Debug)]
pub struct QualityMetricsStore {
    /// Per-agent rolling window of request outcomes (capped at 24h)
    outcomes: DashMap<String, RwLock<VecDeque<RequestOutcome>>>,
    /// Per-agent computed quality metrics
    metrics: DashMap<String, AgentQualityMetrics>,
    /// Quality configuration thresholds
    config: QualityConfig,
}

/// Maximum outcomes stored per agent to prevent unbounded memory growth.
/// At ~24 bytes per outcome, 100k entries ≈ 2.4 MB per agent.
const MAX_OUTCOMES_PER_AGENT: usize = 100_000;

impl QualityMetricsStore {
    /// Create a new empty store with the given configuration.
    pub fn new(config: QualityConfig) -> Self {
        Self {
            outcomes: DashMap::new(),
            metrics: DashMap::new(),
            config,
        }
    }

    /// Record a request outcome for an agent.
    pub fn record_outcome(&self, agent_id: &str, success: bool, ttft_ms: u32) {
        let outcome = RequestOutcome {
            timestamp: Instant::now(),
            success,
            ttft_ms,
        };

        self.outcomes
            .entry(agent_id.to_string())
            .or_insert_with(|| RwLock::new(VecDeque::new()));

        if let Some(entry) = self.outcomes.get(agent_id) {
            match entry.value().write() {
                Ok(mut queue) => {
                    queue.push_back(outcome);
                    while queue.len() > MAX_OUTCOMES_PER_AGENT {
                        queue.pop_front();
                    }
                }
                Err(poisoned) => {
                    tracing::warn!(agent_id, "RwLock poisoned in record_outcome, recovering");
                    let mut queue = poisoned.into_inner();
                    queue.push_back(outcome);
                    while queue.len() > MAX_OUTCOMES_PER_AGENT {
                        queue.pop_front();
                    }
                }
            }
        }
    }

    /// Get the computed quality metrics for an agent.
    /// Returns default (healthy) metrics if no data exists.
    pub fn get_metrics(&self, agent_id: &str) -> AgentQualityMetrics {
        self.metrics
            .get(agent_id)
            .map(|m| m.clone())
            .unwrap_or_default()
    }

    /// Get a snapshot of all agent metrics.
    pub fn get_all_metrics(&self) -> Vec<(String, AgentQualityMetrics)> {
        self.metrics
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// Recompute metrics for all agents from their outcome windows.
    ///
    /// Called periodically by the quality reconciliation loop.
    /// Prunes outcomes older than 24 hours.
    pub fn recompute_all(&self) {
        let now = Instant::now();
        let one_hour = Duration::from_secs(3600);
        let twenty_four_hours = Duration::from_secs(86400);

        for entry in self.outcomes.iter() {
            let agent_id = entry.key().clone();
            let mut outcomes = match entry.value().write() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    tracing::warn!(agent_id, "RwLock poisoned in recompute_all, recovering");
                    poisoned.into_inner()
                }
            };

            // Prune outcomes older than 24 hours
            while let Some(front) = outcomes.front() {
                if now.duration_since(front.timestamp) > twenty_four_hours {
                    outcomes.pop_front();
                } else {
                    break;
                }
            }

            // Compute 1h metrics
            let mut count_1h: u32 = 0;
            let mut errors_1h: u32 = 0;
            let mut ttft_sum_1h: u64 = 0;

            // Compute 24h metrics
            let mut count_24h: u32 = 0;
            let mut successes_24h: u32 = 0;
            let mut last_failure: Option<Instant> = None;

            for outcome in outcomes.iter() {
                let age = now.duration_since(outcome.timestamp);

                // 24h window
                count_24h += 1;
                if outcome.success {
                    successes_24h += 1;
                } else {
                    last_failure = Some(outcome.timestamp);
                }

                // 1h window
                if age <= one_hour {
                    count_1h += 1;
                    if !outcome.success {
                        errors_1h += 1;
                    }
                    ttft_sum_1h += outcome.ttft_ms as u64;
                }
            }

            let error_rate_1h = if count_1h > 0 {
                errors_1h as f32 / count_1h as f32
            } else {
                0.0
            };

            let avg_ttft_ms = if count_1h > 0 {
                (ttft_sum_1h / count_1h as u64) as u32
            } else {
                0
            };

            let success_rate_24h = if count_24h > 0 {
                successes_24h as f32 / count_24h as f32
            } else {
                1.0
            };

            let metrics = AgentQualityMetrics {
                error_rate_1h,
                avg_ttft_ms,
                success_rate_24h,
                last_failure_ts: last_failure,
                request_count_1h: count_1h,
            };

            self.metrics.insert(agent_id, metrics);
        }
    }

    /// Get the quality configuration.
    pub fn config(&self) -> &QualityConfig {
        &self.config
    }
}

/// Run the quality reconciliation loop as a background task.
///
/// Periodically recomputes quality metrics and updates Prometheus gauges.
pub async fn quality_reconciliation_loop(
    store: Arc<QualityMetricsStore>,
    cancel_token: tokio_util::sync::CancellationToken,
) {
    let interval_secs = store.config().metrics_interval_seconds;
    let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));

    tracing::info!(interval_secs, "Quality reconciliation loop started");

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                tracing::info!(
                    "Quality reconciliation loop stopping"
                );
                break;
            }
            _ = interval.tick() => {
                store.recompute_all();

                // Update Prometheus gauges
                for (agent_id, m) in store.get_all_metrics() {
                    metrics::gauge!(
                        "nexus_agent_error_rate",
                        "agent_id" => agent_id.clone(),
                    )
                    .set(m.error_rate_1h as f64);

                    metrics::gauge!(
                        "nexus_agent_success_rate_24h",
                        "agent_id" => agent_id.clone(),
                    )
                    .set(m.success_rate_24h as f64);

                    metrics::histogram!(
                        "nexus_agent_ttft_seconds",
                        "agent_id" => agent_id.clone(),
                    )
                    .record(m.avg_ttft_ms as f64 / 1000.0);
                }

                tracing::trace!("Quality metrics recomputed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> QualityConfig {
        QualityConfig::default()
    }

    #[test]
    fn store_returns_default_for_unknown_agent() {
        let store = QualityMetricsStore::new(default_config());
        let m = store.get_metrics("unknown");
        assert_eq!(m.error_rate_1h, 0.0);
        assert_eq!(m.success_rate_24h, 1.0);
    }

    #[test]
    fn record_outcome_stores_data() {
        let store = QualityMetricsStore::new(default_config());
        store.record_outcome("agent-1", true, 100);
        store.record_outcome("agent-1", false, 200);
        store.recompute_all();

        let m = store.get_metrics("agent-1");
        assert_eq!(m.request_count_1h, 2);
        assert_eq!(m.error_rate_1h, 0.5);
        assert_eq!(m.avg_ttft_ms, 150);
    }

    #[test]
    fn recompute_handles_empty_store() {
        let store = QualityMetricsStore::new(default_config());
        store.recompute_all(); // Should not panic
    }

    #[test]
    fn success_rate_24h_computed() {
        let store = QualityMetricsStore::new(default_config());
        store.record_outcome("a", true, 50);
        store.record_outcome("a", true, 50);
        store.record_outcome("a", false, 50);
        store.recompute_all();

        let m = store.get_metrics("a");
        assert!((m.success_rate_24h - 0.6667).abs() < 0.01);
    }

    #[test]
    fn all_successes_give_zero_error_rate() {
        let store = QualityMetricsStore::new(default_config());
        for _ in 0..10 {
            store.record_outcome("a", true, 100);
        }
        store.recompute_all();

        let m = store.get_metrics("a");
        assert_eq!(m.error_rate_1h, 0.0);
        assert_eq!(m.success_rate_24h, 1.0);
    }

    #[test]
    fn last_failure_ts_tracked() {
        let store = QualityMetricsStore::new(default_config());
        store.record_outcome("a", true, 100);
        store.record_outcome("a", false, 200);
        store.recompute_all();

        let m = store.get_metrics("a");
        assert!(m.last_failure_ts.is_some());
    }

    #[test]
    fn get_all_metrics_returns_all() {
        let store = QualityMetricsStore::new(default_config());
        store.record_outcome("a", true, 100);
        store.record_outcome("b", false, 200);
        store.recompute_all();

        let all = store.get_all_metrics();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_config_returns_configuration() {
        let config = QualityConfig {
            error_rate_threshold: 0.3,
            ttft_penalty_threshold_ms: 5000,
            metrics_interval_seconds: 60,
        };
        let store = QualityMetricsStore::new(config);
        let returned = store.config();
        assert_eq!(returned.error_rate_threshold, 0.3);
        assert_eq!(returned.ttft_penalty_threshold_ms, 5000);
        assert_eq!(returned.metrics_interval_seconds, 60);
    }

    #[tokio::test]
    async fn test_quality_reconciliation_loop_recomputes() {
        use tokio_util::sync::CancellationToken;

        let config = QualityConfig {
            error_rate_threshold: 0.5,
            ttft_penalty_threshold_ms: 3000,
            metrics_interval_seconds: 1,
        };
        let store = Arc::new(QualityMetricsStore::new(config));
        store.record_outcome("agent-1", true, 100);
        store.record_outcome("agent-1", false, 200);

        let cancel_token = CancellationToken::new();
        let store_clone = store.clone();
        let token_clone = cancel_token.clone();
        let handle = tokio::spawn(async move {
            quality_reconciliation_loop(store_clone, token_clone).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel_token.cancel();
        handle.await.unwrap();

        let metrics = store.get_metrics("agent-1");
        assert!(metrics.error_rate_1h > 0.0);
    }

    #[test]
    fn test_quality_store_record_outcome_multiple() {
        let store = QualityMetricsStore::new(default_config());

        // Record a mix of outcomes across two agents
        store.record_outcome("agent-a", true, 50);
        store.record_outcome("agent-a", true, 100);
        store.record_outcome("agent-a", false, 300);
        store.record_outcome("agent-a", true, 80);

        store.record_outcome("agent-b", false, 500);
        store.record_outcome("agent-b", false, 600);

        store.recompute_all();

        let ma = store.get_metrics("agent-a");
        assert_eq!(ma.request_count_1h, 4);
        assert!((ma.error_rate_1h - 0.25).abs() < 0.01); // 1 error / 4 requests
        assert_eq!(ma.avg_ttft_ms, (50 + 100 + 300 + 80) / 4);
        assert!(ma.last_failure_ts.is_some());
        assert!((ma.success_rate_24h - 0.75).abs() < 0.01);

        let mb = store.get_metrics("agent-b");
        assert_eq!(mb.request_count_1h, 2);
        assert_eq!(mb.error_rate_1h, 1.0); // All failures
        assert_eq!(mb.success_rate_24h, 0.0);
    }

    #[test]
    fn test_quality_store_get_profile_unknown_agent() {
        let store = QualityMetricsStore::new(default_config());

        // No outcomes recorded for this agent
        let m = store.get_metrics("totally-unknown-agent");
        assert_eq!(m.error_rate_1h, 0.0);
        assert_eq!(m.avg_ttft_ms, 0);
        assert_eq!(m.success_rate_24h, 1.0);
        assert!(m.last_failure_ts.is_none());
        assert_eq!(m.request_count_1h, 0);

        // Also verify get_all_metrics doesn't include unknown agents
        let all = store.get_all_metrics();
        assert!(all.is_empty());
    }

    fn test_requirements() -> crate::routing::requirements::RequestRequirements {
        crate::routing::requirements::RequestRequirements {
            model: "test-model".to_string(),
            estimated_tokens: 0,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        }
    }

    #[test]
    fn test_quality_reconciliation_with_errors() {
        use crate::config::QualityConfig;
        use crate::routing::reconciler::intent::RoutingIntent;
        use crate::routing::reconciler::quality::QualityReconciler;
        use crate::routing::reconciler::Reconciler;

        let config = QualityConfig {
            error_rate_threshold: 0.5,
            ttft_penalty_threshold_ms: 3000,
            metrics_interval_seconds: 30,
        };
        let store = Arc::new(QualityMetricsStore::new(config.clone()));

        // agent-good: 100% success
        for _ in 0..10 {
            store.record_outcome("agent-good", true, 100);
        }
        // agent-bad: 80% error rate (above 50% threshold)
        for _ in 0..8 {
            store.record_outcome("agent-bad", false, 500);
        }
        for _ in 0..2 {
            store.record_outcome("agent-bad", true, 100);
        }

        store.recompute_all();

        let reconciler = QualityReconciler::new(Arc::clone(&store), config);
        let mut intent = RoutingIntent::new(
            "req-1".to_string(),
            "gpt-4".to_string(),
            "gpt-4".to_string(),
            test_requirements(),
            vec!["agent-good".to_string(), "agent-bad".to_string()],
        );

        reconciler.reconcile(&mut intent).unwrap();

        // agent-bad should be excluded (80% > 50% threshold)
        assert!(intent.candidate_agents.contains(&"agent-good".to_string()));
        assert!(!intent.candidate_agents.contains(&"agent-bad".to_string()));
        assert!(intent.excluded_agents.contains(&"agent-bad".to_string()));
        assert_eq!(intent.rejection_reasons.len(), 1);
        assert!(intent.rejection_reasons[0].reason.contains("Error rate"));
    }

    #[test]
    fn test_quality_reconciliation_deprioritizes_high_ttft() {
        use crate::config::QualityConfig;
        use crate::routing::reconciler::intent::RoutingIntent;
        use crate::routing::reconciler::quality::QualityReconciler;
        use crate::routing::reconciler::Reconciler;

        let config = QualityConfig {
            error_rate_threshold: 0.5,
            ttft_penalty_threshold_ms: 200,
            metrics_interval_seconds: 30,
        };
        let store = Arc::new(QualityMetricsStore::new(config.clone()));

        // agent-fast: low TTFT, all success
        for _ in 0..10 {
            store.record_outcome("agent-fast", true, 50);
        }
        // agent-slow: high TTFT but no errors
        for _ in 0..10 {
            store.record_outcome("agent-slow", true, 5000);
        }

        store.recompute_all();

        let m_fast = store.get_metrics("agent-fast");
        let m_slow = store.get_metrics("agent-slow");

        // Both should have 0 error rate
        assert_eq!(m_fast.error_rate_1h, 0.0);
        assert_eq!(m_slow.error_rate_1h, 0.0);

        // But TTFT should differ significantly
        assert_eq!(m_fast.avg_ttft_ms, 50);
        assert_eq!(m_slow.avg_ttft_ms, 5000);

        // agent-slow's TTFT exceeds the penalty threshold
        assert!(m_slow.avg_ttft_ms > config.ttft_penalty_threshold_ms);
        assert!(m_fast.avg_ttft_ms <= config.ttft_penalty_threshold_ms);

        // Both should still be candidates (quality reconciler only excludes by error rate)
        let reconciler = QualityReconciler::new(Arc::clone(&store), config);
        let mut intent = RoutingIntent::new(
            "req-1".to_string(),
            "model".to_string(),
            "model".to_string(),
            test_requirements(),
            vec!["agent-fast".to_string(), "agent-slow".to_string()],
        );
        reconciler.reconcile(&mut intent).unwrap();

        // Neither excluded by error rate, but metrics track TTFT for scoring
        assert_eq!(intent.candidate_agents.len(), 2);
        assert!(intent.excluded_agents.is_empty());
    }

    #[tokio::test]
    async fn test_quality_store_concurrent_access() {
        let store = Arc::new(QualityMetricsStore::new(default_config()));

        // Spawn multiple tasks that concurrently record outcomes
        let mut handles = Vec::new();
        for i in 0..10 {
            let store_clone = Arc::clone(&store);
            let agent_id = format!("agent-{}", i % 3); // 3 agents, contended
            handles.push(tokio::spawn(async move {
                for j in 0..50 {
                    store_clone.record_outcome(&agent_id, j % 3 != 0, (j * 10) as u32);
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // Recompute should not panic with concurrent data
        store.recompute_all();

        // All 3 agents should have metrics
        let all = store.get_all_metrics();
        assert_eq!(all.len(), 3);

        // Each agent should have recorded outcomes
        for i in 0..3 {
            let m = store.get_metrics(&format!("agent-{}", i));
            assert!(m.request_count_1h > 0);
        }
    }

    #[test]
    fn test_quality_store_max_outcomes_cap() {
        let store = QualityMetricsStore::new(default_config());

        // Record more than MAX_OUTCOMES_PER_AGENT
        for i in 0..(MAX_OUTCOMES_PER_AGENT + 100) {
            store.record_outcome("agent-cap", i % 2 == 0, 100);
        }

        // The queue should be capped at MAX_OUTCOMES_PER_AGENT
        let entry = store.outcomes.get("agent-cap").unwrap();
        let queue = entry.value().read().unwrap();
        assert_eq!(queue.len(), MAX_OUTCOMES_PER_AGENT);
    }

    #[tokio::test]
    async fn test_quality_reconciliation_loop_stops_on_cancel() {
        use tokio_util::sync::CancellationToken;

        let config = QualityConfig {
            error_rate_threshold: 0.5,
            ttft_penalty_threshold_ms: 3000,
            metrics_interval_seconds: 1,
        };
        let store = Arc::new(QualityMetricsStore::new(config));

        // Record some data before starting the loop
        store.record_outcome("agent-x", true, 100);
        store.record_outcome("agent-x", false, 500);

        let cancel_token = CancellationToken::new();
        let store_clone = Arc::clone(&store);
        let token_clone = cancel_token.clone();

        let handle = tokio::spawn(async move {
            quality_reconciliation_loop(store_clone, token_clone).await;
        });

        // Let the loop tick at least once
        tokio::time::sleep(Duration::from_millis(1200)).await;

        // Metrics should have been computed by the loop
        let m = store.get_metrics("agent-x");
        assert_eq!(m.request_count_1h, 2);

        // Cancel and verify clean shutdown
        cancel_token.cancel();
        let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
        assert!(result.is_ok(), "Loop should stop within timeout");
        assert!(result.unwrap().is_ok(), "Loop should not panic");
    }

    #[test]
    fn test_quality_reconciler_skips_agents_without_history() {
        use crate::config::QualityConfig;
        use crate::routing::reconciler::intent::RoutingIntent;
        use crate::routing::reconciler::quality::QualityReconciler;
        use crate::routing::reconciler::Reconciler;

        let config = QualityConfig {
            error_rate_threshold: 0.5,
            ttft_penalty_threshold_ms: 3000,
            metrics_interval_seconds: 30,
        };
        let store = Arc::new(QualityMetricsStore::new(config.clone()));

        // agent-new has no recorded outcomes
        let reconciler = QualityReconciler::new(Arc::clone(&store), config);
        let mut intent = RoutingIntent::new(
            "req-1".to_string(),
            "model".to_string(),
            "model".to_string(),
            test_requirements(),
            vec!["agent-new".to_string()],
        );

        reconciler.reconcile(&mut intent).unwrap();

        // Agent with no history should NOT be excluded
        assert!(intent.candidate_agents.contains(&"agent-new".to_string()));
        assert!(intent.excluded_agents.is_empty());
    }

    #[test]
    fn test_quality_reconciler_excludes_high_error_rate_agent() {
        use crate::config::QualityConfig;
        use crate::routing::reconciler::intent::RoutingIntent;
        use crate::routing::reconciler::quality::QualityReconciler;
        use crate::routing::reconciler::Reconciler;

        let config = QualityConfig {
            error_rate_threshold: 0.3,
            ttft_penalty_threshold_ms: 3000,
            metrics_interval_seconds: 30,
        };
        let store = Arc::new(QualityMetricsStore::new(config.clone()));

        // Record many errors for agent-bad
        for _ in 0..10 {
            store.record_outcome("agent-bad", false, 100);
        }
        // Record successes for agent-good
        for _ in 0..10 {
            store.record_outcome("agent-good", true, 100);
        }

        // Recompute metrics
        store.recompute_all();

        let reconciler = QualityReconciler::new(Arc::clone(&store), config);
        let mut intent = RoutingIntent::new(
            "req-1".to_string(),
            "model".to_string(),
            "model".to_string(),
            test_requirements(),
            vec!["agent-bad".to_string(), "agent-good".to_string()],
        );

        reconciler.reconcile(&mut intent).unwrap();

        // agent-bad should be excluded due to high error rate
        assert!(
            !intent.candidate_agents.contains(&"agent-bad".to_string())
                || intent.excluded_agents.contains(&"agent-bad".to_string()),
            "Expected agent-bad to be excluded or penalized"
        );
    }

    #[test]
    fn test_quality_metrics_store_multiple_agents() {
        let config = QualityConfig {
            error_rate_threshold: 0.5,
            ttft_penalty_threshold_ms: 3000,
            metrics_interval_seconds: 30,
        };
        let store = QualityMetricsStore::new(config);

        store.record_outcome("a", true, 50);
        store.record_outcome("a", true, 100);
        store.record_outcome("b", false, 500);
        store.record_outcome("b", false, 400);
        store.record_outcome("c", true, 200);

        store.recompute_all();

        let ma = store.get_metrics("a");
        assert_eq!(ma.request_count_1h, 2);
        assert_eq!(ma.error_rate_1h, 0.0);

        let mb = store.get_metrics("b");
        assert_eq!(mb.request_count_1h, 2);
        assert_eq!(mb.error_rate_1h, 1.0);

        let mc = store.get_metrics("c");
        assert_eq!(mc.request_count_1h, 1);
    }

    #[tokio::test]
    async fn test_quality_reconciliation_loop_runs_metrics_tick() {
        use tokio_util::sync::CancellationToken;

        let config = QualityConfig {
            error_rate_threshold: 0.5,
            ttft_penalty_threshold_ms: 3000,
            metrics_interval_seconds: 1,
        };
        let store = Arc::new(QualityMetricsStore::new(config));

        // Record data for two agents with different patterns
        for _ in 0..5 {
            store.record_outcome("agent-a", true, 100);
        }
        for _ in 0..5 {
            store.record_outcome("agent-b", false, 500);
        }

        let cancel_token = CancellationToken::new();
        let store_clone = Arc::clone(&store);
        let token_clone = cancel_token.clone();

        let handle = tokio::spawn(async move {
            quality_reconciliation_loop(store_clone, token_clone).await;
        });

        // Let loop tick at least once
        tokio::time::sleep(Duration::from_millis(1500)).await;

        // Verify metrics were computed
        let ma = store.get_metrics("agent-a");
        assert_eq!(ma.request_count_1h, 5);
        assert_eq!(ma.error_rate_1h, 0.0);

        let mb = store.get_metrics("agent-b");
        assert_eq!(mb.request_count_1h, 5);
        assert_eq!(mb.error_rate_1h, 1.0);

        cancel_token.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    }
}
