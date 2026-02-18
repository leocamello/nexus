//! Backends command implementation

use crate::cli::output::{format_backends_json, format_backends_table, BackendView};
use crate::cli::{BackendsAddArgs, BackendsListArgs, BackendsRemoveArgs};
use crate::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Registry};
use std::collections::HashMap;
use std::time::Duration;

/// Parse status string to BackendStatus
fn parse_status(s: &str) -> Result<BackendStatus, Box<dyn std::error::Error>> {
    match s.to_lowercase().as_str() {
        "healthy" => Ok(BackendStatus::Healthy),
        "unhealthy" => Ok(BackendStatus::Unhealthy),
        "unknown" => Ok(BackendStatus::Unknown),
        "draining" => Ok(BackendStatus::Draining),
        _ => Err(format!(
            "Invalid status: {}. Use: healthy, unhealthy, unknown, draining",
            s
        )
        .into()),
    }
}

/// Handle backends list command
pub fn handle_backends_list(
    args: &BackendsListArgs,
    registry: &Registry,
) -> Result<String, Box<dyn std::error::Error>> {
    let backends = registry.get_all_backends();

    // Filter by status if provided
    let filtered: Vec<Backend> = if let Some(ref status) = args.status {
        let target_status = parse_status(status)?;
        backends
            .into_iter()
            .filter(|b| b.status == target_status)
            .collect()
    } else {
        backends
    };

    // Convert to view models
    let views: Vec<BackendView> = filtered.iter().map(BackendView::from).collect();

    if args.json {
        Ok(format_backends_json(&views))
    } else {
        Ok(format_backends_table(&views))
    }
}

/// Auto-detect backend type by probing known endpoints
/// Detection order: Ollama -> LlamaCpp -> OpenAI-compatible -> Generic
async fn detect_backend_type(base_url: &str) -> Option<BackendType> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .ok()?;

    // Try Ollama: GET /api/tags
    if let Ok(resp) = client.get(format!("{}/api/tags", base_url)).send().await {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if text.contains("models") {
                    tracing::debug!(url = %base_url, "Detected Ollama backend");
                    return Some(BackendType::Ollama);
                }
            }
        }
    }

    // Try LlamaCpp: GET /health
    if let Ok(resp) = client.get(format!("{}/health", base_url)).send().await {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if text.contains("ok") || text.contains("status") {
                    tracing::debug!(url = %base_url, "Detected LlamaCpp backend");
                    return Some(BackendType::LlamaCpp);
                }
            }
        }
    }

    // Try OpenAI-compatible: GET /v1/models
    if let Ok(resp) = client.get(format!("{}/v1/models", base_url)).send().await {
        if resp.status().is_success() {
            tracing::debug!(url = %base_url, "Detected OpenAI-compatible backend");
            return Some(BackendType::Generic); // Could be vLLM, Exo, etc.
        }
    }

    // Fallback: unknown, will use Generic
    tracing::debug!(url = %base_url, "Could not detect backend type, using Generic");
    None
}

/// Handle backends add command
pub async fn handle_backends_add(
    args: &BackendsAddArgs,
    registry: &Registry,
) -> Result<String, Box<dyn std::error::Error>> {
    // Validate URL
    let url = reqwest::Url::parse(&args.url).map_err(|e| format!("Invalid URL: {}", e))?;

    // Generate name if not provided
    let name = args
        .name
        .clone()
        .unwrap_or_else(|| url.host_str().unwrap_or("backend").to_string());

    // Parse or auto-detect backend type
    let backend_type = if let Some(ref type_str) = args.backend_type {
        match type_str.to_lowercase().as_str() {
            "ollama" => BackendType::Ollama,
            "vllm" => BackendType::VLLM,
            "llamacpp" | "llama.cpp" => BackendType::LlamaCpp,
            "exo" => BackendType::Exo,
            "openai" => BackendType::OpenAI,
            "generic" => BackendType::Generic,
            _ => return Err(format!("Unknown backend type: {}", type_str).into()),
        }
    } else {
        tracing::info!(url = %args.url, "Auto-detecting backend type...");
        detect_backend_type(&args.url)
            .await
            .unwrap_or(BackendType::Generic)
    };

    let backend = Backend::new(
        uuid::Uuid::new_v4().to_string(),
        name.clone(),
        args.url.clone(),
        backend_type,
        vec![],
        DiscoverySource::Manual,
        HashMap::new(),
    );

    let id = backend.id.clone();
    registry.add_backend(backend)?;

    tracing::info!(name = %name, id = %id, backend_type = ?backend_type, "Backend added");
    Ok(format!(
        "Added backend '{}' ({}) as {:?}",
        name, id, backend_type
    ))
}

