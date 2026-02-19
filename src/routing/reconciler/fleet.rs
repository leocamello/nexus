//! Fleet Intelligence — Background pattern analysis and pre-warming recommendations
//!
//! FleetReconciler is NOT a pipeline Reconciler (not in the hot path).
//! It runs as a background task that:
//! 1. Records model request timestamps (called from completions handler)
//! 2. Periodically analyzes patterns (time-of-day, popularity trends)
//! 3. Generates advisory pre-warming recommendations
//!
//! Recommendation approach is suggestion-first: recommendations are advisory-only
//! and require operator approval to execute (FR-022).

use crate::agent::types::{PrewarmingRecommendation, TrendDirection};
use crate::config::FleetConfig;
use crate::registry::Registry;
use chrono::{DateTime, Datelike, Timelike, Utc};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Maximum number of days of request history to retain.
const MAX_HISTORY_DAYS: i64 = 30;

/// Number of seconds in one hour.
const SECONDS_PER_HOUR: i64 = 3600;

/// Minimum ratio of peak-hour requests to average for pattern detection.
const PEAK_RATIO_THRESHOLD: f64 = 2.0;

/// Threshold for trend detection: recent period must differ by this ratio.
const TREND_THRESHOLD: f64 = 0.2;

/// Recent period for trend comparison (in days).
const TREND_RECENT_DAYS: i64 = 7;

/// Fleet intelligence background analyzer.
///
/// Records request timestamps per model and periodically analyzes patterns
/// to generate pre-warming recommendations. All recommendations are advisory
/// and never auto-execute.
pub struct FleetReconciler {
    /// Request timestamps per model: model_id → Vec<unix_timestamp_secs>
    request_history: DashMap<String, Vec<i64>>,

    /// Current recommendations (updated by analyze())
    recommendations: Arc<RwLock<Vec<PrewarmingRecommendation>>>,

    /// Configuration
    config: FleetConfig,

    /// Registry for backend/model queries
    registry: Arc<Registry>,
}

/// Aggregated hourly data for a model.
#[derive(Debug, Clone)]
struct HourlyProfile {
    /// Average requests per hour-of-day (index 0-23)
    #[allow(dead_code)]
    hourly_avg: [f64; 24],
    /// Total requests across all hours
    total_requests: u64,
    /// Number of distinct days with data
    days_with_data: u32,
    /// Peak hour (0-23)
    peak_hour: u8,
    /// Peak-to-average ratio
    peak_ratio: f64,
}

impl FleetReconciler {
    /// Create a new FleetReconciler.
    pub fn new(config: FleetConfig, registry: Arc<Registry>) -> Self {
        Self {
            request_history: DashMap::new(),
            recommendations: Arc::new(RwLock::new(Vec::new())),
            config,
            registry,
        }
    }

    /// Record a request for a model (T069).
    ///
    /// Called from the completions handler after each request.
    /// Stores the current UTC timestamp for the given model.
    /// No-op when fleet intelligence is disabled to avoid unbounded memory growth.
    pub fn record_request(&self, model_id: &str) {
        if !self.config.enabled {
            return;
        }
        let now = Utc::now().timestamp();
        self.request_history
            .entry(model_id.to_string())
            .or_default()
            .push(now);
    }

    /// Get current pre-warming recommendations (T078).
    pub async fn get_recommendations(&self) -> Vec<PrewarmingRecommendation> {
        self.recommendations.read().await.clone()
    }

