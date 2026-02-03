//! Health command implementation

use crate::cli::output::BackendView;
use crate::cli::HealthArgs;
use crate::registry::{BackendStatus, Registry};
use colored::Colorize;
use serde::Serialize;
use std::fmt::Write;
use std::time::Duration;

#[derive(Serialize)]
pub struct HealthStatus {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
    pub backends: BackendCounts,
    pub models: ModelCounts,
}

#[derive(Serialize)]
pub struct BackendCounts {
    pub total: usize,
    pub healthy: usize,
    pub unhealthy: usize,
}

#[derive(Serialize)]
pub struct ModelCounts {
    pub total: usize,
}

/// Format duration in a human-readable way
fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, secs)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

/// Format health status as pretty text
fn format_health_pretty(status: &HealthStatus, backends: &[BackendView]) -> String {
    let mut output = String::new();

    let status_display = if status.status == "healthy" {
        "Healthy".green()
    } else {
        "Degraded".yellow()
    };

    writeln!(output, "Status: {}", status_display).unwrap();
    writeln!(output, "Version: {}", status.version).unwrap();
    writeln!(output, "Uptime: {}", format_duration(status.uptime_seconds)).unwrap();
    writeln!(output).unwrap();
    writeln!(
        output,
        "Backends: {}/{} healthy",
        status.backends.healthy, status.backends.total
    )
    .unwrap();
    writeln!(output, "Models: {} available", status.models.total).unwrap();

    if !backends.is_empty() {
        writeln!(output).unwrap();
        writeln!(output, "Backend Details:").unwrap();
        for b in backends {
            let status_icon = match b.status {
                BackendStatus::Healthy => "✓".green(),
                BackendStatus::Unhealthy => "✗".red(),
                BackendStatus::Unknown => "?".yellow(),
                BackendStatus::Draining => "~".cyan(),
            };
            writeln!(
                output,
                "  {} {} ({}) - {} models",
                status_icon,
                b.name,
                b.backend_type,
                b.models.len()
            )
            .unwrap();
        }
    }

    output
}

/// Handle health command
pub fn handle_health(
    args: &HealthArgs,
    registry: &Registry,
    uptime: Duration,
) -> Result<String, Box<dyn std::error::Error>> {
    let backends = registry.get_all_backends();
    let healthy = backends
        .iter()
        .filter(|b| b.status == BackendStatus::Healthy)
        .count();
    let model_count = registry.model_count();

    let status = HealthStatus {
        status: if healthy > 0 || backends.is_empty() {
            "healthy".to_string()
        } else {
            "degraded".to_string()
        },
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime.as_secs(),
        backends: BackendCounts {
            total: backends.len(),
            healthy,
            unhealthy: backends.len() - healthy,
        },
        models: ModelCounts { total: model_count },
    };

    if args.json {
        Ok(serde_json::to_string_pretty(&status)?)
    } else {
        let backend_views: Vec<BackendView> = backends.iter().map(BackendView::from).collect();
        Ok(format_health_pretty(&status, &backend_views))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{Backend, BackendType, DiscoverySource};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn create_test_backend() -> Backend {
        Backend::new(
            "test-backend".to_string(),
            "Test Backend".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            HashMap::new(),
        )
    }

    #[test]
    fn test_health_shows_summary() {
        let registry = Arc::new(Registry::new());

        let mut healthy = create_test_backend();
        healthy.id = "healthy".to_string();
        registry.add_backend(healthy).unwrap();
        registry
            .update_status("healthy", BackendStatus::Healthy, None)
            .unwrap();

        let args = HealthArgs {
            json: false,
            config: std::path::PathBuf::from("nexus.toml"),
        };
        let output = handle_health(&args, &registry, Duration::from_secs(3600)).unwrap();

        assert!(output.contains("Status:"));
        assert!(output.contains("1/1 healthy"));
    }

    #[test]
    fn test_health_degraded_status() {
        let registry = Arc::new(Registry::new());

        // All backends unhealthy = degraded
        let backend = create_test_backend();
        let id = backend.id.clone();
        registry.add_backend(backend).unwrap();
        registry
            .update_status(&id, BackendStatus::Unhealthy, Some("error".to_string()))
            .unwrap();

        let args = HealthArgs {
            json: false,
            config: std::path::PathBuf::from("nexus.toml"),
        };
        let output = handle_health(&args, &registry, Duration::from_secs(0)).unwrap();

        assert!(output.contains("degraded") || output.contains("Degraded"));
    }

    #[test]
    fn test_health_json_valid() {
        let registry = Arc::new(Registry::new());

        let args = HealthArgs {
            json: true,
            config: std::path::PathBuf::from("nexus.toml"),
        };
        let output = handle_health(&args, &registry, Duration::from_secs(100)).unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed.get("status").is_some());
        assert!(parsed.get("uptime_seconds").is_some());
    }

    #[test]
    fn test_health_shows_uptime() {
        let registry = Arc::new(Registry::new());

        let args = HealthArgs {
            json: false,
            config: std::path::PathBuf::from("nexus.toml"),
        };
        let output = handle_health(&args, &registry, Duration::from_secs(3661)).unwrap();

        assert!(output.contains("1h") || output.contains("3661"));
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(3661), "1h 1m 1s");
    }
}
