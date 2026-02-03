//! Models command implementation

use crate::cli::output::{format_models_json, format_models_table, ModelView};
use crate::cli::ModelsArgs;
use crate::registry::{Model, Registry};
use std::collections::HashMap;

impl ModelView {
    fn from_model(model: &Model, backends: Vec<String>) -> Self {
        Self {
            id: model.id.clone(),
            backends,
            context_length: model.context_length,
        }
    }
}

/// Handle models command
pub fn handle_models(
    args: &ModelsArgs,
    registry: &Registry,
) -> Result<String, Box<dyn std::error::Error>> {
    let backends = if let Some(ref id) = args.backend {
        match registry.get_backend(id) {
            Some(b) => vec![b],
            None => return Err(format!("Backend not found: {}", id).into()),
        }
    } else {
        registry.get_all_backends()
    };

    // Aggregate models with their backends
    let mut model_map: HashMap<String, ModelView> = HashMap::new();
    for backend in backends {
        for model in &backend.models {
            model_map
                .entry(model.id.clone())
                .and_modify(|mv| mv.backends.push(backend.name.clone()))
                .or_insert_with(|| ModelView::from_model(model, vec![backend.name.clone()]));
        }
    }

    let models: Vec<_> = model_map.into_values().collect();

    if args.json {
        Ok(format_models_json(&models))
    } else {
        Ok(format_models_table(&models))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{Backend, BackendType, DiscoverySource, Model};
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

    fn create_test_model(id: &str) -> Model {
        Model {
            id: id.to_string(),
            name: id.to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }
    }

    #[test]
    fn test_models_list_empty() {
        let registry = Arc::new(Registry::new());
        let args = ModelsArgs {
            json: false,
            backend: None,
            config: std::path::PathBuf::from("nexus.toml"),
        };

        let output = handle_models(&args, &registry).unwrap();
        assert!(output.contains("Model")); // Header
    }

    #[test]
    fn test_models_list_aggregated() {
        let registry = Arc::new(Registry::new());

        // Add two backends with overlapping models
        let mut backend1 = create_test_backend();
        backend1.id = "backend1".to_string();
        backend1.models = vec![create_test_model("llama3:70b")];
        registry.add_backend(backend1).unwrap();

        let mut backend2 = create_test_backend();
        backend2.id = "backend2".to_string();
        backend2.name = "Backend 2".to_string();
        backend2.models = vec![
            create_test_model("llama3:70b"),
            create_test_model("mistral:7b"),
        ];
        registry.add_backend(backend2).unwrap();

        let args = ModelsArgs {
            json: false,
            backend: None,
            config: std::path::PathBuf::from("nexus.toml"),
        };
        let output = handle_models(&args, &registry).unwrap();

        assert!(output.contains("llama3:70b"));
        assert!(output.contains("mistral:7b"));
    }

    #[test]
    fn test_models_filter_by_backend() {
        let registry = Arc::new(Registry::new());

        let mut backend1 = create_test_backend();
        backend1.id = "backend1".to_string();
        backend1.models = vec![create_test_model("llama3:70b")];
        registry.add_backend(backend1).unwrap();

        let mut backend2 = create_test_backend();
        backend2.id = "backend2".to_string();
        backend2.models = vec![create_test_model("mistral:7b")];
        registry.add_backend(backend2).unwrap();

        let args = ModelsArgs {
            json: false,
            backend: Some("backend1".to_string()),
            config: std::path::PathBuf::from("nexus.toml"),
        };
        let output = handle_models(&args, &registry).unwrap();

        assert!(output.contains("llama3:70b"));
        assert!(!output.contains("mistral:7b"));
    }

    #[test]
    fn test_models_json_output() {
        let registry = Arc::new(Registry::new());
        let mut backend = create_test_backend();
        backend.models = vec![create_test_model("llama3:70b")];
        registry.add_backend(backend).unwrap();

        let args = ModelsArgs {
            json: true,
            backend: None,
            config: std::path::PathBuf::from("nexus.toml"),
        };
        let output = handle_models(&args, &registry).unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed.get("models").is_some());
    }
}