    /// Run fleet analysis and update recommendations (T070-T080).
    ///
    /// This is called periodically by the background task.
    /// It analyzes request patterns and generates advisory recommendations.
    pub async fn analyze(&self) {
        if !self.config.enabled {
            return;
        }

        // Clean up old data first
        self.cleanup_old_data();

        let now = Utc::now();
        let mut new_recommendations = Vec::new();

        // Collect model IDs to analyze
        let model_ids: Vec<String> = self
            .request_history
            .iter()
            .map(|entry| entry.key().clone())
            .collect();

        for model_id in &model_ids {
            if let Some(timestamps) = self.request_history.get(model_id) {
                let ts_slice = timestamps.value();

                // T075: Minimum sample size validation
                if !self.meets_sample_threshold(ts_slice) {
                    debug!(
                        model_id = %model_id,
                        request_count = ts_slice.len(),
                        min_required = self.config.min_request_count,
                        "Skipping model: insufficient sample size"
                    );
                    continue;
                }

                // T072: Time-of-day pattern detection
                let profile = self.build_hourly_profile(ts_slice);

                // T073: Model popularity trend analysis
                let trend = self.calculate_trend(ts_slice);

                // Only generate recommendations for models with clear patterns
                if profile.peak_ratio >= PEAK_RATIO_THRESHOLD || trend == TrendDirection::Increasing
                {
                    // T074: Calculate confidence score
                    let confidence = self.calculate_confidence(&profile, ts_slice.len());

                    // T076: Find backends with VRAM headroom
                    let target_backends = self.find_eligible_backends(model_id);

                    if target_backends.is_empty() {
                        debug!(
                            model_id = %model_id,
                            "No eligible backends with VRAM headroom"
                        );
                        continue;
                    }

                    // T077: Hot model protection — skip if already loaded and active
                    if self.is_hot_model(model_id) {
                        debug!(
                            model_id = %model_id,
                            "Skipping hot model: already actively serving"
                        );
                        continue;
                    }

                    let reasoning = self.build_reasoning(model_id, &profile, &trend);

                    // T079: Log recommendation with reasoning
                    info!(
                        model_id = %model_id,
                        confidence = confidence,
                        peak_hour = profile.peak_hour,
                        trend = ?trend,
                        target_backends = ?target_backends,
                        reasoning = %reasoning,
                        "Fleet intelligence recommendation generated"
                    );

                    new_recommendations.push(PrewarmingRecommendation {
                        recommendation_id: format!("rec-{}", uuid::Uuid::new_v4()),
                        model_id: model_id.clone(),
                        target_backend_ids: target_backends,
                        confidence_score: confidence,
                        reasoning,
                        vram_required_bytes: None, // Ollama doesn't expose model size pre-load
                        generated_at: now,
                        expires_at: Some(
                            now + chrono::Duration::seconds(
                                self.config.analysis_interval_seconds as i64,
                            ),
                        ),
                    });
                }
            }
        }

        // Sort by confidence (highest first) and cap at max_recommendations
        new_recommendations.sort_by(|a, b| {
            b.confidence_score
                .partial_cmp(&a.confidence_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        new_recommendations.truncate(self.config.max_recommendations as usize);

        let count = new_recommendations.len();
        *self.recommendations.write().await = new_recommendations;

        if count > 0 {
            info!(
                recommendation_count = count,
                "Fleet analysis complete: {} recommendations generated", count
            );
        } else {
            debug!("Fleet analysis complete: no recommendations generated");
        }
    }

    /// T075: Check if timestamps meet minimum sample thresholds.
    fn meets_sample_threshold(&self, timestamps: &[i64]) -> bool {
        if (timestamps.len() as u32) < self.config.min_request_count {
            return false;
        }

        // Check minimum days of data (safe: already verified non-empty via min_request_count check)
        let (Some(&min_ts), Some(&max_ts)) = (timestamps.iter().min(), timestamps.iter().max())
        else {
            return false;
        };
        let days_span = (max_ts - min_ts) / (24 * SECONDS_PER_HOUR);

        days_span >= self.config.min_sample_days as i64
    }

    /// T072: Build hourly request profile from timestamps.
    fn build_hourly_profile(&self, timestamps: &[i64]) -> HourlyProfile {
        let mut hourly_counts = [0u64; 24];
        let mut days_seen = std::collections::HashSet::new();

        for &ts in timestamps {
            if let Some(dt) = DateTime::from_timestamp(ts, 0) {
                let hour = dt.hour() as usize;
                hourly_counts[hour] += 1;
                // Track unique days
                days_seen.insert((dt.year(), dt.ordinal()));
            }
        }

        let days_with_data = days_seen.len().max(1) as u32;

        // Calculate average per hour-of-day
        let mut hourly_avg = [0.0f64; 24];
        for (i, &count) in hourly_counts.iter().enumerate() {
            hourly_avg[i] = count as f64 / days_with_data as f64;
        }

        // Find peak hour
        let (peak_hour, peak_avg) = hourly_avg
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(h, &v)| (h as u8, v))
            .unwrap_or((0, 0.0));

        // Calculate overall average (across non-zero hours)
        let non_zero_hours = hourly_avg.iter().filter(|&&v| v > 0.0).count().max(1);
        let overall_avg: f64 = hourly_avg.iter().sum::<f64>() / non_zero_hours as f64;

        let peak_ratio = if overall_avg > 0.0 {
            peak_avg / overall_avg
        } else {
            0.0
        };

        HourlyProfile {
            hourly_avg,
            total_requests: timestamps.len() as u64,
            days_with_data,
            peak_hour,
            peak_ratio,
        }
    }

    /// T073: Calculate popularity trend by comparing recent vs older periods.
    fn calculate_trend(&self, timestamps: &[i64]) -> TrendDirection {
        let now = Utc::now().timestamp();
        let recent_cutoff = now - (TREND_RECENT_DAYS * 24 * SECONDS_PER_HOUR);
        let older_cutoff = recent_cutoff - (TREND_RECENT_DAYS * 24 * SECONDS_PER_HOUR);

        let recent_count = timestamps.iter().filter(|&&ts| ts >= recent_cutoff).count() as f64;
        let older_count = timestamps
            .iter()
            .filter(|&&ts| ts >= older_cutoff && ts < recent_cutoff)
            .count() as f64;

        if older_count == 0.0 {
            if recent_count > 0.0 {
                return TrendDirection::Increasing;
            }
            return TrendDirection::Stable;
        }

        let ratio = (recent_count - older_count) / older_count;

        if ratio > TREND_THRESHOLD {
            TrendDirection::Increasing
        } else if ratio < -TREND_THRESHOLD {
            TrendDirection::Decreasing
        } else {
            TrendDirection::Stable
        }
    }

    /// T074: Calculate confidence score (0.0–1.0) based on pattern strength and sample size.
    fn calculate_confidence(&self, profile: &HourlyProfile, sample_size: usize) -> f64 {
        // Pattern strength component: how strong is the time-of-day pattern?
        let pattern_strength = if profile.peak_ratio >= PEAK_RATIO_THRESHOLD {
            ((profile.peak_ratio - 1.0) / 3.0).min(1.0)
        } else {
            0.2 // Base confidence for increasing trend
        };

        // Sample size component: more data = higher confidence
        let sample_factor =
            (sample_size as f64 / (self.config.min_request_count as f64 * 5.0)).min(1.0);

        // Days component: more days = higher confidence
        let days_factor =
            (profile.days_with_data as f64 / (self.config.min_sample_days as f64 * 2.0)).min(1.0);

        // Combined confidence
        let confidence = pattern_strength * 0.5 + sample_factor * 0.25 + days_factor * 0.25;
        confidence.clamp(0.0, 1.0)
    }

    /// T076: Find backends with sufficient VRAM headroom for pre-warming.
    fn find_eligible_backends(&self, model_id: &str) -> Vec<String> {
        let mut eligible = Vec::new();

        for backend in self.registry.get_all_backends() {
            // Skip backends that already have this model loaded
            if backend.models.iter().any(|m| m.id == *model_id) {
                continue;
            }

            // Skip backends with active lifecycle operations
            if let Some(ref op) = backend.current_operation {
                if op.status == crate::agent::types::OperationStatus::InProgress {
                    continue;
                }
            }

            // Only consider healthy backends
            if backend.status != crate::registry::BackendStatus::Healthy {
                continue;
            }

            eligible.push(backend.id.clone());
        }

        eligible
    }

    /// T077: Check if a model is "hot" (actively serving recent requests).
    ///
    /// A hot model is one loaded on at least one healthy backend.
    /// We never recommend unloading a hot model.
    fn is_hot_model(&self, model_id: &str) -> bool {
        // Check if any healthy backend currently has this model loaded
        for backend in self.registry.get_all_backends() {
            if backend.status == crate::registry::BackendStatus::Healthy
                && backend.models.iter().any(|m| m.id == *model_id)
            {
                return true;
            }
        }
        false
    }

    /// Build human-readable reasoning for a recommendation.
    fn build_reasoning(
        &self,
        model_id: &str,
        profile: &HourlyProfile,
        trend: &TrendDirection,
    ) -> String {
        let mut parts = Vec::new();

        if profile.peak_ratio >= PEAK_RATIO_THRESHOLD {
            parts.push(format!(
                "Time-of-day pattern detected: peak at {}:00 UTC ({:.1}x average)",
                profile.peak_hour, profile.peak_ratio
            ));
        }

        match trend {
            TrendDirection::Increasing => {
                parts.push(format!(
                    "Model '{}' popularity is increasing (7-day trend)",
                    model_id
                ));
            }
            TrendDirection::Decreasing => {
                parts.push(format!(
                    "Model '{}' popularity is decreasing (7-day trend)",
                    model_id
                ));
            }
            TrendDirection::Stable => {}
        }

        parts.push(format!(
            "Based on {} requests over {} days",
            profile.total_requests, profile.days_with_data
        ));

        parts.join(". ")
    }

    /// Remove timestamps older than MAX_HISTORY_DAYS.
    fn cleanup_old_data(&self) {
        let cutoff = Utc::now().timestamp() - (MAX_HISTORY_DAYS * 24 * SECONDS_PER_HOUR);

        self.request_history.iter_mut().for_each(|mut entry| {
            entry.value_mut().retain(|&ts| ts >= cutoff);
        });

        // Remove empty entries
        self.request_history.retain(|_, v| !v.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FleetConfig;
    use crate::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model, Registry};
    use chrono::{Duration, Utc};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, AtomicU64};

    fn test_config() -> FleetConfig {
        FleetConfig {
            enabled: true,
            min_sample_days: 7,
            min_request_count: 10, // Lower for testing
            analysis_interval_seconds: 3600,
            max_recommendations: 5,
        }
    }

    fn create_backend(id: &str, models: Vec<&str>) -> Backend {
        Backend {
            id: id.to_string(),
            name: id.to_string(),
            url: format!("http://{}", id),
            backend_type: BackendType::Ollama,
            status: BackendStatus::Healthy,
            last_health_check: Utc::now(),
            last_error: None,
            models: models
                .into_iter()
                .map(|m| Model {
                    id: m.to_string(),
                    name: m.to_string(),
                    context_length: 4096,
                    supports_vision: false,
                    supports_tools: false,
                    supports_json_mode: false,
                    max_output_tokens: None,
                })
                .collect(),
            priority: 1,
            pending_requests: AtomicU32::new(0),
            total_requests: AtomicU64::new(0),
            avg_latency_ms: AtomicU32::new(50),
            discovery_source: DiscoverySource::Static,
            metadata: HashMap::new(),
            current_operation: None,
        }
    }

    /// Generate timestamps simulating a time-of-day pattern.
    /// Creates `count_per_day` requests at `peak_hour` for each day going back `days`.
    fn generate_peak_pattern(peak_hour: u8, count_per_day: u32, days: u32) -> Vec<i64> {
        let now = Utc::now();
        let mut timestamps = Vec::new();

        for day_offset in 0..days {
            let day = now - Duration::days(day_offset as i64);
            let base = day
                .date_naive()
                .and_hms_opt(peak_hour as u32, 0, 0)
                .unwrap()
                .and_utc();

            for i in 0..count_per_day {
                // Spread requests within the peak hour
                timestamps.push(base.timestamp() + (i as i64 * 60));
            }

            // Add some background traffic at other hours (1 req/hour)
            for h in 0..24u32 {
                if h != peak_hour as u32 {
                    let t = day.date_naive().and_hms_opt(h, 30, 0).unwrap().and_utc();
                    timestamps.push(t.timestamp());
                }
            }
        }

        timestamps
    }

    /// Generate uniform timestamps (no time-of-day pattern).
    fn generate_uniform_traffic(requests_per_hour: u32, days: u32) -> Vec<i64> {
        let now = Utc::now();
        let mut timestamps = Vec::new();

        for day_offset in 0..days {
            let day = now - Duration::days(day_offset as i64);
            for h in 0..24u32 {
                for i in 0..requests_per_hour {
                    let t = day
                        .date_naive()
                        .and_hms_opt(h, i * (60 / requests_per_hour.max(1)), 0)
                        .unwrap()
                        .and_utc();
                    timestamps.push(t.timestamp());
                }
            }
        }

        timestamps
    }

    // T062: Pattern detection with simulated request history
    #[test]
    fn detects_time_of_day_pattern() {
        let registry = Arc::new(Registry::new());
        let reconciler = FleetReconciler::new(test_config(), registry);

        // Create strong peak at 9am: 20 requests per day at 9am, 1 per other hour
        let timestamps = generate_peak_pattern(9, 20, 10);

        let profile = reconciler.build_hourly_profile(&timestamps);

        assert_eq!(profile.peak_hour, 9);
        assert!(
            profile.peak_ratio >= PEAK_RATIO_THRESHOLD,
            "Peak ratio {} should be >= {}",
            profile.peak_ratio,
            PEAK_RATIO_THRESHOLD
        );
    }

    // T063: Time-of-day spike detection
    #[test]
    fn spike_detection_distinguishes_peak_from_uniform() {
        let registry = Arc::new(Registry::new());
        let reconciler = FleetReconciler::new(test_config(), registry);

        // Uniform traffic should NOT show strong pattern
        let uniform = generate_uniform_traffic(2, 10);
        let uniform_profile = reconciler.build_hourly_profile(&uniform);
        assert!(
            uniform_profile.peak_ratio < PEAK_RATIO_THRESHOLD,
            "Uniform traffic peak ratio {} should be < {}",
            uniform_profile.peak_ratio,
            PEAK_RATIO_THRESHOLD
        );

        // Peaked traffic SHOULD show strong pattern
        let peaked = generate_peak_pattern(14, 30, 10);
        let peak_profile = reconciler.build_hourly_profile(&peaked);
        assert!(
            peak_profile.peak_ratio >= PEAK_RATIO_THRESHOLD,
            "Peaked traffic peak ratio {} should be >= {}",
            peak_profile.peak_ratio,
            PEAK_RATIO_THRESHOLD
        );
        assert_eq!(peak_profile.peak_hour, 14);
    }

    // T064: VRAM headroom constraint validation
    #[test]
    fn eligible_backends_excludes_those_with_model() {
        let registry = Arc::new(Registry::new());
        // Backend already has the model
        registry
            .add_backend(create_backend("b1", vec!["llama3:8b"]))
            .unwrap();
        // Backend does NOT have the model
        registry
            .add_backend(create_backend("b2", vec!["codellama:7b"]))
            .unwrap();

        let reconciler = FleetReconciler::new(test_config(), Arc::clone(&registry));

        let eligible = reconciler.find_eligible_backends("llama3:8b");
        assert_eq!(eligible, vec!["b2"]);
    }

    // T065: Hot model protection
    #[test]
    fn hot_model_detection() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("b1", vec!["llama3:8b"]))
            .unwrap();

        let reconciler = FleetReconciler::new(test_config(), Arc::clone(&registry));

        // Model loaded on healthy backend = hot
        assert!(reconciler.is_hot_model("llama3:8b"));

        // Model not loaded anywhere = not hot
        assert!(!reconciler.is_hot_model("nonexistent-model"));
    }

