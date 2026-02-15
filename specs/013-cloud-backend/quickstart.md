# Quickstart: Cloud Backend Support

**Feature**: F12 - Cloud Backend Support with Nexus-Transparent Protocol  
**Target**: Developers implementing cloud backend integration  
**Time**: ~15 minutes to understand, 2-4 hours to implement

---

## Overview

This quickstart walks you through implementing cloud backend support in Nexus. You'll learn how to:

1. Enhance the existing `OpenAIAgent` with token counting
2. Implement `AnthropicAgent` with API translation
3. Add X-Nexus-* response headers
4. Create actionable 503 error responses

**Prerequisites**: Familiarity with Rust, async/await, and the Nexus codebase structure.

---

## Architecture Quick Reference

```
Request → API Layer → Routing → Cloud Agent → API Translation → Cloud Provider
                                     ↓
                                 Response
                                     ↓
                             Header Injection → Client
```

**Key Components**:
- `src/agent/{openai,anthropic,google}.rs` - Cloud agent implementations
- `src/api/headers.rs` - X-Nexus-* header builder
- `src/api/error.rs` - Actionable 503 error context
- `src/config/backend.rs` - Backend config with zone/tier fields

---

## Step 1: Enhance OpenAI Agent with Token Counting (30 minutes)

### 1.1 Add tiktoken-rs dependency

```toml
# Cargo.toml
[dependencies]
tiktoken-rs = "0.5"
```

### 1.2 Add encodings to OpenAIAgent struct

```rust
// src/agent/openai.rs
use tiktoken_rs::{get_encoding, CoreBPE};
use std::sync::Arc;

pub struct OpenAIAgent {
    id: String,
    name: String,
    base_url: String,
    api_key: String,
    client: Arc<Client>,
    // NEW: Cached encodings
    encoding_o200k: Arc<CoreBPE>,
    encoding_cl100k: Arc<CoreBPE>,
}
```

### 1.3 Initialize encodings in constructor

```rust
impl OpenAIAgent {
    pub fn new(
        id: String,
        name: String,
        base_url: String,
        api_key: String,
        client: Arc<Client>,
    ) -> Result<Self, AgentError> {
        // Load encodings once at creation
        let encoding_o200k = Arc::new(
            get_encoding("o200k_base")
                .map_err(|e| AgentError::Configuration(format!("Failed to load o200k encoding: {}", e)))?
        );
        let encoding_cl100k = Arc::new(
            get_encoding("cl100k_base")
                .map_err(|e| AgentError::Configuration(format!("Failed to load cl100k encoding: {}", e)))?
        );
        
        Ok(Self {
            id,
            name,
            base_url,
            api_key,
            client,
            encoding_o200k,
            encoding_cl100k,
        })
    }
}
```

### 1.4 Implement count_tokens method

```rust
#[async_trait]
impl InferenceAgent for OpenAIAgent {
    // ... existing methods ...
    
    async fn count_tokens(&self, model_id: &str, text: &str) -> TokenCount {
        // Select encoding based on model
        let encoding = if model_id.contains("gpt-4o") {
            &self.encoding_o200k
        } else {
            &self.encoding_cl100k
        };
        
        // Encode and count
        let tokens = encoding.encode_ordinary(text);
        TokenCount::Exact(tokens.len() as u32)
    }
}
```

### 1.5 Update profile to indicate token counting support

```rust
fn profile(&self) -> AgentProfile {
    AgentProfile {
        backend_type: "openai".to_string(),
        version: None,
        privacy_zone: PrivacyZone::Open,
        capabilities: AgentCapabilities {
            embeddings: false,
            model_lifecycle: false,
            token_counting: true,  // NOW TRUE
            resource_monitoring: false,
        },
    }
}
```

**Testing**:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_token_counting_gpt4o() {
        let agent = create_test_agent();
        let count = agent.count_tokens("gpt-4o", "Hello world").await;
        
        match count {
            TokenCount::Exact(n) => assert!(n > 0 && n < 10),
            _ => panic!("Expected exact count"),
        }
    }
}
```

---

## Step 2: Implement Anthropic Agent (60 minutes)

### 2.1 Create anthropic.rs module

```rust
// src/agent/anthropic.rs
use super::{AgentError, AgentProfile, HealthStatus, InferenceAgent, ...};
use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};
use async_trait::async_trait;
use axum::http::HeaderMap;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub struct AnthropicAgent {
    id: String,
    name: String,
    base_url: String,
    api_key: String,
    client: Arc<Client>,
}