/// Handle backends remove command
pub fn handle_backends_remove(
    args: &BackendsRemoveArgs,
    registry: &Registry,
) -> Result<String, Box<dyn std::error::Error>> {
    registry.remove_backend(&args.name)?;
    Ok(format!("Removed backend: {}", args.name))
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn test_backends_list_empty_registry() {
        let registry = Arc::new(Registry::new());
        let args = BackendsListArgs {
            json: false,
            status: None,
            config: std::path::PathBuf::from("nexus.toml"),
        };

        let output = handle_backends_list(&args, &registry);
        assert!(output.is_ok());
    }

    #[test]
    fn test_backends_list_with_backends() {
        let registry = Arc::new(Registry::new());
        registry.add_backend(create_test_backend()).unwrap();

        let args = BackendsListArgs {
            json: false,
            status: None,
            config: std::path::PathBuf::from("nexus.toml"),
        };
        let output = handle_backends_list(&args, &registry).unwrap();

        assert!(output.contains("Test Backend") || output.contains("test"));
    }

    #[test]
    fn test_backends_list_filter_healthy() {
        let registry = Arc::new(Registry::new());

        let mut healthy = create_test_backend();
        healthy.id = "healthy".to_string();
        registry.add_backend(healthy).unwrap();
        registry
            .update_status("healthy", BackendStatus::Healthy, None)
            .unwrap();

        let mut unhealthy = create_test_backend();
        unhealthy.id = "unhealthy".to_string();
        unhealthy.name = "Unhealthy Backend".to_string();
        registry.add_backend(unhealthy).unwrap();
        registry
            .update_status(
                "unhealthy",
                BackendStatus::Unhealthy,
                Some("error".to_string()),
            )
            .unwrap();

        let args = BackendsListArgs {
            json: false,
            status: Some("healthy".to_string()),
            config: std::path::PathBuf::from("nexus.toml"),
        };
        let output = handle_backends_list(&args, &registry).unwrap();

        assert!(output.contains("Test Backend"));
        assert!(!output.contains("Unhealthy Backend"));
    }

    #[test]
    fn test_backends_list_json_output() {
        let registry = Arc::new(Registry::new());
        registry.add_backend(create_test_backend()).unwrap();

        let args = BackendsListArgs {
            json: true,
            status: None,
            config: std::path::PathBuf::from("nexus.toml"),
        };
        let output = handle_backends_list(&args, &registry).unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed.get("backends").is_some());
    }

    #[tokio::test]
    async fn test_backends_add_success() {
        let registry = Arc::new(Registry::new());
        let args = BackendsAddArgs {
            url: "http://localhost:11434".to_string(),
            name: Some("test".to_string()),
            backend_type: Some("ollama".to_string()),
            priority: 1,
            config: std::path::PathBuf::from("nexus.toml"),
        };

        let result = handle_backends_add(&args, &registry).await;
        assert!(result.is_ok());
        assert_eq!(registry.backend_count(), 1);
    }

    #[tokio::test]
    async fn test_backends_add_generates_name() {
        let registry = Arc::new(Registry::new());
        let args = BackendsAddArgs {
            url: "http://192.168.1.100:8000".to_string(),
            name: None,
            backend_type: Some("vllm".to_string()),
            priority: 1,
            config: std::path::PathBuf::from("nexus.toml"),
        };

        handle_backends_add(&args, &registry).await.unwrap();

        let backends = registry.get_all_backends();
        assert!(!backends[0].name.is_empty());
        assert!(backends[0].name.contains("192.168.1.100"));
    }

    #[tokio::test]
    async fn test_backends_add_invalid_url() {
        let registry = Arc::new(Registry::new());
        let args = BackendsAddArgs {
            url: "not-a-url".to_string(),
            name: None,
            backend_type: None,
            priority: 1,
            config: std::path::PathBuf::from("nexus.toml"),
        };

        let result = handle_backends_add(&args, &registry).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_backends_remove_success() {
        let registry = Arc::new(Registry::new());
        registry.add_backend(create_test_backend()).unwrap();

        let args = BackendsRemoveArgs {
            name: "test-backend".to_string(),
            config: std::path::PathBuf::from("nexus.toml"),
        };
        let result = handle_backends_remove(&args, &registry);

        assert!(result.is_ok());
        assert_eq!(registry.backend_count(), 0);
    }

    #[test]
    fn test_backends_remove_not_found() {
        let registry = Arc::new(Registry::new());

        let args = BackendsRemoveArgs {
            name: "nonexistent".to_string(),
            config: std::path::PathBuf::from("nexus.toml"),
        };
        let result = handle_backends_remove(&args, &registry);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_status_all_variants() {
        assert_eq!(parse_status("healthy").unwrap(), BackendStatus::Healthy);
        assert_eq!(parse_status("unhealthy").unwrap(), BackendStatus::Unhealthy);
        assert_eq!(parse_status("unknown").unwrap(), BackendStatus::Unknown);
        assert_eq!(parse_status("draining").unwrap(), BackendStatus::Draining);
        assert_eq!(parse_status("HEALTHY").unwrap(), BackendStatus::Healthy);
    }

    #[test]
    fn test_parse_status_invalid() {
        let result = parse_status("invalid_status");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_backends_add_unknown_type() {
        let registry = Arc::new(Registry::new());
        let args = BackendsAddArgs {
            url: "http://localhost:8000".to_string(),
            name: Some("test".to_string()),
            backend_type: Some("unknown_type".to_string()),
            priority: 1,
            config: std::path::PathBuf::from("nexus.toml"),
        };

        let result = handle_backends_add(&args, &registry).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_backends_add_all_backend_types() {
        for (type_str, expected) in [
            ("ollama", BackendType::Ollama),
            ("vllm", BackendType::VLLM),
            ("llamacpp", BackendType::LlamaCpp),
            ("llama.cpp", BackendType::LlamaCpp),
            ("exo", BackendType::Exo),
            ("openai", BackendType::OpenAI),
            ("generic", BackendType::Generic),
        ] {
            let registry = Registry::new();
            let args = BackendsAddArgs {
                url: "http://localhost:8000".to_string(),
                name: Some(format!("test-{}", type_str)),
                backend_type: Some(type_str.to_string()),
                priority: 1,
                config: std::path::PathBuf::from("nexus.toml"),
            };
            handle_backends_add(&args, &registry).await.unwrap();
            let backends = registry.get_all_backends();
            assert_eq!(backends[0].backend_type, expected);
        }
    }

    #[test]
    fn test_backends_list_filter_unhealthy() {
        let registry = Arc::new(Registry::new());

        let mut b1 = create_test_backend();
        b1.id = "b1".to_string();
        registry.add_backend(b1).unwrap();
        registry
            .update_status("b1", BackendStatus::Unhealthy, Some("err".to_string()))
            .unwrap();

        let args = BackendsListArgs {
            json: false,
            status: Some("unhealthy".to_string()),
            config: std::path::PathBuf::from("nexus.toml"),
        };
        let output = handle_backends_list(&args, &registry).unwrap();
        assert!(output.contains("Test Backend") || output.contains("b1"));
    }

    #[tokio::test]
    async fn test_detect_backend_type_ollama() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/api/tags")
            .with_status(200)
            .with_body(r#"{"models":[]}"#)
            .create_async()
            .await;

        let result = detect_backend_type(&server.url()).await;
        mock.assert_async().await;
        assert_eq!(result, Some(BackendType::Ollama));
    }

    #[tokio::test]
    async fn test_detect_backend_type_llamacpp() {
        let mut server = mockito::Server::new_async().await;
        let _ollama_mock = server
            .mock("GET", "/api/tags")
            .with_status(404)
            .create_async()
            .await;
        let mock = server
            .mock("GET", "/health")
            .with_status(200)
            .with_body(r#"{"status":"ok"}"#)
            .create_async()
            .await;

        let result = detect_backend_type(&server.url()).await;
        mock.assert_async().await;
        assert_eq!(result, Some(BackendType::LlamaCpp));
    }

    #[tokio::test]
    async fn test_detect_backend_type_openai_compatible() {
        let mut server = mockito::Server::new_async().await;
        let _ollama = server
            .mock("GET", "/api/tags")
            .with_status(404)
            .create_async()
            .await;
        let _health = server
            .mock("GET", "/health")
            .with_status(404)
            .create_async()
            .await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(200)
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;

        let result = detect_backend_type(&server.url()).await;
        mock.assert_async().await;
        assert_eq!(result, Some(BackendType::Generic));
    }

    #[tokio::test]
    async fn test_detect_backend_type_unknown() {
        let mut server = mockito::Server::new_async().await;
        let _m1 = server
            .mock("GET", "/api/tags")
            .with_status(404)
            .create_async()
            .await;
        let _m2 = server
            .mock("GET", "/health")
            .with_status(404)
            .create_async()
            .await;
        let _m3 = server
            .mock("GET", "/v1/models")
            .with_status(404)
            .create_async()
            .await;

        let result = detect_backend_type(&server.url()).await;
        assert_eq!(result, None);
    }
}