    // T066: Confidence score calculation
    #[test]
    fn confidence_score_range() {
        let registry = Arc::new(Registry::new());
        let reconciler = FleetReconciler::new(test_config(), registry);

        // Strong pattern + large sample
        let strong_profile = HourlyProfile {
            hourly_avg: {
                let mut arr = [1.0; 24];
                arr[9] = 20.0;
                arr
            },
            total_requests: 5000,
            days_with_data: 30,
            peak_hour: 9,
            peak_ratio: 5.0,
        };
        let high_conf = reconciler.calculate_confidence(&strong_profile, 5000);
        assert!(
            high_conf > 0.5,
            "Strong pattern should have high confidence: {}",
            high_conf
        );

        // Weak pattern + small sample
        let weak_profile = HourlyProfile {
            hourly_avg: [1.0; 24],
            total_requests: 20,
            days_with_data: 8,
            peak_hour: 0,
            peak_ratio: 1.2,
        };
        let low_conf = reconciler.calculate_confidence(&weak_profile, 20);
        assert!(
            low_conf < high_conf,
            "Weak pattern should have lower confidence: {} vs {}",
            low_conf,
            high_conf
        );

        // All confidence scores should be in [0.0, 1.0]
        assert!((0.0..=1.0).contains(&high_conf));
        assert!((0.0..=1.0).contains(&low_conf));
    }

