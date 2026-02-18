//! TXT record parsing utilities

use crate::registry::BackendType;
use std::collections::HashMap;

/// Parsed service information from mDNS TXT records
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedService {
    pub backend_type: BackendType,
    pub api_path: String,
    pub version: Option<String>,
}

/// Parse TXT records into service metadata
pub fn parse_txt_records(txt: &HashMap<String, String>, service_type: &str) -> ParsedService {
    // Try to get type from TXT record first
    let backend_type = if let Some(type_str) = txt.get("type") {
        parse_backend_type(type_str)
    } else {
        // Fall back to inferring from service type
        infer_type_from_service_type(service_type)
    };

    // Extract API path
    let api_path = txt
        .get("api_path")
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            // Default based on backend type
            match backend_type {
                BackendType::Ollama => String::new(),
                _ => "/v1".to_string(),
            }
        });

    // Extract version
    let version = txt.get("version").map(|s| s.to_string());

    ParsedService {
        backend_type,
        api_path,
        version,
    }
}

fn parse_backend_type(type_str: &str) -> BackendType {
    match type_str.to_lowercase().as_str() {
        "ollama" => BackendType::Ollama,
        "vllm" => BackendType::VLLM,
        "llamacpp" | "llama.cpp" => BackendType::LlamaCpp,
        "exo" => BackendType::Exo,
        "openai" => BackendType::OpenAI,
        _ => BackendType::Generic,
    }
}

fn infer_type_from_service_type(service_type: &str) -> BackendType {
    if service_type.contains("_ollama.") {
        BackendType::Ollama
    } else {
        BackendType::Generic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_txt_empty() {
        let txt = HashMap::new();
        let parsed = parse_txt_records(&txt, "_ollama._tcp.local");
        assert_eq!(parsed.backend_type, BackendType::Ollama);
        assert_eq!(parsed.api_path, "");
    }

    #[test]
    fn test_parse_txt_type_vllm() {
        let mut txt = HashMap::new();
        txt.insert("type".to_string(), "vllm".to_string());
        let parsed = parse_txt_records(&txt, "_llm._tcp.local");
        assert_eq!(parsed.backend_type, BackendType::VLLM);
    }

    #[test]
    fn test_parse_txt_api_path() {
        let mut txt = HashMap::new();
        txt.insert("api_path".to_string(), "/v1".to_string());
        let parsed = parse_txt_records(&txt, "_llm._tcp.local");
        assert_eq!(parsed.api_path, "/v1");
    }

    #[test]
    fn test_parse_txt_type_case_insensitive() {
        let mut txt = HashMap::new();
        txt.insert("type".to_string(), "OLLAMA".to_string());
        let parsed = parse_txt_records(&txt, "_llm._tcp.local");
        assert_eq!(parsed.backend_type, BackendType::Ollama);
    }

    #[test]
    fn test_infer_type_from_service_type_ollama() {
        let txt = HashMap::new();
        let parsed = parse_txt_records(&txt, "_ollama._tcp.local");
        assert_eq!(parsed.backend_type, BackendType::Ollama);
    }

    #[test]
    fn test_infer_type_from_service_type_generic() {
        let txt = HashMap::new();
        let parsed = parse_txt_records(&txt, "_llm._tcp.local");
        assert_eq!(parsed.backend_type, BackendType::Generic);
    }

    #[test]
    fn test_parse_txt_unknown_keys_ignored() {
        let mut txt = HashMap::new();
        txt.insert("unknown_key".to_string(), "value".to_string());
        txt.insert("type".to_string(), "ollama".to_string());
        let parsed = parse_txt_records(&txt, "_llm._tcp.local");
        assert_eq!(parsed.backend_type, BackendType::Ollama);
    }

    #[test]
    fn test_parse_txt_type_llamacpp() {
        let mut txt = HashMap::new();
        txt.insert("type".to_string(), "llamacpp".to_string());
        let parsed = parse_txt_records(&txt, "_llm._tcp.local");
        assert_eq!(parsed.backend_type, BackendType::LlamaCpp);
    }

    #[test]
    fn test_parse_txt_type_llama_dot_cpp() {
        let mut txt = HashMap::new();
        txt.insert("type".to_string(), "llama.cpp".to_string());
        let parsed = parse_txt_records(&txt, "_llm._tcp.local");
        assert_eq!(parsed.backend_type, BackendType::LlamaCpp);
    }

    #[test]
    fn test_parse_txt_type_exo() {
        let mut txt = HashMap::new();
        txt.insert("type".to_string(), "exo".to_string());
        let parsed = parse_txt_records(&txt, "_llm._tcp.local");
        assert_eq!(parsed.backend_type, BackendType::Exo);
    }

    #[test]
    fn test_parse_txt_type_openai() {
        let mut txt = HashMap::new();
        txt.insert("type".to_string(), "openai".to_string());
        let parsed = parse_txt_records(&txt, "_llm._tcp.local");
        assert_eq!(parsed.backend_type, BackendType::OpenAI);
    }

    #[test]
    fn test_parse_txt_type_unknown_becomes_generic() {
        let mut txt = HashMap::new();
        txt.insert("type".to_string(), "something_else".to_string());
        let parsed = parse_txt_records(&txt, "_llm._tcp.local");
        assert_eq!(parsed.backend_type, BackendType::Generic);
    }

    #[test]
    fn test_parse_txt_version_extracted() {
        let mut txt = HashMap::new();
        txt.insert("type".to_string(), "ollama".to_string());
        txt.insert("version".to_string(), "0.3.12".to_string());
        let parsed = parse_txt_records(&txt, "_ollama._tcp.local");
        assert_eq!(parsed.version.as_deref(), Some("0.3.12"));
    }

    #[test]
    fn test_parse_txt_no_version() {
        let mut txt = HashMap::new();
        txt.insert("type".to_string(), "vllm".to_string());
        let parsed = parse_txt_records(&txt, "_llm._tcp.local");
        assert!(parsed.version.is_none());
    }

    #[test]
    fn test_parse_txt_ollama_default_api_path_empty() {
        let mut txt = HashMap::new();
        txt.insert("type".to_string(), "ollama".to_string());
        let parsed = parse_txt_records(&txt, "_llm._tcp.local");
        assert_eq!(parsed.api_path, "");
    }

    #[test]
    fn test_parse_txt_non_ollama_default_api_path_v1() {
        let mut txt = HashMap::new();
        txt.insert("type".to_string(), "vllm".to_string());
        let parsed = parse_txt_records(&txt, "_llm._tcp.local");
        assert_eq!(parsed.api_path, "/v1");
    }
}
