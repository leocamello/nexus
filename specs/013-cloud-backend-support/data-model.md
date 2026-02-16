# Data Model: Cloud Backend Support

**Feature**: F12 - Cloud Backend Support with Nexus-Transparent Protocol  
**Date**: 2024-02-11  
**Status**: Complete

This document defines all entities, relationships, and state transitions for cloud backend support.

---

## Entity Overview

```text
┌─────────────────────┐
│  BackendConfig      │ (TOML configuration)
│  ├─ zone            │ ───┐
│  ├─ tier            │    │ creates
│  └─ api_key_env     │    │
└─────────────────────┘    │
                           ▼
┌─────────────────────────────────────┐
│  CloudInferenceAgent                │ (Runtime instance)
│  ├─ Implements: InferenceAgent      │
│  ├─ profile: AgentProfile           │
│  │   ├─ privacy_zone: PrivacyZone   │ ───┐
│  │   ├─ capabilities               │    │
│  │   └─ agent_type                 │    │ used by
│  ├─ http_client: reqwest::Client   │    │
│  ├─ translator: APITranslator       │    │
│  └─ pricing: ModelPricing           │    │
└─────────────────────────────────────┘    │
                                           ▼
┌─────────────────────┐         ┌──────────────────────┐
│  RoutingResult      │         │  Router              │
│  ├─ backend         │ ◄───────┤  select_backend()    │
│  ├─ actual_model    │         │  (privacy filtering) │
│  ├─ route_reason    │         └──────────────────────┘
│  └─ cost_estimated  │ NEW
└─────────────────────┘
           │ produces
           ▼
┌─────────────────────┐
│  NexusHeaders       │ (Response metadata)
│  ├─ X-Nexus-Backend │
│  ├─ X-Nexus-Backend-Type │
│  ├─ X-Nexus-Route-Reason │
│  ├─ X-Nexus-Privacy-Zone │
│  └─ X-Nexus-Cost-Estimated │
└─────────────────────┘
```

---

## Entity Definitions

### 1. BackendConfig (Extended)

**Location**: `src/config/backend.rs`  
**Purpose**: TOML configuration for backend registration  
**Lifecycle**: Static (loaded at startup from nexus.toml)

**Fields**:
```rust
pub struct BackendConfig {
    // Existing fields
    pub name: String,                  // e.g., "openai-gpt4"
    pub url: String,                   // e.g., "https://api.openai.com/v1"
    pub backend_type: BackendType,     // Ollama | OpenAI | Generic (extended below)
    pub priority: i32,                 // Default: 50
    pub api_key_env: Option<String>,   // Existing but now mandatory for cloud
    
    // NEW fields for F12
    pub zone: Option<PrivacyZone>,     // NEW: Restricted | Open (default: derived from type)
    pub tier: Option<u8>,              // NEW: 1-5 capability tier (default: 3)
}
```

**Validation Rules**:
- `api_key_env` is **required** for cloud backends (OpenAI, Anthropic, Google)
- `zone` defaults to:
  - `PrivacyZone::Restricted` for local backends (Ollama, VLLM, LlamaCpp)
  - `PrivacyZone::Open` for cloud backends (OpenAI, Anthropic, Google)
- `tier` defaults to 3 if not specified
- Environment variable named in `api_key_env` must exist at startup (validated during health check)

**Example TOML**:
```toml
[[backends]]
name = "openai-gpt4"
url = "https://api.openai.com/v1"
type = "openai"
api_key_env = "OPENAI_API_KEY"
zone = "open"
tier = 5
priority = 100
```

---

### 2. BackendType (Extended)

**Location**: `src/registry/backend.rs`  
**Purpose**: Enum for backend classification  
**Change**: Add new variants for cloud providers

