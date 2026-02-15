# Data Model: Cloud Backend Support

**Feature**: F12 - Cloud Backend Support  
**Date**: 2025-02-15

## Overview

This document defines the data entities, their relationships, and state transitions for cloud backend support. The design extends existing Nexus entities rather than introducing new core abstractions.

---

## Entity Definitions

### 1. CloudBackendConfig

**Purpose**: Configuration for a cloud LLM API backend  
**Storage**: TOML configuration file  
**Lifecycle**: Loaded at startup, immutable during runtime

**Fields**:
```rust
pub struct BackendConfig {  // Extends existing struct
    pub name: String,                    // e.g., "openai-gpt4"
    pub url: String,                     // e.g., "https://api.openai.com"
    pub backend_type: BackendType,       // openai | anthropic | google
    pub priority: i32,                   // Routing priority (default: 50)
    pub api_key_env: Option<String>,     // Environment variable name
    pub zone: PrivacyZone,               // NEW: restricted | open
    pub tier: Option<u8>,                // NEW: 0-4 capability tier
}
```

**Validation Rules**:
- `name`: Must be unique across all backends
- `url`: Must be valid HTTP/HTTPS URL
- `backend_type`: Must match known cloud provider (enforced by enum)
- `api_key_env`: If specified, environment variable must exist at startup
- `zone`: Defaults to `Open` for cloud backends
- `tier`: If specified, must be 0-4; None means auto-detect from model capabilities

**Relationships**:
- 1 BackendConfig → 1 InferenceAgent (created by factory)
- N BackendConfig → 1 Registry (registered at startup)

**Example**:
```toml
[[backends]]
name = "openai-gpt4"
url = "https://api.openai.com"
type = "openai"
api_key_env = "OPENAI_API_KEY"
zone = "open"
tier = 4
priority = 70
```

---

### 2. CloudAgent (Abstract)

**Purpose**: Implementation of InferenceAgent trait for cloud providers  
**Storage**: In-memory (Arc<dyn InferenceAgent>)  
**Lifecycle**: Created at startup, lives until shutdown

**Concrete Types**:
- `OpenAIAgent` (existing, to be enhanced)
- `AnthropicAgent` (new)
- `GoogleAgent` (new)

**Common Fields** (all implementations):
```rust
pub struct {Provider}Agent {
    id: String,              // UUID for agent instance
    name: String,            // Human-readable name
    base_url: String,        // Provider API base URL
    api_key: String,         // Loaded from env var
    client: Arc<Client>,     // Shared HTTP client
    // Provider-specific fields below
}
```

**OpenAIAgent Enhancements**:
```rust
pub struct OpenAIAgent {
    // ... existing fields ...
    encoding_o200k: Arc<Encoding>,   // NEW: Cached tiktoken encoder
    encoding_cl100k: Arc<Encoding>,  // NEW: Cached tiktoken encoder
}
```

**AnthropicAgent Specific**:
```rust
pub struct AnthropicAgent {
    // ... common fields ...
    anthropic_version: String,  // API version header (e.g., "2023-06-01")
}
```

**GoogleAgent Specific**:
```rust
pub struct GoogleAgent {
    // ... common fields ...
    model_endpoint_base: String,  // "/v1beta/models"
}
```

**State Transitions**: None (agents are stateless; health status tracked by Registry)

**Relationships**:
- 1 CloudAgent → N ModelCapability (from list_models())
- 1 CloudAgent → 1 AgentProfile (metadata)

---

### 3. NexusTransparentHeaders

**Purpose**: Response headers revealing routing decisions  
**Storage**: Serialized to HTTP headers on each response  
**Lifecycle**: Created per request, destroyed after response sent

**Fields**:
```rust
pub struct NexusHeaders {
    pub backend: String,              // Backend name that served request
    pub backend_type: BackendType,    // local | cloud
    pub route_reason: RouteReason,    // Why this backend was chosen
    pub privacy_zone: PrivacyZone,    // restricted | open
    pub cost_estimated: Option<f32>,  // USD cost (cloud only)
}
```

**Validation Rules**:
- `backend`: Must match a registered backend name
- `route_reason`: Must be one of: capability-match, capacity-overflow, privacy-requirement, backend-failover
- `cost_estimated`: Only included for cloud backends; None for local backends
- All fields except cost_estimated are mandatory

**Serialization** (HTTP headers):
```
X-Nexus-Backend: openai-gpt4
X-Nexus-Backend-Type: cloud
X-Nexus-Route-Reason: capacity-overflow
X-Nexus-Privacy-Zone: open
X-Nexus-Cost-Estimated: $0.0042
```

**Relationships**:
- 1 RoutingDecision → 1 NexusHeaders (constructed from routing result)
- 1 NexusHeaders → 1 HTTP Response (injected before return)

---

### 4. ActionableErrorContext

**Purpose**: Structured context for 503 Service Unavailable errors  
**Storage**: Serialized to JSON in error response  
**Lifecycle**: Created when no backend available, destroyed after response sent