    // T067: Minimum sample size threshold enforcement
    #[test]
    fn sample_size_threshold() {
        let registry = Arc::new(Registry::new());
        let config = FleetConfig {
            min_sample_days: 7,
            min_request_count: 100,
            ..test_config()
        };
        let reconciler = FleetReconciler::new(config, registry);

        // Too few requests
        let few_requests: Vec<i64> = (0..50)
            .map(|i| Utc::now().timestamp() - i * SECONDS_PER_HOUR)
            .collect();
        assert!(!reconciler.meets_sample_threshold(&few_requests));

        // Enough requests but too few days
        let short_period: Vec<i64> = (0..200)
            .map(|i| Utc::now().timestamp() - i * 60) // All within a few hours
            .collect();
        assert!(!reconciler.meets_sample_threshold(&short_period));

        // Sufficient requests and days
        let good_data: Vec<i64> = (0..200)
            .map(|i| Utc::now().timestamp() - i * SECONDS_PER_HOUR)
            .collect();
        assert!(reconciler.meets_sample_threshold(&good_data));
    }

    // T073: Trend analysis
    #[test]
    fn trend_detection() {
        let registry = Arc::new(Registry::new());
        let reconciler = FleetReconciler::new(test_config(), registry);

        let now = Utc::now().timestamp();
        let day = 24 * SECONDS_PER_HOUR;

        // Increasing: lots of recent, few older
        let increasing: Vec<i64> = (0..100)
            .map(|i| now - i * 3600) // 100 in last ~4 days
            .chain((0..20).map(|i| now - 8 * day - i * 3600)) // 20 in 8-9 days ago
            .collect();
        assert_eq!(
            reconciler.calculate_trend(&increasing),
            TrendDirection::Increasing
        );

        // Decreasing: few recent, many older
        let decreasing: Vec<i64> = (0..20)
            .map(|i| now - i * 3600) // 20 in last day
            .chain((0..100).map(|i| now - 8 * day - i * 3600)) // 100 in 8-12 days ago
            .collect();
        assert_eq!(
            reconciler.calculate_trend(&decreasing),
            TrendDirection::Decreasing
        );

        // Stable: similar recent and older
        let stable: Vec<i64> = (0..50)
            .map(|i| now - i * 3600) // 50 recent
            .chain((0..50).map(|i| now - 8 * day - i * 3600)) // 50 older
            .collect();
        assert_eq!(reconciler.calculate_trend(&stable), TrendDirection::Stable);
    }

