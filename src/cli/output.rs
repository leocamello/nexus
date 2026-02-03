//! Output formatting helpers for CLI commands

use crate::registry::{Backend, BackendStatus};
use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Cell, ContentArrangement, Table};
use serde_json::json;
use std::sync::atomic::Ordering;

/// View model for backend display
#[derive(Debug, Clone, serde::Serialize)]
pub struct BackendView {
    pub name: String,
    pub url: String,
    pub backend_type: String,
    pub status: BackendStatus,
    pub models: Vec<String>,
    pub avg_latency_ms: u64,
}

impl From<&Backend> for BackendView {
    fn from(backend: &Backend) -> Self {
        Self {
            name: backend.name.clone(),
            url: backend.url.clone(),
            backend_type: format!("{:?}", backend.backend_type),
            status: backend.status,
            models: backend.models.iter().map(|m| m.id.clone()).collect(),
            avg_latency_ms: backend.avg_latency_ms.load(Ordering::Relaxed) as u64,
        }
    }
}

/// View model for model display
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelView {
    pub id: String,
    pub backends: Vec<String>,
    pub context_length: u32,
}

/// Format backends as a table
pub fn format_backends_table(backends: &[BackendView]) -> String {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec!["Name", "URL", "Type", "Status", "Models", "Latency"]);

    for b in backends {
        let status_str = match b.status {
            BackendStatus::Healthy => "Healthy".green().to_string(),
            BackendStatus::Unhealthy => "Unhealthy".red().to_string(),
            BackendStatus::Unknown => "Unknown".yellow().to_string(),
            BackendStatus::Draining => "Draining".cyan().to_string(),
        };

        table.add_row(vec![
            Cell::new(&b.name),
            Cell::new(&b.url),
            Cell::new(&b.backend_type),
            Cell::new(status_str),
            Cell::new(b.models.len()),
            Cell::new(format!("{}ms", b.avg_latency_ms)),
        ]);
    }

    table.to_string()
}

/// Format backends as JSON
pub fn format_backends_json(backends: &[BackendView]) -> String {
    serde_json::to_string_pretty(&json!({
        "backends": backends
    }))
    .unwrap()
}

/// Format models as a table
pub fn format_models_table(models: &[ModelView]) -> String {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec!["Model", "Backends", "Context Length"]);

    for m in models {
        table.add_row(vec![
            Cell::new(&m.id),
            Cell::new(m.backends.join(", ")),
            Cell::new(m.context_length),
        ]);
    }

    table.to_string()
}

/// Format models as JSON
pub fn format_models_json(models: &[ModelView]) -> String {
    serde_json::to_string_pretty(&json!({
        "models": models
    }))
    .unwrap()
}

/// Get status icon for backend status
pub fn status_icon(status: BackendStatus) -> &'static str {
    match status {
        BackendStatus::Healthy => "✓",
        BackendStatus::Unhealthy => "✗",
        BackendStatus::Unknown => "?",
        BackendStatus::Draining => "~",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_backend_view() -> BackendView {
        BackendView {
            name: "test-backend".to_string(),
            url: "http://localhost:11434".to_string(),
            backend_type: "Ollama".to_string(),
            status: BackendStatus::Healthy,
            models: vec!["llama2".to_string()],
            avg_latency_ms: 50,
        }
    }

    fn create_test_model_view() -> ModelView {
        ModelView {
            id: "llama2".to_string(),
            backends: vec!["backend1".to_string()],
            context_length: 4096,
        }
    }

    #[test]
    fn test_format_backends_table_empty() {
        let output = format_backends_table(&[]);
        assert!(output.contains("Name")); // Header present
    }

    #[test]
    fn test_format_backends_table_with_data() {
        let backends = vec![create_test_backend_view()];
        let output = format_backends_table(&backends);
        assert!(output.contains("test-backend"));
        assert!(output.contains("Healthy"));
    }

    #[test]
    fn test_format_backends_json_valid() {
        let backends = vec![create_test_backend_view()];
        let output = format_backends_json(&backends);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed.get("backends").is_some());
    }

    #[test]
    fn test_format_models_table() {
        let models = vec![create_test_model_view()];
        let output = format_models_table(&models);
        assert!(output.contains("Model"));
        assert!(output.contains("Context"));
    }

    #[test]
    fn test_status_icon_healthy() {
        assert_eq!(status_icon(BackendStatus::Healthy), "✓");
        assert_eq!(status_icon(BackendStatus::Unhealthy), "✗");
        assert_eq!(status_icon(BackendStatus::Unknown), "?");
    }
}