**Definition**:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendType {
    // Existing local backends
    Ollama,
    VLLM,
    LlamaCpp,
    Exo,
    LMStudio,
    Generic,
    
    // NEW: Cloud backends
    OpenAI,      // Already exists (modified for cloud)
    Anthropic,   // NEW
    Google,      // NEW (Google Generative AI)
}
```

**Privacy Zone Mapping**:
```rust
impl BackendType {
    pub fn default_privacy_zone(&self) -> PrivacyZone {
        match self {
            BackendType::Ollama | BackendType::VLLM | BackendType::LlamaCpp 
            | BackendType::Exo | BackendType::LMStudio | BackendType::Generic 
                => PrivacyZone::Restricted,
            
            BackendType::OpenAI | BackendType::Anthropic | BackendType::Google 
                => PrivacyZone::Open,
        }
    }
}
```

---

### 3. CloudInferenceAgent

**Location**: `src/agent/{openai.rs, anthropic.rs, google.rs}`  
**Purpose**: Runtime agent instances for cloud backends  
**Lifecycle**: Created by factory during backend registration

**Trait Implementation**:
```rust
pub trait InferenceAgent: Send + Sync + 'static {
    // Existing methods (unchanged)
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn profile(&self) -> AgentProfile;
    async fn health_check(&self) -> Result<HealthStatus, AgentError>;
    async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError>;
    async fn chat_completion(&self, req: ChatCompletionRequest, headers: HeaderMap) 
        -> Result<ChatCompletionResponse, AgentError>;
    async fn chat_completion_stream(&self, req: ChatCompletionRequest, headers: HeaderMap) 
        -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError>;
    
    // Optional methods
    async fn count_tokens(&self, model_id: &str, text: &str) -> TokenCount {
        TokenCount::Heuristic((text.len() / 4) as u32)  // Default implementation
    }
}
```

**OpenAIAgent (Enhanced)**:
```rust
pub struct OpenAIAgent {
    id: String,
    name: String,
    base_url: String,
    api_key: String,                       // Read from env var
    http_client: reqwest::Client,
    pricing: Arc<PricingTable>,            // NEW: for cost estimation
}

impl OpenAIAgent {
    // NEW: Exact token counting using tiktoken-rs
    async fn count_tokens(&self, model_id: &str, text: &str) -> TokenCount {
        let encoding = tiktoken_rs::get_bpe_from_model(model_id).unwrap();
        let tokens = encoding.encode_with_special_tokens(text);
        TokenCount::Exact(tokens.len() as u32)
    }
    
    // AgentProfile with updated capabilities
    fn profile(&self) -> AgentProfile {
        AgentProfile {
            agent_type: "openai".to_string(),
            privacy_zone: PrivacyZone::Open,           // Cloud backend
            capabilities: AgentCapabilities {
                embeddings: true,
                model_lifecycle: false,
                token_counting: true,                   // NEW: enabled
                resource_monitoring: false,
            },
            // ... (other fields)
        }
    }
}
```

**AnthropicAgent (New)**:
```rust
pub struct AnthropicAgent {
    id: String,
    name: String,
    base_url: String,                      // https://api.anthropic.com
    api_key: String,
    http_client: reqwest::Client,
    translator: AnthropicTranslator,       // Handles format conversion
    pricing: Arc<PricingTable>,
}

impl InferenceAgent for AnthropicAgent {
    fn profile(&self) -> AgentProfile {
        AgentProfile {
            agent_type: "anthropic".to_string(),
            privacy_zone: PrivacyZone::Open,
            capabilities: AgentCapabilities {
                embeddings: false,                     // Anthropic doesn't support embeddings
                model_lifecycle: false,
                token_counting: false,                 // Heuristic only (no exact counter yet)
                resource_monitoring: false,
            },
            // ... (other fields)
        }
    }
    
    async fn chat_completion(&self, req: ChatCompletionRequest, headers: HeaderMap) 
        -> Result<ChatCompletionResponse, AgentError> 
    {
        // 1. Translate OpenAI request to Anthropic Messages API format
        let anthropic_req = self.translator.openai_to_anthropic(req)?;
        
        // 2. Send to Anthropic API with x-api-key header
        let response = self.http_client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&anthropic_req)
            .send()
            .await?;
        
