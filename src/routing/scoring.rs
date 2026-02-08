//! Scoring function for smart routing strategy

/// Weights for scoring backend candidates
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScoringWeights {
    /// Weight for backend priority (0-100)
    pub priority: u32,

    /// Weight for backend load/pending requests (0-100)
    pub load: u32,

    /// Weight for backend latency (0-100)
    pub latency: u32,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            priority: 50,
            load: 30,
            latency: 20,
        }
    }
}

impl ScoringWeights {
    /// Validate that weights sum to 100
    pub fn validate(&self) -> Result<(), String> {
        let sum = self.priority + self.load + self.latency;
        if sum != 100 {
            Err(format!("Scoring weights must sum to 100, got {}", sum))
        } else {
            Ok(())
        }
    }
}

/// Score a backend based on its current state and the configured weights
///
/// Returns a score in the range 0-100, where higher is better.
pub fn score_backend(
    priority: u32,
    pending_requests: u32,
    avg_latency_ms: u32,
    weights: &ScoringWeights,
) -> u32 {
    // Priority score: lower priority number = higher score
    let priority_score = 100 - priority.min(100);

    // Load score: fewer pending requests = higher score
    let load_score = 100 - pending_requests.min(100);

    // Latency score: lower latency = higher score
    // Divide by 10 to scale: 0ms=100, 100ms=90, 500ms=50, 1000ms=0
    let latency_score = 100 - (avg_latency_ms / 10).min(100);

    // Weighted average
    (priority_score * weights.priority
        + load_score * weights.load
        + latency_score * weights.latency)
        / 100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_weights_sum_to_100() {
        let weights = ScoringWeights::default();
        assert_eq!(weights.priority + weights.load + weights.latency, 100);
    }

    #[test]
    fn validate_accepts_valid_weights() {
        let weights = ScoringWeights {
            priority: 40,
            load: 40,
            latency: 20,
        };
        assert!(weights.validate().is_ok());
    }

    #[test]
    fn validate_rejects_invalid_weights() {
        let weights = ScoringWeights {
            priority: 50,
            load: 50,
            latency: 50,
        };
        assert!(weights.validate().is_err());
    }

    #[test]
    fn score_with_default_weights() {
        let weights = ScoringWeights::default();
        // Priority 1, no load, 50ms latency
        let score = score_backend(1, 0, 50, &weights);
        // priority_score = 99 * 0.5 = 49.5
        // load_score = 100 * 0.3 = 30
        // latency_score = 95 * 0.2 = 19
        // total = 98.5 ≈ 98
        assert!((97..=99).contains(&score));
    }

    #[test]
    fn score_prioritizes_low_priority() {
        let weights = ScoringWeights::default();
        let score1 = score_backend(1, 0, 100, &weights);
        let score2 = score_backend(10, 0, 100, &weights);
        assert!(score1 > score2);
    }

    #[test]
    fn score_prioritizes_low_load() {
        let weights = ScoringWeights::default();
        let score1 = score_backend(5, 0, 100, &weights);
        let score2 = score_backend(5, 50, 100, &weights);
        assert!(score1 > score2);
    }

    #[test]
    fn score_prioritizes_low_latency() {
        let weights = ScoringWeights::default();
        let score1 = score_backend(5, 0, 50, &weights);
        let score2 = score_backend(5, 0, 500, &weights);
        assert!(score1 > score2);
    }

    #[test]
    fn score_clamps_at_100() {
        let weights = ScoringWeights::default();
        // Best possible: priority 0, no load, 0ms latency
        let score = score_backend(0, 0, 0, &weights);
        assert_eq!(score, 100);
    }

    #[test]
    fn score_handles_high_values() {
        let weights = ScoringWeights::default();
        // All max values should be clamped
        let score = score_backend(1000, 1000, 10000, &weights);
        assert_eq!(score, 0);
    }
}