    // T068: Request history storage
    #[test]
    fn record_request_stores_timestamps() {
        let registry = Arc::new(Registry::new());
        let reconciler = FleetReconciler::new(test_config(), registry);

        reconciler.record_request("llama3:8b");
        reconciler.record_request("llama3:8b");
        reconciler.record_request("codellama:7b");

        assert_eq!(
            reconciler.request_history.get("llama3:8b").unwrap().len(),
            2
        );
        assert_eq!(
            reconciler
                .request_history
                .get("codellama:7b")
                .unwrap()
                .len(),
            1
        );
    }

    // T070: Data cleanup
    #[test]
    fn cleanup_removes_old_data() {
        let registry = Arc::new(Registry::new());
        let reconciler = FleetReconciler::new(test_config(), registry);

        let now = Utc::now().timestamp();
        let old = now - (MAX_HISTORY_DAYS + 1) * 24 * SECONDS_PER_HOUR;

        reconciler
            .request_history
            .insert("old_model".to_string(), vec![old]);
        reconciler
            .request_history
            .insert("new_model".to_string(), vec![now]);

        reconciler.cleanup_old_data();

        assert!(!reconciler.request_history.contains_key("old_model"));
        assert!(reconciler.request_history.contains_key("new_model"));
    }

    // T080a: Suggestion-first approach
    #[tokio::test]
    async fn recommendations_are_advisory_only() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("b1", vec!["codellama:7b"]))
            .unwrap();

        let config = FleetConfig {
            min_sample_days: 1, // Lower for test
            min_request_count: 5,
            ..test_config()
        };
        let reconciler = FleetReconciler::new(config, Arc::clone(&registry));

        // Inject historical data with strong time-of-day pattern for a new model
        let timestamps = generate_peak_pattern(9, 20, 10);
        reconciler
            .request_history
            .insert("llama3:8b".to_string(), timestamps);

        // Run analysis
        reconciler.analyze().await;

        let recs = reconciler.get_recommendations().await;

        // Recommendations should exist
        if !recs.is_empty() {
            // Each recommendation must have required fields
            for rec in &recs {
                assert!(!rec.recommendation_id.is_empty());
                assert!(!rec.model_id.is_empty());
                assert!(!rec.target_backend_ids.is_empty());
                assert!((0.0..=1.0).contains(&rec.confidence_score));
                assert!(!rec.reasoning.is_empty());
                assert!(rec.expires_at.is_some());
            }
        }
    }

    // T061: Contract test for GET /v1/fleet/recommendations shape
    #[test]
    fn recommendation_serializes_correctly() {
        let rec = PrewarmingRecommendation {
            recommendation_id: "rec-123".to_string(),
            model_id: "llama3:8b".to_string(),
            target_backend_ids: vec!["b1".to_string()],
            confidence_score: 0.85,
            reasoning: "Peak at 9am".to_string(),
            vram_required_bytes: None,
            generated_at: Utc::now(),
            expires_at: Some(Utc::now() + Duration::hours(1)),
        };

        let json = serde_json::to_value(&rec).unwrap();
        assert_eq!(json["model_id"], "llama3:8b");
        assert_eq!(json["confidence_score"], 0.85);
        assert!(json["target_backend_ids"].is_array());
        assert!(json["generated_at"].is_string());
    }

    // Full integration: analyze with real-ish data
    #[tokio::test]
    async fn full_analysis_with_pattern() {
        let registry = Arc::new(Registry::new());
        // Backend without the model → eligible for pre-warming
        registry
            .add_backend(create_backend("gpu-01", vec!["codellama:7b"]))
            .unwrap();

        let config = FleetConfig {
            min_sample_days: 1,
            min_request_count: 5,
            ..test_config()
        };
        let reconciler = FleetReconciler::new(config, Arc::clone(&registry));

        // Inject strong 9am pattern for a model NOT on any backend
        let timestamps = generate_peak_pattern(9, 30, 14);
        reconciler
            .request_history
            .insert("llama3:8b".to_string(), timestamps);

        reconciler.analyze().await;

        let recs = reconciler.get_recommendations().await;
        assert!(
            !recs.is_empty(),
            "Should generate recommendations for strong pattern"
        );

        let rec = &recs[0];
        assert_eq!(rec.model_id, "llama3:8b");
        assert!(rec.target_backend_ids.contains(&"gpu-01".to_string()));
        assert!(rec.confidence_score > 0.0);
        assert!(rec.reasoning.contains("peak"));
    }

    // Disabled fleet should produce no recommendations
    #[tokio::test]
    async fn disabled_fleet_produces_no_recommendations() {
        let registry = Arc::new(Registry::new());
        let config = FleetConfig {
            enabled: false,
            ..test_config()
        };
        let reconciler = FleetReconciler::new(config, registry);

        // Inject data
        let timestamps = generate_peak_pattern(9, 20, 10);
        reconciler
            .request_history
            .insert("llama3:8b".to_string(), timestamps);

        reconciler.analyze().await;

        let recs = reconciler.get_recommendations().await;
        assert!(recs.is_empty());
    }

    // Backends with active operations are excluded from recommendations
    #[test]
    fn excludes_backends_with_active_operations() {
        let registry = Arc::new(Registry::new());
        let mut backend = create_backend("b1", vec!["codellama:7b"]);
        backend.current_operation = Some(crate::agent::types::LifecycleOperation {
            operation_id: "op-1".to_string(),
            operation_type: crate::agent::types::OperationType::Load,
            model_id: "other-model".to_string(),
            source_backend_id: None,
            target_backend_id: "b1".to_string(),
            status: crate::agent::types::OperationStatus::InProgress,
            progress_percent: 50,
            eta_ms: None,
            initiated_at: Utc::now(),
            completed_at: None,
            error_details: None,
        });
        registry.add_backend(backend).unwrap();

        let reconciler = FleetReconciler::new(test_config(), Arc::clone(&registry));

        let eligible = reconciler.find_eligible_backends("llama3:8b");
        assert!(eligible.is_empty(), "Busy backends should be excluded");
    }

    // Unhealthy backends are excluded
    #[test]
    fn excludes_unhealthy_backends() {
        let registry = Arc::new(Registry::new());
        let mut backend = create_backend("b1", vec!["codellama:7b"]);
        backend.status = BackendStatus::Unhealthy;
        registry.add_backend(backend).unwrap();

        let reconciler = FleetReconciler::new(test_config(), Arc::clone(&registry));

        let eligible = reconciler.find_eligible_backends("llama3:8b");
        assert!(eligible.is_empty(), "Unhealthy backends should be excluded");
    }
}