**Fields**:
```rust
pub struct ActionableErrorContext {
    pub required_tier: Option<u8>,               // Tier needed for request
    pub available_backends: Vec<BackendStatus>,  // Status of all backends
    pub eta_seconds: Option<u32>,                // Estimated wait time
}

pub struct BackendStatus {
    pub name: String,        // Backend name
    pub status: String,      // "healthy" | "unhealthy" | "at_capacity"
    pub zone: String,        // "restricted" | "open"
}
```

**Validation Rules**:
- `required_tier`: If specified, must be 0-4
- `available_backends`: Must include all backends matching capability
- `eta_seconds`: If specified, must be >0; based on queue depth or average request time
- At least one of {required_tier, available_backends} must be non-empty

**Serialization** (JSON):
```json
{
  "error": {
    "message": "No backend available for request",
    "type": "service_unavailable",
    "code": "capacity_exceeded",
    "context": {
      "required_tier": 4,
      "available_backends": [
        {"name": "openai-gpt4", "status": "at_capacity", "zone": "open"}
      ],
      "eta_seconds": 45
    }
  }
}
```

**State Transitions**: None (ephemeral, created per error)

**Relationships**:
- 1 RoutingError → 1 ActionableErrorContext (created when routing fails)
- 1 ActionableErrorContext → 1 OpenAIErrorResponse (embedded in error object)

---

### 5. PricingConfig

**Purpose**: Token pricing rates for cost estimation  
**Storage**: TOML configuration file  
**Lifecycle**: Loaded at startup, immutable during runtime

**Schema**:
```rust
pub struct PricingConfig {
    pub providers: HashMap<String, ProviderPricing>,
}

pub struct ProviderPricing {
    pub models: HashMap<String, ModelPricing>,
}

pub struct ModelPricing {
    pub input_per_1k: f32,   // USD per 1K input tokens
    pub output_per_1k: f32,  // USD per 1K output tokens
}
```

**Validation Rules**:
- Provider keys: Must match BackendType values ("openai", "anthropic", "google")
- Model keys: Match model identifiers from provider APIs
- Pricing values: Must be ≥0.0 (free models allowed)

**Example Configuration**:
```toml
[pricing.openai]
"gpt-4" = { input_per_1k = 0.03, output_per_1k = 0.06 }
"gpt-3.5-turbo" = { input_per_1k = 0.0005, output_per_1k = 0.0015 }

[pricing.anthropic]
"claude-3-opus" = { input_per_1k = 0.015, output_per_1k = 0.075 }

[pricing.google]
"gemini-2.0-flash" = { input_per_1k = 0.0001, output_per_1k = 0.0004 }
```

**Relationships**:
- 1 PricingConfig → N ModelPricing (one per model)
- 1 ModelPricing → N CostEstimate (used per request)

---

### 6. APITranslation (Ephemeral)

**Purpose**: Request/response translation between provider formats and OpenAI format  
**Storage**: Transient (created per request, not persisted)  
**Lifecycle**: Created during request, destroyed after translation

**Translation Pairs**:

#### Anthropic ↔ OpenAI
```rust
// Request translation
OpenAI ChatCompletionRequest → Anthropic MessagesRequest {
    Extract system message → system field
    Filter messages → remove system from array
    Map roles: direct copy (both use user/assistant)
}

// Response translation
Anthropic MessagesResponse → OpenAI ChatCompletionResponse {
    content[0].text → choices[0].message.content
    stop_reason → finish_reason ("end_turn" → "stop")
    usage.input_tokens → usage.prompt_tokens
}
```

#### Google ↔ OpenAI
```rust
// Request translation
OpenAI ChatCompletionRequest → Google GenerateContentRequest {
    messages → contents
    role: "assistant" → role: "model"
    content (string) → parts: [{text: content}]
    Extract system message → systemInstruction
}

// Response translation
Google GenerateContentResponse → OpenAI ChatCompletionResponse {
    candidates[0].content → choices[0].message
    role: "model" → role: "assistant"
    parts[0].text → content (flatten)
    finishReason → finish_reason (STOP → "stop")
}
```

**No Persistent State**: All translation is pure function transformation

---

## Entity Relationships Diagram

```
┌─────────────────┐
│ BackendConfig   │ (TOML)
│ - name          │
│ - url           │
│ - type          │
│ - api_key_env   │
│ - zone          │ NEW
│ - tier          │ NEW
└────────┬────────┘
         │ 1:1
         │ creates
         ▼
┌─────────────────┐         ┌──────────────────┐
│ CloudAgent      │────────▶│ InferenceAgent   │ (trait)
│ - id            │ impls   │ - chat_completion│
│ - name          │         │ - list_models    │
│ - api_key       │         │ - health_check   │
│ - client        │         └──────────────────┘
└────────┬────────┘
         │ 1:N
         │ registered in
         ▼
┌─────────────────┐
│ Registry        │
│ (in-memory)     │
└────────┬────────┘
         │
         │ routing
         ▼
┌─────────────────┐         ┌──────────────────┐
│ RoutingDecision │────────▶│ NexusHeaders     │ (response)
│ - selected      │ creates │ - backend        │
│ - reason        │         │ - backend_type   │
└─────────────────┘         │ - route_reason   │
                            │ - privacy_zone   │
                            │ - cost_estimated │
                            └──────────────────┘

┌─────────────────┐
│ PricingConfig   │ (TOML)
│ - providers     │
│   - models      │
└────────┬────────┘
         │
         │ used by
         ▼
┌─────────────────┐
│ cost_estimate() │ (function)
│ → f32           │
└─────────────────┘
         │
         │ populates
         ▼
┌─────────────────┐
│ NexusHeaders    │
│ .cost_estimated │
└─────────────────┘
```

