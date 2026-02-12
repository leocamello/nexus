//! Request and response types for the OpenAI-compatible API.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Chat completion request matching OpenAI format.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Pass through any additional fields to backend
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// A single message in the conversation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(flatten)]
    pub content: MessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Message content - either text or multimodal parts.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text { content: String },
    Parts { content: Vec<ContentPart> },
}

/// Content part for multimodal messages.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContentPart {
    #[serde(rename = "type")]
    pub part_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<ImageUrl>,
}

/// Image URL for vision requests.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ImageUrl {
    pub url: String,
}

/// Chat completion response (non-streaming).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<Choice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

/// A single choice in the response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Choice {
    pub index: u32,
    pub message: ChatMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Token usage statistics.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Chat completion chunk for streaming responses.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
}

/// A single choice in a streaming chunk.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: ChunkDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Delta content in a streaming chunk.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChunkDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// API error response in OpenAI format.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiError {
    pub error: ApiErrorBody,
}

/// Error details.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiErrorBody {
    pub message: String,
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl ApiError {
    /// Create a bad request error (400).
    pub fn bad_request(message: &str) -> Self {
        Self {
            error: ApiErrorBody {
                message: message.to_string(),
                r#type: "invalid_request_error".to_string(),
                param: None,
                code: Some("invalid_request_error".to_string()),
            },
        }
    }

    /// Create a model not found error (404) with available models hint.
    pub fn model_not_found(model: &str, available: &[String]) -> Self {
        let hint = if available.is_empty() {
            "No models available".to_string()
        } else {
            format!("Available: {}", available.join(", "))
        };
        Self {
            error: ApiErrorBody {
                message: format!("Model '{}' not found. {}", model, hint),
                r#type: "invalid_request_error".to_string(),
                param: Some("model".to_string()),
                code: Some("model_not_found".to_string()),
            },
        }
    }

    /// Create a bad gateway error (502).
    pub fn bad_gateway(message: &str) -> Self {
        Self {
            error: ApiErrorBody {
                message: message.to_string(),
                r#type: "server_error".to_string(),
                param: None,
                code: Some("bad_gateway".to_string()),
            },
        }
    }

    /// Create a gateway timeout error (504).
    pub fn gateway_timeout() -> Self {
        Self {
            error: ApiErrorBody {
                message: "Backend request timed out".to_string(),
                r#type: "server_error".to_string(),
                param: None,
                code: Some("gateway_timeout".to_string()),
            },
        }
    }

    /// Create a service unavailable error (503).
    pub fn service_unavailable(message: &str) -> Self {
        Self {
            error: ApiErrorBody {
                message: message.to_string(),
                r#type: "server_error".to_string(),
                param: None,
                code: Some("service_unavailable".to_string()),
            },
        }
    }

    /// Get the HTTP status code for this error.
    fn status_code(&self) -> StatusCode {
        match self.error.code.as_deref() {
            Some("invalid_request_error") => StatusCode::BAD_REQUEST,
            Some("model_not_found") => StatusCode::NOT_FOUND,
            Some("bad_gateway") => StatusCode::BAD_GATEWAY,
            Some("gateway_timeout") => StatusCode::GATEWAY_TIMEOUT,
            Some("service_unavailable") => StatusCode::SERVICE_UNAVAILABLE,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status_code(), Json(self)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_chat_message_deserialize_text() {
        let json = json!({"role": "user", "content": "Hello"});
        let msg: ChatMessage = serde_json::from_value(json).unwrap();
        assert_eq!(msg.role, "user");
        if let MessageContent::Text { content } = msg.content {
            assert_eq!(content, "Hello");
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_chat_message_deserialize_multimodal() {
        let json = json!({
            "role": "user",
            "content": [
                {"type": "text", "text": "What's in this image?"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,..."}}
            ]
        });
        let msg: ChatMessage = serde_json::from_value(json).unwrap();
        assert_eq!(msg.role, "user");
        if let MessageContent::Parts { content } = msg.content {
            assert_eq!(content.len(), 2);
            assert_eq!(content[0].part_type, "text");
        } else {
            panic!("Expected parts content");
        }
    }

    #[test]
    fn test_chat_request_deserialize_minimal() {
        let json = json!({
            "model": "llama3:70b",
            "messages": [{"role": "user", "content": "Hi"}]
        });
        let req: ChatCompletionRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.model, "llama3:70b");
        assert!(!req.stream); // default false
    }

    #[test]
    fn test_chat_request_deserialize_full() {
        let json = json!({
            "model": "llama3:70b",
            "messages": [{"role": "user", "content": "Hi"}],
            "stream": true,
            "temperature": 0.7,
            "max_tokens": 1000,
            "top_p": 0.9
        });
        let req: ChatCompletionRequest = serde_json::from_value(json).unwrap();
        assert!(req.stream);
        assert_eq!(req.temperature, Some(0.7));
        assert_eq!(req.max_tokens, Some(1000));
        assert_eq!(req.top_p, Some(0.9));
    }

    #[test]
    fn test_chat_request_stream_default_false() {
        let json = json!({
            "model": "test",
            "messages": []
        });
        let req: ChatCompletionRequest = serde_json::from_value(json).unwrap();
        assert!(!req.stream);
    }

    #[test]
    fn test_chat_response_serialize() {
        let response = ChatCompletionResponse {
            id: "chatcmpl-123".to_string(),
            object: "chat.completion".to_string(),
            created: 1699999999,
            model: "llama3:70b".to_string(),
            choices: vec![],
            usage: None,
        };
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["object"], "chat.completion");
        assert_eq!(json["id"], "chatcmpl-123");
        assert_eq!(json["model"], "llama3:70b");
    }

    #[test]
    fn test_chat_chunk_serialize() {
        let chunk = ChatCompletionChunk {
            id: "chatcmpl-123".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1699999999,
            model: "llama3:70b".to_string(),
            choices: vec![],
        };
        let json = serde_json::to_value(&chunk).unwrap();
        assert_eq!(json["object"], "chat.completion.chunk");
        assert_eq!(json["id"], "chatcmpl-123");
    }

    #[test]
    fn test_usage_serialize() {
        let usage = Usage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
        };
        let json = serde_json::to_value(&usage).unwrap();
        assert_eq!(json["prompt_tokens"], 10);
        assert_eq!(json["completion_tokens"], 20);
        assert_eq!(json["total_tokens"], 30);
    }

    #[test]
    fn test_api_error_serialize() {
        let error = ApiError {
            error: ApiErrorBody {
                message: "Test error".to_string(),
                r#type: "invalid_request_error".to_string(),
                param: Some("model".to_string()),
                code: Some("model_not_found".to_string()),
            },
        };
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["error"]["message"], "Test error");
        assert_eq!(json["error"]["type"], "invalid_request_error");
        assert_eq!(json["error"]["code"], "model_not_found");
    }

    #[test]
    fn test_choice_serialize() {
        let choice = Choice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content: MessageContent::Text {
                    content: "Hello!".to_string(),
                },
                name: None,
            },
            finish_reason: Some("stop".to_string()),
        };
        let json = serde_json::to_value(&choice).unwrap();
        assert_eq!(json["index"], 0);
        assert_eq!(json["finish_reason"], "stop");
    }

    #[test]
    fn test_chunk_delta_serialize() {
        let delta = ChunkDelta {
            role: Some("assistant".to_string()),
            content: Some("Hello".to_string()),
        };
        let json = serde_json::to_value(&delta).unwrap();
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["content"], "Hello");
    }

    #[test]
    fn test_api_error_serialize_400() {
        let error = ApiError::bad_request("Invalid JSON");
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["error"]["code"], "invalid_request_error");
        assert_eq!(json["error"]["message"], "Invalid JSON");
    }

    #[test]
    fn test_api_error_serialize_404() {
        let error = ApiError::model_not_found(
            "gpt-4",
            &["llama3:70b".to_string(), "mistral:7b".to_string()],
        );
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["error"]["code"], "model_not_found");
        assert!(json["error"]["message"].as_str().unwrap().contains("gpt-4"));
        assert!(json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("llama3:70b"));
    }

    #[test]
    fn test_api_error_serialize_502() {
        let error = ApiError::bad_gateway("Connection refused");
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["error"]["code"], "bad_gateway");
        assert_eq!(json["error"]["message"], "Connection refused");
    }

    #[test]
    fn test_api_error_into_response() {
        // Test that ApiError implements IntoResponse correctly
        let error = ApiError::service_unavailable("No backends");
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn test_api_error_model_not_found_empty_available() {
        let error = ApiError::model_not_found("gpt-4", &[]);
        let json = serde_json::to_value(&error).unwrap();
        assert!(json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("No models available"));
    }

    #[test]
    fn test_api_error_gateway_timeout() {
        let error = ApiError::gateway_timeout();
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["error"]["code"], "gateway_timeout");
        assert!(json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("timed out"));
    }

    #[test]
    fn test_api_error_status_codes() {
        assert_eq!(
            ApiError::bad_request("x").into_response().status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ApiError::model_not_found("x", &[]).into_response().status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            ApiError::bad_gateway("x").into_response().status(),
            StatusCode::BAD_GATEWAY
        );
        assert_eq!(
            ApiError::gateway_timeout().into_response().status(),
            StatusCode::GATEWAY_TIMEOUT
        );
        assert_eq!(
            ApiError::service_unavailable("x").into_response().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    fn test_api_error_unknown_code_returns_500() {
        let error = ApiError {
            error: ApiErrorBody {
                message: "Unknown".to_string(),
                r#type: "server_error".to_string(),
                param: None,
                code: Some("unknown_code".to_string()),
            },
        };
        assert_eq!(
            error.into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_api_error_no_code_returns_500() {
        let error = ApiError {
            error: ApiErrorBody {
                message: "Unknown".to_string(),
                r#type: "server_error".to_string(),
                param: None,
                code: None,
            },
        };
        assert_eq!(
            error.into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