        // 3. Translate Anthropic response to OpenAI format
        let anthropic_resp = response.json().await?;
        self.translator.anthropic_to_openai(anthropic_resp)
    }
}
```

**GoogleAIAgent (New)**:
```rust
pub struct GoogleAIAgent {
    id: String,
    name: String,
    base_url: String,                      // https://generativelanguage.googleapis.com
    api_key: String,
    http_client: reqwest::Client,
    translator: GoogleTranslator,
    pricing: Arc<PricingTable>,
}

impl InferenceAgent for GoogleAIAgent {
    fn profile(&self) -> AgentProfile {
        AgentProfile {
            agent_type: "google".to_string(),
            privacy_zone: PrivacyZone::Open,
            capabilities: AgentCapabilities {
                embeddings: true,                      // Gemini supports embeddings
                model_lifecycle: false,
                token_counting: false,                 // Heuristic only
                resource_monitoring: false,
            },
            // ... (other fields)
        }
    }
    
    async fn chat_completion(&self, req: ChatCompletionRequest, headers: HeaderMap) 
        -> Result<ChatCompletionResponse, AgentError> 
    {
        // 1. Translate OpenAI request to Google generateContent format
        let google_req = self.translator.openai_to_google(req)?;
        
        // 2. Send to Google AI API with API key as query parameter
        let response = self.http_client
            .post(format!("{}/v1beta/models/{}:generateContent?key={}", 
                self.base_url, google_req.model, self.api_key))
            .json(&google_req)
            .send()
            .await?;
        
        // 3. Translate Google response to OpenAI format
        let google_resp = response.json().await?;
        self.translator.google_to_openai(google_resp)
    }
}
```

---

### 4. APITranslator

**Location**: `src/agent/translation.rs` (new module)  
**Purpose**: Bidirectional format conversion between OpenAI and provider-specific APIs  
**Lifecycle**: Owned by each CloudInferenceAgent instance

**Trait Definition**:
```rust
pub trait APITranslator: Send + Sync {
    /// Convert OpenAI request to provider-specific format
    fn translate_request(&self, req: ChatCompletionRequest) -> Result<serde_json::Value, TranslationError>;
    
    /// Convert provider response to OpenAI format
    fn translate_response(&self, resp: serde_json::Value) -> Result<ChatCompletionResponse, TranslationError>;
    
    /// Convert streaming chunk to OpenAI SSE format
    fn translate_stream_chunk(&self, chunk: &[u8]) -> Result<Vec<StreamChunk>, TranslationError>;
}
```

**AnthropicTranslator**:
```rust
pub struct AnthropicTranslator;

impl AnthropicTranslator {
    pub fn openai_to_anthropic(&self, req: ChatCompletionRequest) -> Result<AnthropicRequest, TranslationError> {
        let mut messages = Vec::new();
        let mut system_message = None;
        
        // Extract system message and convert roles
        for msg in req.messages {
            match msg.role.as_str() {
                "system" => system_message = Some(msg.content),
                "user" | "assistant" => messages.push(AnthropicMessage {
                    role: msg.role,
                    content: msg.content,
                }),
                _ => return Err(TranslationError::UnsupportedRole(msg.role)),
            }
        }
        
        Ok(AnthropicRequest {
            model: map_model_name(&req.model),  // gpt-4 → claude-3-opus-20240229
            system: system_message,
            messages,
            max_tokens: req.max_tokens.unwrap_or(4096),
            temperature: req.temperature,
            stream: req.stream,
        })
    }
    
