//! Backend configuration

use serde::{Deserialize, Serialize};

/// Backend type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendType {
    Ollama,
    Vllm,
    OpenAI,
    Claude,
}

/// Backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    pub name: String,
    pub url: String,
    #[serde(rename = "type")]
    pub backend_type: BackendType,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default)]
    pub api_key_env: Option<String>,
}

fn default_priority() -> i32 {
    50
}