impl AnthropicAgent {
    pub fn new(
        id: String,
        name: String,
        base_url: String,
        api_key: String,
        client: Arc<Client>,
    ) -> Self {
        Self { id, name, base_url, api_key, client }
    }
}
```

### 2.2 Define Anthropic API types

```rust
// Anthropic request format
#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,  // "user" or "assistant"
    content: String,
}

// Anthropic response format
#[derive(Deserialize)]
struct AnthropicResponse {
    id: String,
    #[serde(rename = "type")]
    response_type: String,
    role: String,
    content: Vec<AnthropicContent>,
    model: String,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}
```

### 2.3 Implement request translation

```rust
fn translate_request(openai_req: &ChatCompletionRequest) -> AnthropicRequest {
    // Extract system message
    let system_message = openai_req.messages
        .iter()
        .find(|m| m.role == "system")
        .map(|m| m.content.clone());
    
    // Filter out system messages and convert to Anthropic format
    let messages: Vec<AnthropicMessage> = openai_req.messages
        .iter()
        .filter(|m| m.role != "system")
        .map(|m| AnthropicMessage {
            role: m.role.clone(),
            content: m.content.clone(),
        })
        .collect();
    
    AnthropicRequest {
        model: openai_req.model.clone(),
        max_tokens: openai_req.max_tokens.unwrap_or(4096),
        system: system_message,
        messages,
        temperature: openai_req.temperature,
    }
}
```

### 2.4 Implement response translation

```rust
fn translate_response(anthropic_resp: AnthropicResponse) -> ChatCompletionResponse {
    let content = anthropic_resp.content
        .first()
        .map(|c| c.text.clone())
        .unwrap_or_default();
    
    let finish_reason = match anthropic_resp.stop_reason.as_deref() {
        Some("end_turn") => "stop",
        Some("max_tokens") => "length",
        _ => "stop",
    }.to_string();
    
    ChatCompletionResponse {
        id: format!("chatcmpl-{}", anthropic_resp.id),
        object: "chat.completion".to_string(),
        created: chrono::Utc::now().timestamp() as u64,
        model: anthropic_resp.model,
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content,
            },
            finish_reason: Some(finish_reason),
        }],
        usage: Some(Usage {
            prompt_tokens: anthropic_resp.usage.input_tokens,
            completion_tokens: anthropic_resp.usage.output_tokens,
            total_tokens: anthropic_resp.usage.input_tokens + anthropic_resp.usage.output_tokens,
        }),
    }
}
```

### 2.5 Implement InferenceAgent trait

```rust
#[async_trait]
impl InferenceAgent for AnthropicAgent {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    
    fn profile(&self) -> AgentProfile {
        AgentProfile {
            backend_type: "anthropic".to_string(),
            version: None,
            privacy_zone: PrivacyZone::Open,
            capabilities: AgentCapabilities::default(),
        }
    }
    
    async fn health_check(&self) -> Result<HealthStatus, AgentError> {
        // Anthropic doesn't have /v1/models, use /v1/complete with minimal request
        let url = format!("{}/v1/messages", self.base_url);
        
        let response = self.client
            .get(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| AgentError::Network(e.to_string()))?;
        
        if response.status().is_success() || response.status() == 400 {
            // 400 is acceptable (empty request), means API is reachable
            Ok(HealthStatus::Healthy)
        } else {
            Ok(HealthStatus::Unhealthy)
        }
    }
    