    pub fn anthropic_to_openai(&self, resp: AnthropicResponse) -> Result<ChatCompletionResponse, TranslationError> {
        // Extract text from content blocks
        let content = resp.content
            .iter()
            .filter_map(|block| {
                if block.type_ == "text" {
                    Some(block.text.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");
        
        Ok(ChatCompletionResponse {
            id: resp.id,
            object: "chat.completion".to_string(),
            created: Utc::now().timestamp() as u64,
            model: resp.model,
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: "assistant".to_string(),
                    content,
                    function_call: None,
                },
                finish_reason: Some(map_stop_reason(resp.stop_reason)),
            }],
            usage: Some(Usage {
                prompt_tokens: resp.usage.input_tokens,
                completion_tokens: resp.usage.output_tokens,
                total_tokens: resp.usage.input_tokens + resp.usage.output_tokens,
            }),
        })
    }
}
```

**GoogleTranslator**:
```rust
pub struct GoogleTranslator;

impl GoogleTranslator {
    pub fn openai_to_google(&self, req: ChatCompletionRequest) -> Result<GoogleRequest, TranslationError> {
        let mut parts = Vec::new();
        
        // Combine system message and user messages (Google has no system role)
        for msg in req.messages {
            let prefix = match msg.role.as_str() {
                "system" => "System: ",
                "user" => "User: ",
                "assistant" => "Assistant: ",
                _ => return Err(TranslationError::UnsupportedRole(msg.role)),
            };
            parts.push(GooglePart {
                text: format!("{}{}", prefix, msg.content),
            });
        }
        
        Ok(GoogleRequest {
            contents: vec![GoogleContent {
                role: "user".to_string(),
                parts,
            }],
            generation_config: GoogleGenerationConfig {
                temperature: req.temperature,
                max_output_tokens: req.max_tokens.map(|t| t as i32),
            },
        })
    }
    
    pub fn google_to_openai(&self, resp: GoogleResponse) -> Result<ChatCompletionResponse, TranslationError> {
        let candidate = resp.candidates
            .first()
            .ok_or(TranslationError::MissingField("candidates"))?;
        
        let content = candidate.content.parts
            .iter()
            .map(|part| part.text.clone())
            .collect::<Vec<_>>()
            .join("");
        
        Ok(ChatCompletionResponse {
            id: format!("google-{}", uuid::Uuid::new_v4()),
            object: "chat.completion".to_string(),
            created: Utc::now().timestamp() as u64,
            model: resp.model_version.unwrap_or_default(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: "assistant".to_string(),
                    content,
                    function_call: None,
                },
                finish_reason: Some(map_finish_reason(candidate.finish_reason)),
            }],
            usage: resp.usage_metadata.map(|usage| Usage {
                prompt_tokens: usage.prompt_token_count as u32,
                completion_tokens: usage.candidates_token_count as u32,
                total_tokens: usage.total_token_count as u32,
            }),
        })
    }
}
```

---

### 5. NexusTransparentHeaders

**Location**: `src/api/headers.rs` (new module)  
**Purpose**: Standard header set for routing transparency  
**Lifecycle**: Created per request, injected into response

**Structure**:
```rust
pub struct NexusTransparentHeaders {
    pub backend: String,                  // Backend name (e.g., "openai-gpt4")
    pub backend_type: BackendType,        // "local" or "cloud"
    pub route_reason: RouteReason,        // Why this backend was chosen
    pub privacy_zone: PrivacyZone,        // "restricted" or "open"
    pub cost_estimated: Option<f64>,      // USD cost (cloud only)
}

pub enum RouteReason {
    CapabilityMatch,      // Backend has required model/capabilities
    CapacityOverflow,     // Primary backend at capacity
    PrivacyRequirement,   // Privacy zone filtering eliminated others
    Failover,             // Previous backend failed
}

impl NexusTransparentHeaders {
    pub fn inject_into_response(&self, response: &mut Response) {
        let headers = response.headers_mut();
        
        headers.insert(
            HeaderName::from_static("x-nexus-backend"),
            HeaderValue::from_str(&self.backend).unwrap()
        );
        
        headers.insert(
            HeaderName::from_static("x-nexus-backend-type"),
            HeaderValue::from_static(match self.backend_type {
                BackendType::Ollama | BackendType::VLLM | BackendType::LlamaCpp 
                | BackendType::Exo | BackendType::LMStudio | BackendType::Generic => "local",
                BackendType::OpenAI | BackendType::Anthropic | BackendType::Google => "cloud",
            })
        );
        
        headers.insert(
            HeaderName::from_static("x-nexus-route-reason"),
            HeaderValue::from_static(self.route_reason.as_str())
        );
        
        headers.insert(
            HeaderName::from_static("x-nexus-privacy-zone"),
            HeaderValue::from_static(match self.privacy_zone {
                PrivacyZone::Restricted => "restricted",
                PrivacyZone::Open => "open",
            })
        );
        
        if let Some(cost) = self.cost_estimated {
            headers.insert(
                HeaderName::from_static("x-nexus-cost-estimated"),
                HeaderValue::from_str(&format!("{:.4}", cost)).unwrap()
            );
        }
    }
}

