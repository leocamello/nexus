//! Agent factory for creating InferenceAgent trait objects from configuration.

use super::{
    generic::GenericOpenAIAgent, lmstudio::LMStudioAgent, ollama::OllamaAgent, openai::OpenAIAgent,
    AgentError, InferenceAgent,
};
use crate::registry::BackendType;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;

/// Create an agent from backend configuration.
///
/// # Arguments
///
/// * `id` - Unique identifier for the agent
/// * `name` - Human-readable name
/// * `url` - Base URL for the backend
/// * `backend_type` - Type of backend (determines agent implementation)
/// * `client` - Shared HTTP client for connection pooling
/// * `metadata` - Additional configuration (e.g., API keys)
///
/// # Returns
///
/// Returns an `Arc<dyn InferenceAgent>` trait object that can be stored
/// in the registry and used for health checking, model discovery, and inference.
///
/// # Examples
///
/// ```
/// use nexus::agent::factory::create_agent;
/// use nexus::registry::BackendType;
/// use reqwest::Client;
/// use std::collections::HashMap;
/// use std::sync::Arc;
///
/// let client = Arc::new(Client::new());
/// let agent = create_agent(
///     "backend-1".to_string(),
///     "Local Ollama".to_string(),
///     "http://localhost:11434".to_string(),
///     BackendType::Ollama,
///     client,
///     HashMap::new(),
/// ).unwrap();
///
/// assert_eq!(agent.id(), "backend-1");
/// ```
pub fn create_agent(
    id: String,
    name: String,
    url: String,
    backend_type: BackendType,
    client: Arc<Client>,
    metadata: HashMap<String, String>,
) -> Result<Arc<dyn InferenceAgent>, AgentError> {
    match backend_type {
        BackendType::Ollama => Ok(Arc::new(OllamaAgent::new(id, name, url, client))),
        BackendType::OpenAI => {
            // Extract API key from metadata
            // First check for direct "api_key" field, then "api_key_env" for env var lookup
            let api_key = if let Some(key) = metadata.get("api_key") {
                key.clone()
            } else if let Some(env_var) = metadata.get("api_key_env") {
                std::env::var(env_var).map_err(|e| {
                    AgentError::Configuration(format!(
                        "Failed to read API key from env var '{}': {}",
                        env_var, e
                    ))
                })?
            } else {
                return Err(AgentError::Configuration(
                    "OpenAI backend requires 'api_key' or 'api_key_env' in metadata".to_string(),
                ));
            };

            Ok(Arc::new(OpenAIAgent::new(id, name, url, api_key, client)))
        }
        BackendType::LMStudio => Ok(Arc::new(LMStudioAgent::new(id, name, url, client))),
        BackendType::VLLM | BackendType::LlamaCpp | BackendType::Exo | BackendType::Generic => Ok(
            Arc::new(GenericOpenAIAgent::new(id, name, backend_type, url, client)),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_client() -> Arc<Client> {
        Arc::new(Client::new())
    }

    #[test]
    fn test_create_ollama_agent() {
        let agent = create_agent(
            "test-1".to_string(),
            "Test Ollama".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            test_client(),
            HashMap::new(),
        )
        .unwrap();

        assert_eq!(agent.id(), "test-1");
        assert_eq!(agent.name(), "Test Ollama");
        assert_eq!(agent.profile().backend_type, "ollama");
    }

    #[test]
    fn test_create_openai_agent_with_direct_key() {
        let mut metadata = HashMap::new();
        metadata.insert("api_key".to_string(), "sk-test123".to_string());

        let agent = create_agent(
            "test-2".to_string(),
            "Test OpenAI".to_string(),
            "https://api.openai.com".to_string(),
            BackendType::OpenAI,
            test_client(),
            metadata,
        )
        .unwrap();

        assert_eq!(agent.id(), "test-2");
        assert_eq!(agent.profile().backend_type, "openai");
    }

    #[test]
    fn test_create_openai_agent_with_env_key() {
        std::env::set_var("TEST_OPENAI_KEY", "sk-test-env");

        let mut metadata = HashMap::new();
        metadata.insert("api_key_env".to_string(), "TEST_OPENAI_KEY".to_string());

        let agent = create_agent(
            "test-3".to_string(),
            "Test OpenAI Env".to_string(),
            "https://api.openai.com".to_string(),
            BackendType::OpenAI,
            test_client(),
            metadata,
        )
        .unwrap();

        assert_eq!(agent.id(), "test-3");
        assert_eq!(agent.profile().backend_type, "openai");

        std::env::remove_var("TEST_OPENAI_KEY");
    }

    #[test]
    fn test_create_openai_agent_missing_key() {
        let result = create_agent(
            "test-4".to_string(),
            "Test OpenAI No Key".to_string(),
            "https://api.openai.com".to_string(),
            BackendType::OpenAI,
            test_client(),
            HashMap::new(),
        );

        assert!(
            matches!(result, Err(AgentError::Configuration(ref msg)) if msg.contains("api_key"))
        );
    }

    #[test]
    fn test_create_lmstudio_agent() {
        let agent = create_agent(
            "test-5".to_string(),
            "Test LM Studio".to_string(),
            "http://localhost:1234".to_string(),
            BackendType::LMStudio,
            test_client(),
            HashMap::new(),
        )
        .unwrap();

        assert_eq!(agent.id(), "test-5");
        assert_eq!(agent.profile().backend_type, "lmstudio");
    }

    #[test]
    fn test_create_vllm_agent() {
        let agent = create_agent(
            "test-6".to_string(),
            "Test VLLM".to_string(),
            "http://localhost:8000".to_string(),
            BackendType::VLLM,
            test_client(),
            HashMap::new(),
        )
        .unwrap();

        assert_eq!(agent.id(), "test-6");
        assert_eq!(agent.profile().backend_type, "vllm");
    }

    #[test]
    fn test_create_llamacpp_agent() {
        let agent = create_agent(
            "test-7".to_string(),
            "Test LlamaCpp".to_string(),
            "http://localhost:8080".to_string(),
            BackendType::LlamaCpp,
            test_client(),
            HashMap::new(),
        )
        .unwrap();

        assert_eq!(agent.id(), "test-7");
        assert_eq!(agent.profile().backend_type, "llamacpp");
    }

    #[test]
    fn test_create_exo_agent() {
        let agent = create_agent(
            "test-8".to_string(),
            "Test Exo".to_string(),
            "http://localhost:52415".to_string(),
            BackendType::Exo,
            test_client(),
            HashMap::new(),
        )
        .unwrap();

        assert_eq!(agent.id(), "test-8");
        assert_eq!(agent.profile().backend_type, "exo");
    }

    #[test]
    fn test_create_generic_agent() {
        let agent = create_agent(
            "test-9".to_string(),
            "Test Generic".to_string(),
            "http://localhost:9000".to_string(),
            BackendType::Generic,
            test_client(),
            HashMap::new(),
        )
        .unwrap();

        assert_eq!(agent.id(), "test-9");
        assert_eq!(agent.profile().backend_type, "generic");
    }

    #[test]
    fn test_shared_client() {
        let client = test_client();

        let agent1 = create_agent(
            "test-10".to_string(),
            "Agent 1".to_string(),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            client.clone(),
            HashMap::new(),
        )
        .unwrap();

        let agent2 = create_agent(
            "test-11".to_string(),
            "Agent 2".to_string(),
            "http://localhost:1234".to_string(),
            BackendType::LMStudio,
            client.clone(),
            HashMap::new(),
        )
        .unwrap();

        // Both agents should share the same client (connection pooling)
        assert_eq!(agent1.id(), "test-10");
        assert_eq!(agent2.id(), "test-11");
    }
}
