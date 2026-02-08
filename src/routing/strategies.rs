//! Routing strategies for backend selection

use std::str::FromStr;

/// Routing strategy determines how backends are selected from candidates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoutingStrategy {
    /// Score backends by priority, load, and latency; select highest
    #[default]
    Smart,

    /// Rotate through backends in round-robin fashion
    RoundRobin,

    /// Always select the backend with lowest priority number
    PriorityOnly,

    /// Randomly select from available backends
    Random,
}

impl FromStr for RoutingStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "smart" => Ok(RoutingStrategy::Smart),
            "round_robin" => Ok(RoutingStrategy::RoundRobin),
            "priority_only" => Ok(RoutingStrategy::PriorityOnly),
            "random" => Ok(RoutingStrategy::Random),
            _ => Err(format!("Unknown routing strategy: {}", s)),
        }
    }
}

impl std::fmt::Display for RoutingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoutingStrategy::Smart => write!(f, "smart"),
            RoutingStrategy::RoundRobin => write!(f, "round_robin"),
            RoutingStrategy::PriorityOnly => write!(f, "priority_only"),
            RoutingStrategy::Random => write!(f, "random"),
        }
    }
}