impl RouteReason {
    fn as_str(&self) -> &'static str {
        match self {
            RouteReason::CapabilityMatch => "capability-match",
            RouteReason::CapacityOverflow => "capacity-overflow",
            RouteReason::PrivacyRequirement => "privacy-requirement",
            RouteReason::Failover => "failover",
        }
    }
}
```

---

### 6. ActionableErrorContext

**Location**: `src/api/error.rs` (extend existing)  
**Purpose**: Structured context for 503 responses  
**Lifecycle**: Created when routing fails, serialized to JSON

**Structure**:
```rust
#[derive(Debug, Serialize)]
pub struct ActionableErrorContext {
    pub required_tier: Option<u8>,                // Tier needed for request (if applicable)
    pub available_backends: Vec<String>,          // Backends currently available
    pub eta_seconds: Option<u64>,                 // Estimated recovery time
    pub privacy_zone_required: Option<String>,    // If privacy constraint caused failure
}

#[derive(Debug, Serialize)]
pub struct ServiceUnavailableError {
    pub error: OpenAIError,                       // Standard OpenAI error envelope
    pub context: ActionableErrorContext,          // Nexus-specific context
}

impl ServiceUnavailableError {
    pub fn new(message: String, context: ActionableErrorContext) -> Self {
        Self {
            error: OpenAIError {
                message,
                type_: "service_unavailable".to_string(),
                param: None,
                code: None,
            },
            context,
        }
    }
}
```

**Example JSON**:
```json
{
  "error": {
    "message": "No backend available for model 'gpt-4' (tier 5 required)",
    "type": "service_unavailable",
    "param": null,
    "code": null
  },
  "context": {
    "required_tier": 5,
    "available_backends": ["ollama-llama2", "lmstudio-mistral"],
    "eta_seconds": null,
    "privacy_zone_required": null
  }
}
```

---

### 7. ModelPricing

**Location**: `src/agent/pricing.rs` (new module)  
**Purpose**: Cost estimation for cloud models  
**Lifecycle**: Static table, loaded at startup

**Structure**:
```rust
#[derive(Debug, Clone)]
pub struct ModelPricing {
    pub input_price_per_1k: f64,     // USD per 1K input tokens
    pub output_price_per_1k: f64,    // USD per 1K output tokens
}

pub struct PricingTable {
    prices: HashMap<String, ModelPricing>,
}

impl PricingTable {
    pub fn new() -> Self {
        let mut prices = HashMap::new();
        
        // OpenAI
        prices.insert("gpt-4-turbo".to_string(), ModelPricing { 
            input_price_per_1k: 0.01, 
            output_price_per_1k: 0.03 
        });
        prices.insert("gpt-3.5-turbo".to_string(), ModelPricing { 
            input_price_per_1k: 0.0005, 
            output_price_per_1k: 0.0015 
        });
        
        // Anthropic
        prices.insert("claude-3-opus-20240229".to_string(), ModelPricing { 
            input_price_per_1k: 0.015, 
            output_price_per_1k: 0.075 
        });
        prices.insert("claude-3-sonnet-20240229".to_string(), ModelPricing { 
            input_price_per_1k: 0.003, 
            output_price_per_1k: 0.015 
        });
        
        // Google
        prices.insert("gemini-1.5-pro".to_string(), ModelPricing { 
            input_price_per_1k: 0.0035, 
            output_price_per_1k: 0.0105 
        });
        
        Self { prices }
    }
    
