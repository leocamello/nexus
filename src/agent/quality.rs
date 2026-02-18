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
}