---

## State Transitions

### Backend Health Status
(Existing entity, unchanged)

```
┌─────────┐   health_check()   ┌─────────┐
│ Unknown │ ──────────────────▶│ Healthy │
└─────────┘                    └────┬────┘
                                    │
                                    │ failure
                                    ▼
                               ┌──────────┐
                               │ Unhealthy│
                               └────┬─────┘
                                    │
                                    │ recovery
                                    ▼
                               ┌─────────┐
                               │ Healthy │
                               └─────────┘
```

**No new state transitions** - cloud backends use existing health check system

### Request Routing Flow

```
Request
  │
  ▼
┌────────────────┐
│ Parse Model    │
│ Requirement    │
└───────┬────────┘
        │
        ▼
┌────────────────┐     No backends      ┌──────────────────┐
│ Query Registry │────────────────────▶│ Create 503 Error │
│ for Backends   │                     │ + Context        │
└───────┬────────┘                     └──────────────────┘
        │
        │ Backends found
        ▼
┌────────────────┐
│ Route Request  │
│ (capacity,     │
│  privacy,      │
│  priority)     │
└───────┬────────┘
        │
        ▼
┌────────────────┐
│ Execute        │
│ chat_completion│
└───────┬────────┘
        │
        ▼
┌────────────────┐
│ Inject Headers │
│ (NexusHeaders) │
└───────┬────────┘
        │
        ▼
    Response
```

---

## Data Validation

### Startup Validation
- All `api_key_env` references must resolve to environment variables
- Backend URLs must be valid and reachable (health check)
- Pricing config models should match registered backends (warning if mismatch)
- Zone/tier values must be valid enums

### Runtime Validation
- Token counts must be ≥0
- Cost estimates must be ≥0.0
- Model names must match pattern (alphanumeric + hyphens)
- Privacy zone constraints enforced by PrivacyReconciler (F13 integration)

### Error Handling
- Missing API key → log error, skip backend registration
- Invalid health check → mark backend unhealthy
- Missing pricing data → omit cost header (graceful degradation)
- Translation failure → return 422 Unprocessable Entity

---

## Performance Considerations

### Memory Footprint
| Entity | Per-Instance Size | Count | Total |
|--------|------------------|-------|-------|
| BackendConfig | ~200 bytes | 3-10 | ~2KB |
| CloudAgent | ~500 bytes + encodings (~10MB) | 3-10 | ~30-100MB |
| PricingConfig | ~1KB | 1 | ~1KB |
| NexusHeaders | ~200 bytes | Per request | Ephemeral |

**Total Static**: ~30-100MB (encodings dominate; acceptable per Constitution)

### Encoding Cache Strategy
- Tiktoken encodings loaded once at agent creation
- Stored as `Arc<Encoding>` for zero-copy sharing
- Two encodings per OpenAIAgent: o200k_base + cl100k_base
- Total: ~10MB per OpenAI agent (one-time cost)

### Translation Performance
- Request translation: <0.1ms (field mapping only)
- Response translation: <0.2ms (includes JSON parsing)
- Streaming translation: <0.05ms per chunk (incremental)
- No buffering of streamed responses (constant memory)

---

## Integration Points

### Existing Systems
1. **Registry**: Cloud agents registered alongside local agents (no distinction)
2. **Routing**: Existing scoring algorithm handles cloud backends (zone/tier checks added)
3. **Health Check**: Cloud agents participate in periodic health checks
4. **Metrics**: Cloud requests tracked in MetricsCollector (backend_type dimension)

### New Interactions
1. **Privacy Reconciler** (F13): Filters backends by zone before routing
2. **Budget Manager** (F14): Uses cost estimates for budget tracking (future)
3. **Agent Factory**: Extended to create cloud agents based on backend_type

---

## Summary

This data model extends Nexus's existing architecture with minimal new entities:
- **Extended**: BackendConfig (zone/tier fields), OpenAIAgent (encodings)
- **New**: AnthropicAgent, GoogleAgent, NexusHeaders, ActionableErrorContext, PricingConfig
- **Reused**: InferenceAgent trait, Registry, RoutingDecision, HealthStatus

All entities follow Constitution principles: stateless design, in-memory storage, performance-conscious caching.