    pub fn estimate_cost(&self, model: &str, input_tokens: u32, output_tokens: u32) -> Option<f64> {
        self.prices.get(model).map(|pricing| {
            let input_cost = (input_tokens as f64 / 1000.0) * pricing.input_price_per_1k;
            let output_cost = (output_tokens as f64 / 1000.0) * pricing.output_price_per_1k;
            input_cost + output_cost
        })
    }
}
```

---

### 8. RoutingResult (Extended)

**Location**: `src/routing/mod.rs`  
**Purpose**: Result of routing decision  
**Change**: Add cost estimation field

**Definition**:
```rust
#[derive(Debug)]
pub struct RoutingResult {
    pub backend: Arc<Backend>,           // Selected backend (existing)
    pub actual_model: String,            // Actual model name (existing)
    pub fallback_used: bool,             // Fallback indicator (existing)
    pub route_reason: String,            // Decision explanation (existing)
    pub cost_estimated: Option<f64>,     // NEW: Estimated cost in USD
}
```

**Cost Population**:
```rust
impl Router {
    pub fn select_backend(&self, requirements: &RequestRequirements) -> Result<RoutingResult, RoutingError> {
        // ... (existing routing logic)
        
        // Calculate cost if backend supports token counting
        let cost_estimated = if backend.agent.profile().capabilities.token_counting {
            let input_tokens = backend.agent.count_tokens(&model, &requirements.prompt).await;
            if let TokenCount::Exact(count) = input_tokens {
                backend.agent.estimate_cost(&model, count, 0)  // 0 output tokens (estimated later)
            } else {
                None
            }
        } else {
            None
        };
        
        Ok(RoutingResult {
            backend,
            actual_model,
            fallback_used,
            route_reason,
            cost_estimated,  // NEW
        })
    }
}
```

---

## State Transitions

### Backend Health States

```text
┌─────────┐  startup   ┌─────────┐  health_check()  ┌─────────┐
│ Unknown │ ────────▶  │ Healthy │ ◄───────────────▶ │Unhealthy│
└─────────┘            └─────────┘                   └─────────┘
                            │                              │
                            │  drain()                     │
                            ▼                              │
                       ┌──────────┐  health_check()       │
                       │ Draining │ ─────────────────────▶│
                       └──────────┘
```

**Transitions**:
- `Unknown → Healthy`: First successful health check after startup
- `Healthy → Unhealthy`: Health check fails (API key invalid, network error, timeout)
- `Unhealthy → Healthy`: Health check succeeds after recovery
- `Healthy → Draining`: Manual drain command (out of scope for F12)
- `Draining → Unhealthy`: Health check fails during drain

---

## Relationships

```text
BackendConfig (1) ──creates──▶ (1) CloudInferenceAgent
CloudInferenceAgent (1) ──uses──▶ (1) APITranslator
CloudInferenceAgent (1) ──references──▶ (1) PricingTable
Router (1) ──selects──▶ (N) CloudInferenceAgent
Router (1) ──filters by──▶ PrivacyZone
RoutingResult (1) ──contains──▶ (1) Backend
RoutingResult (1) ──generates──▶ (1) NexusTransparentHeaders
ApiError (1) ──includes──▶ (1) ActionableErrorContext
```

---

## Validation Rules

### BackendConfig Validation
1. `api_key_env` must be set for cloud backends (OpenAI, Anthropic, Google)
2. Environment variable named in `api_key_env` must exist and be non-empty
3. `tier` must be 1-5 if specified
4. `zone` must be "restricted" or "open" if specified
5. `url` must be valid HTTPS URL for cloud backends

### RoutingResult Validation
1. `backend` must have `BackendStatus::Healthy`
2. `actual_model` must exist in backend's model list
3. `cost_estimated` is only populated for cloud backends with exact token counting
4. `route_reason` must be one of: capability-match, capacity-overflow, privacy-requirement, failover

### NexusTransparentHeaders Validation
1. All 5 headers must be present on every proxied response
2. `x-nexus-cost-estimated` is optional (only for cloud with token counting)
3. Header values must be lowercase for HTTP/2 compatibility
4. `cost_estimated` formatted to 4 decimal places

---

## Summary

**New Entities**: 3 (AnthropicAgent, GoogleAIAgent, APITranslator modules)  
**Extended Entities**: 4 (BackendConfig, BackendType, RoutingResult, ApiError)  
**New Modules**: 3 (translation.rs, pricing.rs, headers.rs)  
**Total Changes**: 10 files modified/created

**Next Step**: Generate contracts/ with API format examples and quickstart.md.