    async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
        // Anthropic model list is static (no discovery API)
        Ok(vec![
            ModelCapability {
                name: "claude-3-opus-20240229".to_string(),
                context_length: 200_000,
                supports_vision: true,
                supports_tools: true,
            },
            ModelCapability {
                name: "claude-3-sonnet-20240229".to_string(),
                context_length: 200_000,
                supports_vision: true,
                supports_tools: true,
            },
        ])
    }
    
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
        _headers: Option<&HeaderMap>,
    ) -> Result<ChatCompletionResponse, AgentError> {
        let url = format!("{}/v1/messages", self.base_url);
        let anthropic_req = translate_request(&request);
        
        let response = self.client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&anthropic_req)
            .timeout(Duration::from_secs(60))
            .send()
            .await
            .map_err(|e| AgentError::Network(e.to_string()))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AgentError::Upstream(status.as_u16(), body));
        }
        
        let anthropic_resp: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| AgentError::InvalidResponse(e.to_string()))?;
        
        Ok(translate_response(anthropic_resp))
    }
    
    async fn chat_completion_stream(
        &self,
        _request: ChatCompletionRequest,
        _headers: Option<&HeaderMap>,
    ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
        // TODO: Implement streaming translation
        Err(AgentError::Unsupported("streaming"))
    }
}
```

**Testing**:
```rust
#[tokio::test]
async fn test_anthropic_translation() {
    let openai_req = ChatCompletionRequest {
        model: "claude-3-opus".to_string(),
        messages: vec![
            ChatMessage { role: "system".to_string(), content: "You are helpful".to_string() },
            ChatMessage { role: "user".to_string(), content: "Hello".to_string() },
        ],
        max_tokens: Some(100),
        temperature: Some(0.7),
        ..Default::default()
    };
    
    let anthropic_req = translate_request(&openai_req);
    
    assert_eq!(anthropic_req.system, Some("You are helpful".to_string()));
    assert_eq!(anthropic_req.messages.len(), 1);  // system removed
    assert_eq!(anthropic_req.messages[0].role, "user");
}
```

---

## Step 3: Add X-Nexus-* Response Headers (45 minutes)

### 3.1 Create headers.rs module

```rust
// src/api/headers.rs
use crate::agent::PrivacyZone;
use crate::registry::BackendType;
use axum::http::{HeaderMap, HeaderName, HeaderValue};

pub enum RouteReason {
    CapabilityMatch,
    CapacityOverflow,
    PrivacyRequirement,
    BackendFailover,
}

impl RouteReason {
    fn as_str(&self) -> &'static str {
        match self {
            Self::CapabilityMatch => "capability-match",
            Self::CapacityOverflow => "capacity-overflow",
            Self::PrivacyRequirement => "privacy-requirement",
            Self::BackendFailover => "backend-failover",
        }
    }
}

pub struct NexusHeaders {
    pub backend: String,
    pub backend_type: BackendType,
    pub route_reason: RouteReason,
    pub privacy_zone: PrivacyZone,
    pub cost_estimated: Option<f32>,
}

impl NexusHeaders {
    pub fn inject_into(self, headers: &mut HeaderMap) -> Result<(), String> {
        headers.insert(
            HeaderName::from_static("x-nexus-backend"),
            HeaderValue::from_str(&self.backend)
                .map_err(|e| format!("Invalid backend name: {}", e))?,
        );
        
        headers.insert(
            HeaderName::from_static("x-nexus-backend-type"),
            HeaderValue::from_static(match self.backend_type {
                BackendType::Ollama | BackendType::Generic => "local",
                BackendType::OpenAI | BackendType::Anthropic | BackendType::Google => "cloud",
            }),
        );
        
        headers.insert(
            HeaderName::from_static("x-nexus-route-reason"),
            HeaderValue::from_static(self.route_reason.as_str()),
        );
        
        headers.insert(
            HeaderName::from_static("x-nexus-privacy-zone"),
            HeaderValue::from_static(match self.privacy_zone {
                PrivacyZone::Restricted => "restricted",
                PrivacyZone::Open => "open",
            }),
        );
        
        if let Some(cost) = self.cost_estimated {
            headers.insert(
                HeaderName::from_static("x-nexus-cost-estimated"),
                HeaderValue::from_str(&format!("${:.4}", cost))
                    .map_err(|e| format!("Invalid cost: {}", e))?,
            );
        }
        
        Ok(())
    }
}
```

### 3.2 Integrate into completions handler

```rust
// src/api/completions.rs
use crate::api::headers::{NexusHeaders, RouteReason};

