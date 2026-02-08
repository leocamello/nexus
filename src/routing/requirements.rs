//! Request requirements extraction

use crate::api::types::{ChatCompletionRequest, MessageContent};

/// Requirements extracted from an incoming request
#[derive(Debug, Clone, PartialEq)]
pub struct RequestRequirements {
    /// The requested model name
    pub model: String,

    /// Estimated token count for context length checking
    pub estimated_tokens: u32,

    /// Whether the request requires vision capability
    pub needs_vision: bool,

    /// Whether the request requires tools/function calling
    pub needs_tools: bool,

    /// Whether the request requires JSON mode
    pub needs_json_mode: bool,
}

impl RequestRequirements {
    /// Extract requirements from a chat completion request
    pub fn from_request(request: &ChatCompletionRequest) -> Self {
        let model = request.model.clone();

        // Estimate tokens: sum of message content lengths divided by 4
        let mut estimated_tokens = 0;
        let mut needs_vision = false;

        for message in &request.messages {
            match &message.content {
                MessageContent::Text { content } => {
                    estimated_tokens += content.len() as u32 / 4;
                }
                MessageContent::Parts { content } => {
                    for part in content {
                        if part.part_type == "text" {
                            if let Some(text) = &part.text {
                                estimated_tokens += text.len() as u32 / 4;
                            }
                        } else if part.part_type == "image_url" {
                            needs_vision = true;
                        }
                    }
                }
            }
        }

        // Check for tools in extra fields
        let needs_tools = request.extra.contains_key("tools");

        // Check for JSON mode in response_format
        let needs_json_mode = request
            .extra
            .get("response_format")
            .and_then(|v: &serde_json::Value| v.as_object())
            .and_then(|obj: &serde_json::Map<String, serde_json::Value>| obj.get("type"))
            .and_then(|v: &serde_json::Value| v.as_str())
            .map(|t: &str| t == "json_object")
            .unwrap_or(false);

        Self {
            model,
            estimated_tokens,
            needs_vision,
            needs_tools,
            needs_json_mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{ChatMessage, ContentPart, ImageUrl};
    use std::collections::HashMap;

    fn create_simple_request(model: &str, content: &str) -> ChatCompletionRequest {
        ChatCompletionRequest {
            model: model.to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text {
                    content: content.to_string(),
                },
                name: None,
            }],
            stream: false,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            extra: HashMap::new(),
        }
    }

    fn create_vision_request(model: &str, image_url: &str) -> ChatCompletionRequest {
        ChatCompletionRequest {
            model: model.to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Parts {
                    content: vec![
                        ContentPart {
                            part_type: "text".to_string(),
                            text: Some("What's in this image?".to_string()),
                            image_url: None,
                        },
                        ContentPart {
                            part_type: "image_url".to_string(),
                            text: None,
                            image_url: Some(ImageUrl {
                                url: image_url.to_string(),
                            }),
                        },
                    ],
                },
                name: None,
            }],
            stream: false,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            extra: HashMap::new(),
        }
    }

    fn create_tools_request(model: &str) -> ChatCompletionRequest {
        let mut extra = HashMap::new();
        extra.insert(
            "tools".to_string(),
            serde_json::json!([{"type": "function", "function": {"name": "get_weather"}}]),
        );

        ChatCompletionRequest {
            model: model.to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text {
                    content: "What's the weather?".to_string(),
                },
                name: None,
            }],
            stream: false,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            extra,
        }
    }

    fn create_json_mode_request(model: &str) -> ChatCompletionRequest {
        let mut extra = HashMap::new();
        extra.insert(
            "response_format".to_string(),
            serde_json::json!({"type": "json_object"}),
        );

        ChatCompletionRequest {
            model: model.to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text {
                    content: "Return JSON".to_string(),
                },
                name: None,
            }],
            stream: false,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            extra,
        }
    }

    #[test]
    fn extracts_model_name() {
        let request = create_simple_request("llama3:8b", "Hello");
        let requirements = RequestRequirements::from_request(&request);
        assert_eq!(requirements.model, "llama3:8b");
    }

    #[test]
    fn estimates_tokens_from_content() {
        let content = "a".repeat(1000);
        let request = create_simple_request("llama3:8b", &content);
        let requirements = RequestRequirements::from_request(&request);
        assert!(requirements.estimated_tokens >= 250); // 1000 chars / 4
    }

    #[test]
    fn detects_vision_requirement() {
        let request = create_vision_request("llava", "http://example.com/image.jpg");
        let requirements = RequestRequirements::from_request(&request);
        assert!(requirements.needs_vision);
    }

    #[test]
    fn detects_tools_requirement() {
        let request = create_tools_request("llama3:8b");
        let requirements = RequestRequirements::from_request(&request);
        assert!(requirements.needs_tools);
    }

    #[test]
    fn detects_json_mode_requirement() {
        let request = create_json_mode_request("llama3:8b");
        let requirements = RequestRequirements::from_request(&request);
        assert!(requirements.needs_json_mode);
    }

    #[test]
    fn simple_request_has_no_special_requirements() {
        let request = create_simple_request("llama3:8b", "Hello");
        let requirements = RequestRequirements::from_request(&request);
        assert!(!requirements.needs_vision);
        assert!(!requirements.needs_tools);
        assert!(!requirements.needs_json_mode);
    }
}