pub async fn chat_completion(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<impl IntoResponse, OpenAIError> {
    // ... existing routing logic ...
    
    // Execute request
    let mut response = agent.chat_completion(request, Some(&headers)).await?;
    
    // Create Nexus headers
    let nexus_headers = NexusHeaders {
        backend: agent.name().to_string(),
        backend_type: backend.backend_type.clone(),
        route_reason: determine_route_reason(&routing_decision),
        privacy_zone: backend.zone,
        cost_estimated: calculate_cost(&backend, &response),
    };
    
    // Inject headers
    let mut response_headers = HeaderMap::new();
    nexus_headers.inject_into(&mut response_headers)?;
    
    Ok((response_headers, Json(response)))
}
```

---

## Step 4: Implement Actionable 503 Errors (30 minutes)

### 4.1 Create error.rs module

```rust
// src/api/error.rs
use serde::Serialize;

#[derive(Serialize)]
pub struct ActionableErrorContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_tier: Option<u8>,
    pub available_backends: Vec<BackendStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_seconds: Option<u32>,
}

#[derive(Serialize)]
pub struct BackendStatus {
    pub name: String,
    pub status: String,
    pub zone: String,
}

pub fn create_503_response(
    context: ActionableErrorContext,
) -> Result<impl IntoResponse, OpenAIError> {
    let error = OpenAIError {
        message: "No backend available for request".to_string(),
        error_type: "service_unavailable".to_string(),
        code: Some("capacity_exceeded".to_string()),
        param: None,
        context: Some(serde_json::to_value(context).unwrap()),
    };
    
    Ok((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse { error })
    ))
}
```

### 4.2 Use in routing failures

```rust
// src/api/completions.rs
pub async fn chat_completion(...) -> Result<impl IntoResponse, OpenAIError> {
    let backends = state.registry.find_backends(&request.model);
    
    if backends.is_empty() {
        let context = ActionableErrorContext {
            required_tier: Some(4),  // Example
            available_backends: state.registry.all_backends()
                .map(|b| BackendStatus {
                    name: b.name.clone(),
                    status: if b.healthy { "healthy" } else { "unhealthy" },
                    zone: b.zone.as_str().to_string(),
                })
                .collect(),
            eta_seconds: estimate_eta(&state),
        };
        
        return create_503_response(context);
    }
    
    // ... continue normal routing ...
}
```

---

## Step 5: Configuration Updates (15 minutes)

### 5.1 Extend BackendConfig

```rust
// src/config/backend.rs
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
    #[serde(default = "default_zone")]
    pub zone: PrivacyZone,
    #[serde(default)]
    pub tier: Option<u8>,
}

fn default_zone() -> PrivacyZone {
    PrivacyZone::Open
}
```

### 5.2 Register new backend types

```rust
// src/registry/mod.rs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BackendType {
    Ollama,
    Generic,
    OpenAI,
    Anthropic,  // NEW
    Google,     // NEW
    LMStudio,
}
```

---

## Testing Checklist

### Unit Tests
- [ ] Token counting with tiktoken-rs (GPT-4o and GPT-3.5)
- [ ] Anthropic request translation (system message extraction)
- [ ] Anthropic response translation (content flattening)
- [ ] Header injection (all 5 headers present)
- [ ] Cost estimation calculation

### Integration Tests
- [ ] OpenAI cloud backend registration from config
- [ ] Anthropic agent health check with mock API
- [ ] Complete request flow with header validation
- [ ] 503 error with context object structure

### Contract Tests
- [ ] Response headers match nexus-headers.yaml schema
- [ ] 503 error matches actionable-error.json schema
- [ ] OpenAI response body unchanged (JSON comparison)

---

## Common Pitfalls

1. **Encoding cache miss**: Always initialize tiktoken encodings in agent constructor, not lazily
2. **System message handling**: Anthropic requires system in separate field; don't forget to filter from messages array
3. **Streaming headers**: Headers must be sent before first SSE chunk; use Response builder
4. **Cost estimation**: Always check for pricing config; gracefully omit header if missing
5. **Privacy zone defaults**: Cloud backends should default to "open" zone

---

## Next Steps

After implementing core functionality:

1. **Google Agent**: Follow Anthropic pattern but with Google-specific translation
2. **Streaming Support**: Implement streaming translation for Anthropic/Google
3. **Advanced Pricing**: Add pricing update mechanism without restarts
4. **Monitoring**: Add metrics for cloud vs local routing decisions

---

## References

- [Feature Spec](./spec.md) - Full requirements
- [Data Model](./data-model.md) - Entity definitions
- [Research](./research.md) - API format details
- [Contracts](./contracts/) - API schemas and examples
