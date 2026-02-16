# Cloud Backend Support - Code Walkthrough

**Feature**: F12 - Cloud Backend Support with Nexus-Transparent Protocol  
**Audience**: Junior developers joining the project  
**Last Updated**: 2026-02-16

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: registry/backend.rs - New Backend Types](#file-1-registrybackendrs---new-backend-types)
4. [File 2: config/backend.rs - Cloud Configuration](#file-2-configbackendrs---cloud-configuration)
5. [File 3: agent/factory.rs - Creating Cloud Agents](#file-3-agentfactoryrs---creating-cloud-agents)
6. [File 4: agent/openai.rs - OpenAI Agent (Tiktoken)](#file-4-agentopenairs---openai-agent-tiktoken)
7. [File 5: agent/anthropic.rs - Anthropic Agent](#file-5-agentanthropicrs---anthropic-agent)
8. [File 6: agent/google.rs - Google AI Agent](#file-6-agentgooglers---google-ai-agent)
9. [File 7: agent/pricing.rs - Cost Estimation](#file-7-agentpricingrs---cost-estimation)
10. [File 8: api/headers.rs - The Transparent Protocol](#file-8-apiheadersrs---the-transparent-protocol)
11. [File 9: api/error.rs - Actionable 503 Errors](#file-9-apierrorrs---actionable-503-errors)
12. [File 10: api/completions.rs - Wiring It All Together](#file-10-apicompletionsrs---wiring-it-all-together)
13. [Understanding the Tests](#understanding-the-tests)
14. [Key Rust Concepts](#key-rust-concepts)
15. [Common Patterns in This Codebase](#common-patterns-in-this-codebase)
16. [Next Steps](#next-steps)

---

## The Big Picture

Think of Nexus as a **universal remote control for AI backends**. Before this feature, that remote only worked with local devices — Ollama, vLLM, LM Studio servers running on your network. Cloud Backend Support adds the ability to also talk to cloud services like OpenAI, Anthropic, and Google, all through the same buttons.

The trick is that each cloud provider speaks a **different language**:

- OpenAI speaks... OpenAI format (easy — Nexus already speaks this)
- Anthropic speaks the Messages API (different message structure, different streaming format)
- Google speaks the Generative AI API (completely different JSON schema, different roles)

Each agent acts as a **translator**: your app sends OpenAI-format requests, and the agent converts them to whatever the cloud provider expects, then converts the response back.

### What Problem Does This Solve?

Without F12, if you wanted to use GPT-4 alongside your local Ollama models, you'd need two separate API endpoints in your app. With F12, you configure both in `nexus.example.toml` and your app talks to one URL — Nexus handles which backend gets each request.

### How Cloud Backends Fit Into Nexus

```
┌─────────────────────────────────────────────────────────────────────────┐
│                               Nexus                                     │
│                                                                         │
│  ┌──────────┐     ┌──────────┐     ┌───────────────────────────────┐    │
│  │   API    │────▶│  Router  │────▶│  Registry (Dual Storage)      │    │
│  │ Gateway  │     │          │     │                               │    │
│  └──────────┘     └──────────┘     │  backends: DashMap<Backend>   │    │
│       │                            │  agents:   DashMap<Agent>     │    │
│       │                            └──────────────┬────────────────┘    │
│       │                                           │                     │
│       │  ┌────────────────────────────────────────┘                     │
│       │  │                                                              │
│       ▼  ▼                                                              │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │              InferenceAgent Trait                                 │   │
│  │                                                                  │   │
│  │  LOCAL (Restricted)              CLOUD (Open) ← NEW!             │   │
│  │  ┌──────────┐ ┌──────────┐      ┌──────────┐ ┌──────────┐       │   │
│  │  │ Ollama   │ │ LM       │      │ OpenAI   │ │Anthropic │       │   │
│  │  │ Agent    │ │ Studio   │      │ Agent    │ │ Agent    │       │   │
│  │  └──────────┘ └──────────┘      └──────────┘ └──────────┘       │   │
│  │  ┌──────────┐                   ┌──────────┐                     │   │
│  │  │ Generic  │                   │ Google   │                     │   │
│  │  │ (vLLM,   │                   │ Agent    │                     │   │
│  │  │  exo...) │                   │          │                     │   │
│  │  └──────────┘                   └──────────┘                     │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │  NEW: Nexus-Transparent Protocol                                 │   │
│  │                                                                  │   │
│  │  Every response gets X-Nexus-* headers:                          │   │
│  │  X-Nexus-Backend: openai-gpt4                                    │   │
│  │  X-Nexus-Backend-Type: cloud                                     │   │
│  │  X-Nexus-Route-Reason: capability-match                          │   │
│  │  X-Nexus-Privacy-Zone: open                                      │   │
│  │  X-Nexus-Cost-Estimated: 0.0042                                  │   │
│  │                                                                  │   │
│  │  Response JSON body is NEVER modified (Constitution Principle III)│   │
│  └──────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

### Key Design Decisions

| Decision | Why |
|----------|-----|
| Translation embedded in agents, not standalone translators | Anti-Abstraction Principle — no extra layer of indirection |
| Headers only, never modify JSON body | Constitution Principle III — strict OpenAI compatibility |
| API keys from env vars, never in config | Security — config files may be committed to version control |
| `PricingTable` with hardcoded rates | Simplicity — no runtime dependency on pricing APIs |
| `PrivacyZone::Open` for cloud backends | Foundation for F13 (Privacy Zones) enforcement |
| Heuristic token counting for non-OpenAI | Only OpenAI has a Rust tokenizer (tiktoken-rs); others use chars/4 |

### Request Lifecycle With Cloud Backend

Here's what happens when a chat request is routed to Anthropic:

```
┌──────────────────────────────────────────────────────────────────────────┐
│                  Request Lifecycle (Cloud Backend)                        │
│                                                                          │
│  Client                                                                  │
│    │                                                                     │
│    │  POST /v1/chat/completions                                          │
│    │  { "model": "claude-3-sonnet",                                      │
│    │    "messages": [                                                     │
│    │      {"role": "system", "content": "You are helpful"},              │
│    │      {"role": "user", "content": "Hello"}                           │
│    │    ] }                                                               │
│    ▼                                                                     │
│  ┌─────────────────────────────────────────────────────────────────────┐ │
│  │  api/completions.rs :: handle()                                     │ │
│  │                                                                     │ │
│  │  ① Router selects backend "anthropic-cloud" for "claude-3-sonnet"   │ │
│  │  │                                                                  │ │
│  │  ② registry.get_agent("anthropic-cloud")                            │ │
│  │  │  └─ Returns Arc<dyn InferenceAgent> (an AnthropicAgent)          │ │
│  │  │                                                                  │ │
│  │  ③ agent.chat_completion(request, headers)                          │ │
│  │  │  ├─ translate_request() → Anthropic format                       │ │
│  │  │  │  ├─ System messages extracted to `system` parameter           │ │
│  │  │  │  └─ max_tokens added (required by Anthropic)                  │ │
│  │  │  ├─ POST https://api.anthropic.com/v1/messages                   │ │
│  │  │  │  Headers: x-api-key: sk-ant-..., anthropic-version: ...       │ │
│  │  │  └─ translate_response() → OpenAI format                         │ │
│  │  │     ├─ stop_reason "end_turn" → finish_reason "stop"             │ │
│  │  │     └─ usage.input_tokens → usage.prompt_tokens                  │ │
│  │  │                                                                  │ │
│  │  ④ Estimate cost from response.usage + PricingTable                 │ │
│  │  │                                                                  │ │
│  │  ⑤ Inject X-Nexus-* headers into response                          │ │
│  │  │  └─ Headers only — JSON body untouched                           │ │
│  │  │                                                                  │ │
│  │  ⑥ Return OpenAI-compatible response to client                      │ │
│  └─────────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────┘
```

---

## File Structure

```
src/
├── agent/
│   ├── openai.rs              # MODIFIED: added tiktoken token counting
│   ├── anthropic.rs           # NEW: Anthropic Messages API agent (~807 lines)
│   ├── google.rs              # NEW: Google Generative AI agent (~854 lines)
│   ├── pricing.rs             # NEW: Cost estimation table (~218 lines)
│   ├── translation.rs         # MODIFIED: cleaned up to shared error types only
│   ├── factory.rs             # MODIFIED: added Anthropic + Google creation
│   └── mod.rs                 # MODIFIED: registered new modules
├── api/
│   ├── headers.rs             # NEW: X-Nexus-* transparent headers (~209 lines)
│   ├── error.rs               # NEW: Actionable 503 errors (~172 lines)
│   ├── completions.rs         # MODIFIED: cost estimation + header injection + 503 handling
│   └── mod.rs                 # MODIFIED: added PricingTable to AppState
├── config/
│   └── backend.rs             # MODIFIED: added zone + tier fields
├── registry/
│   └── backend.rs             # MODIFIED: added Anthropic + Google to BackendType

tests/
├── transparent_protocol_test.rs       # NEW: 14 tests for X-Nexus-* headers
├── openai_compatibility_contract.rs   # NEW: 3 contract tests for body preservation
├── actionable_errors_unit.rs          # NEW: 10 unit tests for error types
└── actionable_errors_integration.rs   # NEW: 5 integration tests for 503 responses
```

---

## File 1: registry/backend.rs - New Backend Types

Two new variants were added to the `BackendType` enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendType {
    Ollama,
    VLLM,
    LlamaCpp,
    Exo,
    OpenAI,
    LMStudio,
    Generic,
    Anthropic,    // ← NEW
    Google,       // ← NEW
}
```

**What's happening here:**

- `Anthropic` and `Google` are new variants that the router, health checker, and factory now recognize.
- `#[serde(rename_all = "lowercase")]` means they serialize as `"anthropic"` and `"google"` in JSON and TOML.

### default_privacy_zone() — Local vs Cloud Classification

```rust
impl BackendType {
    pub fn default_privacy_zone(&self) -> PrivacyZone {
        match self {
            // Local backends — data stays on your network
            BackendType::Ollama
            | BackendType::VLLM
            | BackendType::LlamaCpp
            | BackendType::Exo
            | BackendType::LMStudio
            | BackendType::Generic => PrivacyZone::Restricted,

            // Cloud backends — data leaves your network
            BackendType::OpenAI
            | BackendType::Anthropic
            | BackendType::Google => PrivacyZone::Open,
        }
    }
}
```

**Why this matters:** This is the foundation for F13 (Privacy Zones). When a user marks a request as "restricted", Nexus will filter out all cloud backends. But for now in F12, we just classify and report via headers.

---

## File 2: config/backend.rs - Cloud Configuration

Two new optional fields were added to `BackendConfig`:

```rust
pub struct BackendConfig {
    pub name: String,
    pub url: String,
    pub backend_type: BackendType,
    pub priority: i32,
    pub api_key_env: Option<String>,   // Existed before (for OpenAI)
    pub zone: Option<PrivacyZone>,     // ← NEW
    pub tier: Option<u8>,              // ← NEW
}
```

**What each field does:**

| Field | Type | Default | Purpose |
|-------|------|---------|---------|
| `api_key_env` | `Option<String>` | `None` | Name of environment variable holding the API key |
| `zone` | `Option<PrivacyZone>` | Backend's default | Override privacy zone (`"open"` or `"restricted"`) |
| `tier` | `Option<u8>` | `3` | Capability tier 1-5 (for future F13 routing) |

### Validation

```rust
impl BackendConfig {
    pub fn validate(&self) -> Result<(), String> {
        // Cloud backends MUST have api_key_env
        if matches!(
            self.backend_type,
            BackendType::OpenAI | BackendType::Anthropic | BackendType::Google
        ) && self.api_key_env.is_none()
        {
            return Err(format!(
                "Backend '{}' of type {:?} requires 'api_key_env' field",
                self.name, self.backend_type
            ));
        }

        // Tier must be 1-5
        if let Some(tier) = self.tier {
            if !(1..=5).contains(&tier) {
                return Err(format!(
                    "Backend '{}' has invalid tier {}, must be 1-5",
                    self.name, tier
                ));
            }
        }

        Ok(())
    }
}
```

**What `matches!(...)` does:** It's a macro that returns `true` if the value matches any of the listed patterns. It's cleaner than writing `if self.backend_type == A || self.backend_type == B || ...`.

### Example TOML Configuration

```toml
# Cloud backend with API key from environment
[[backends]]
name = "anthropic-cloud"
url = "https://api.anthropic.com"
type = "anthropic"
priority = 101
api_key_env = "ANTHROPIC_API_KEY"
zone = "open"
tier = 3
```

The key insight: `api_key_env = "ANTHROPIC_API_KEY"` says "read the API key from the `ANTHROPIC_API_KEY` environment variable". The actual key is **never** in the config file.

---

## File 3: agent/factory.rs - Creating Cloud Agents

The factory gained two new match arms for Anthropic and Google:

```rust
pub fn create_agent(
    id: String,
    name: String,
    url: String,
    backend_type: BackendType,
    client: Arc<Client>,
    metadata: HashMap<String, String>,
) -> Result<Arc<dyn InferenceAgent>, AgentError> {
    match backend_type {
        // ... existing: Ollama, OpenAI, LMStudio, Generic ...

        BackendType::Anthropic => {
            let api_key = extract_api_key(&metadata)?;   // From env var
            Ok(Arc::new(AnthropicAgent::new(
                id, name, url, api_key, client,
            )))
        }

        BackendType::Google => {
            let api_key = extract_api_key(&metadata)?;   // From env var
            Ok(Arc::new(GoogleAIAgent::new(
                id, name, url, api_key, client,
            )))
        }
    }
}
```

**What's happening here:**

- `extract_api_key()` checks `metadata["api_key"]` first, then looks up `metadata["api_key_env"]` as an environment variable name with `std::env::var()`. Missing key → `AgentError::Configuration`.
- Each agent gets a shared `Arc<Client>` — connection pooling across all agents.
- The return type `Arc<dyn InferenceAgent>` is a **trait object** — the caller doesn't know (or care) if it's an `AnthropicAgent` or `OllamaAgent`. It just calls `.chat_completion()`.

### BackendType → Agent Mapping (Updated)

| BackendType | Agent | Privacy Zone | Auth Method |
|-------------|-------|-------------|-------------|
| `Ollama` | `OllamaAgent` | Restricted | None |
| `OpenAI` | `OpenAIAgent` | Open | `Authorization: Bearer {key}` |
| `Anthropic` | `AnthropicAgent` | Open | `x-api-key: {key}` |
| `Google` | `GoogleAIAgent` | Open | `?key={key}` query parameter |
| `LMStudio` | `LMStudioAgent` | Restricted | None |
| `VLLM`, `LlamaCpp`, `Exo`, `Generic` | `GenericOpenAIAgent` | Restricted | None |

---

## File 4: agent/openai.rs - OpenAI Agent (Tiktoken)

The OpenAI agent existed before F12, but gained **exact token counting** using tiktoken-rs:

```rust
async fn count_tokens(&self, _model_id: &str, text: &str) -> TokenCount {
    use tiktoken_rs::o200k_base;

    match o200k_base() {
        Ok(bpe) => {
            let tokens = bpe.encode_ordinary(text);
            TokenCount::Exact(tokens.len() as u32)
        }
        Err(e) => {
            // Fall back to heuristic if tiktoken fails
            tracing::warn!("tiktoken encoding failed: {}, using heuristic", e);
            TokenCount::Heuristic((text.len() / 4) as u32)
        }
    }
}
```

**What's happening here:**

- `o200k_base()` loads OpenAI's BPE (Byte Pair Encoding) tokenizer. This is the same tokenizer OpenAI uses internally.
- `encode_ordinary(text)` splits the text into tokens. The length of the resulting vector is the exact token count.
- `TokenCount::Exact(n)` tells the caller "this is a precise count" — distinct from `TokenCount::Heuristic(n)` which says "this is an estimate."
- If the tokenizer fails to load (unlikely), we fall back to the chars/4 heuristic that all agents use by default.

**Why only OpenAI gets exact counting:** tiktoken-rs implements OpenAI's specific tokenizer. Anthropic and Google use different tokenizers that don't have Rust implementations yet, so they use the inherited default `TokenCount::Heuristic((text.len() / 4) as u32)` from the `InferenceAgent` trait.

---

## File 5: agent/anthropic.rs - Anthropic Agent

This is the most complex new file. It translates between OpenAI format and Anthropic's Messages API.

### The Struct

```rust
pub struct AnthropicAgent {
    id: String,
    name: String,
    base_url: String,           // "https://api.anthropic.com"
    api_key: String,            // From ANTHROPIC_API_KEY env var
    client: Arc<Client>,        // Shared connection pool
    pricing: Arc<PricingTable>, // For cost estimation
}
```

### Translation: The Core Challenge

OpenAI and Anthropic handle system messages differently:

```
OpenAI format:                          Anthropic format:
{                                       {
  "model": "claude-3-sonnet",            "model": "claude-3-sonnet",
  "messages": [                          "system": "You are helpful",  ← EXTRACTED
    {"role": "system",                   "messages": [
     "content": "You are helpful"},        {"role": "user",
    {"role": "user",                        "content": "Hello"}
     "content": "Hello"}                 ],
  ]                                      "max_tokens": 4096  ← REQUIRED
}                                       }
```

Here's how the translation works:

```rust
fn translate_request(&self, request: &ChatCompletionRequest) -> AnthropicRequest {
    // Step 1: Extract system messages into a separate parameter
    let system_messages: Vec<String> = request
        .messages
        .iter()
        .filter_map(|msg| {
            if msg.role == "system" {
                // Pull out the text content
                match &msg.content {
                    MessageContent::Text { content } => Some(content.clone()),
                    MessageContent::Parts { .. } => None,
                }
            } else {
                None // Skip user/assistant messages here
            }
        })
        .collect();

    // Combine multiple system messages into one string
    let system = if system_messages.is_empty() {
        None
    } else {
        Some(system_messages.join("\n"))
    };

    // Step 2: Filter OUT system messages, keep user/assistant
    let messages: Vec<AnthropicMessage> = request
        .messages
        .iter()
        .filter_map(|msg| {
            if msg.role == "system" {
                None  // Already extracted above
            } else {
                Some(AnthropicMessage {
                    role: msg.role.clone(),
                    content: /* extract text */,
                })
            }
        })
        .collect();

    AnthropicRequest {
        model: request.model.clone(),
        messages,
        max_tokens: request.max_tokens.unwrap_or(4096), // Anthropic REQUIRES this
        system,
        temperature: request.temperature,
        stream: Some(request.stream),
    }
}
```

**Key Rust concept — `filter_map`:** This iterator method combines `filter` and `map` in one step. If the closure returns `Some(value)`, the item is included. If it returns `None`, the item is skipped. It's more efficient and readable than `.filter(...).map(...)`.

### Response Translation

The response comes back in Anthropic's format and needs to be converted:

```rust
fn translate_response(&self, response: AnthropicResponse) -> ChatCompletionResponse {
    // Anthropic returns content as an array of "blocks"
    let text = response.content
        .iter()
        .filter_map(|block| {
            if block.r#type == "text" { block.text.clone() }
            else { None }
        })
        .collect::<Vec<String>>()
        .join("");

    // Map Anthropic's stop_reason to OpenAI's finish_reason
    let finish_reason = match response.stop_reason.as_deref() {
        Some("end_turn")      => "stop",     // Normal completion
        Some("max_tokens")    => "length",   // Hit token limit
        Some("stop_sequence") => "stop",     // Hit custom stop
        _                     => "stop",     // Default
    };

    ChatCompletionResponse {
        id: response.id,
        object: "chat.completion".to_string(),
        model: response.model,
        choices: vec![Choice {
            message: ChatMessage {
                role: "assistant".to_string(),
                content: MessageContent::Text { content: text },
                /* ... */
            },
            finish_reason: Some(finish_reason.to_string()),
            /* ... */
        }],
        // Note: different field names in usage!
        usage: Some(Usage {
            prompt_tokens: response.usage.input_tokens,       // ← different name
            completion_tokens: response.usage.output_tokens,  // ← different name
            total_tokens: response.usage.input_tokens + response.usage.output_tokens,
        }),
        /* ... */
    }
}
```

**What `as_deref()` does:** `response.stop_reason` is `Option<String>`. Calling `.as_deref()` converts `Option<String>` → `Option<&str>`, which lets us pattern-match against string literals like `"end_turn"`. Without it, we'd need to compare owned `String` values.

### Streaming Translation

Anthropic uses **Server-Sent Events (SSE)** but with different event types than OpenAI:

```
Anthropic SSE:                    OpenAI SSE (what Nexus returns):
event: message_start             data: {"choices":[{"delta":{"role":"assistant"}}]}
data: {"message":{"id":"..."}}

event: content_block_delta       data: {"choices":[{"delta":{"content":"Hello"}}]}
data: {"delta":{"text":"Hello"}}

event: message_delta             data: {"choices":[{"delta":{},"finish_reason":"stop"}]}
data: {"delta":{"stop_reason":"end_turn"}}

event: message_stop              data: [DONE]
```

The `translate_stream_chunk` method handles each event type:

```rust
fn translate_stream_chunk(&self, event: &str, data: &str) -> Option<String> {
    match event {
        "message_start" => {
            // First chunk: role announcement
            let chunk = json!({"choices": [{"delta": {"role": "assistant"}}]});
            Some(format!("data: {}\n\n", chunk))
        }
        "content_block_delta" => {
            // Content chunk: actual text
            let delta: ContentBlockDelta = serde_json::from_str(data)?;
            let chunk = json!({"choices": [{"delta": {"content": delta.delta.text}}]});
            Some(format!("data: {}\n\n", chunk))
        }
        "message_delta" => {
            // Final chunk: finish reason
            let chunk = json!({"choices": [{"finish_reason": "stop"}]});
            Some(format!("data: {}\n\n", chunk))
        }
        "message_stop" => {
            // Done sentinel
            Some("data: [DONE]\n\n".to_string())
        }
        _ => None, // Ignore unknown events
    }
}
```

### Authentication

```rust
// Anthropic uses a custom header, not Bearer token
let response = self.client
    .post(&url)
    .header("x-api-key", &self.api_key)         // ← Custom header
    .header("anthropic-version", "2023-06-01")   // ← API version pinning
    .header("content-type", "application/json")
    .json(&anthropic_request)
    .send()
    .await?;
```

---

## File 6: agent/google.rs - Google AI Agent

Similar structure to the Anthropic agent, but with Google's unique quirks.

### Translation: Two Key Differences

**1. Role Mapping: "assistant" ↔ "model"**

Google uses `"model"` where OpenAI uses `"assistant"`:

```rust
// OpenAI → Google
let role = if msg.role == "assistant" {
    "model".to_string()     // Google calls it "model"
} else {
    msg.role.clone()        // "user" stays "user"
};
```

**2. System Messages → `systemInstruction`**

Google puts system messages in a completely different structure:

```
OpenAI format:                          Google format:
{                                       {
  "messages": [                           "systemInstruction": {
    {"role": "system",                      "parts": [{"text": "Be helpful"}]
     "content": "Be helpful"},            },
    {"role": "user",                      "contents": [
     "content": "Hi"}                      {"role": "user",
  ]                                         "parts": [{"text": "Hi"}]}
}                                         ],
                                          "generationConfig": {
                                            "temperature": 0.7
                                          }
                                        }
```

```rust
fn translate_request(&self, request: &ChatCompletionRequest) -> GoogleRequest {
    // System messages → systemInstruction.parts
    let system_instruction = if system_messages.is_empty() {
        None
    } else {
        Some(GoogleSystemInstruction {
            parts: vec![GooglePart {
                text: system_messages.join("\n"),
            }],
        })
    };

    // Non-system messages → contents
    let contents: Vec<GoogleContent> = request.messages
        .iter()
        .filter_map(|msg| {
            if msg.role == "system" { return None; } // Already handled

            let role = if msg.role == "assistant" { "model" } else { &msg.role };
            Some(GoogleContent {
                role: role.to_string(),
                parts: vec![GooglePart { text: /* ... */ }],
            })
        })
        .collect();

    GoogleRequest {
        contents,
        system_instruction,
        generation_config: Some(/* temperature, max_output_tokens */),
    }
}
```

### Response Translation: finish_reason Mapping

```rust
let finish = match candidate.finish_reason.as_deref() {
    Some("STOP")       => "stop",            // Normal completion
    Some("MAX_TOKENS") => "length",          // Hit token limit
    Some("SAFETY")     => "content_filter",  // Safety filter triggered
    Some("RECITATION") => "content_filter",  // Copyright filter
    _                  => "stop",            // Default
};
```

### Authentication: Query Parameter

Unlike OpenAI (Bearer token) and Anthropic (custom header), Google uses a **query parameter**:

```rust
// Health check
let url = format!("{}/v1beta/models?key={}", self.base_url, self.api_key);

// Chat completion
let url = format!(
    "{}/v1beta/models/{}:generateContent?key={}",
    self.base_url, model, self.api_key
);

// Streaming
let url = format!(
    "{}/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
    self.base_url, model, self.api_key
);
```

### Comparison: Three Different Worlds

| Aspect | OpenAI | Anthropic | Google |
|--------|--------|-----------|--------|
| Auth | `Authorization: Bearer sk-...` | `x-api-key: sk-ant-...` | `?key=AIza...` |
| System messages | In messages array | Top-level `system` param | `systemInstruction.parts` |
| Assistant role | `"assistant"` | `"assistant"` | `"model"` |
| Streaming format | SSE with `data:` | SSE with `event:` + `data:` | Newline-delimited JSON |
| Stop reason | `"stop"` | `"end_turn"` | `"STOP"` |
| Token limit | `"length"` | `"max_tokens"` | `"MAX_TOKENS"` |
| Content structure | `message.content` | `content[].text` | `content.parts[].text` |

---

## File 7: agent/pricing.rs - Cost Estimation

This module provides per-request cost estimates for cloud backends.

### The Data Model

```rust
pub struct ModelPricing {
    pub input_price_per_1k: f64,    // USD per 1,000 input tokens
    pub output_price_per_1k: f64,   // USD per 1,000 output tokens
}

pub struct PricingTable {
    prices: Arc<HashMap<String, ModelPricing>>,
}
```

### Estimating Cost

```rust
pub fn estimate_cost(
    &self,
    model: &str,           // "gpt-4-turbo"
    input_tokens: u32,     // 1000
    output_tokens: u32,    // 500
) -> Option<f64> {
    self.prices.get(model).map(|pricing| {
        let input_cost = (input_tokens as f64 / 1000.0) * pricing.input_price_per_1k;
        let output_cost = (output_tokens as f64 / 1000.0) * pricing.output_price_per_1k;
        input_cost + output_cost
    })
}
```

**What `.map()` does on `Option`:** If the model is in the pricing table, we compute and return `Some(cost)`. If not (e.g., a local Ollama model), we return `None`. No cost header gets added for local backends.

### Sample Prices (Hardcoded)

| Model | Input ($/1K) | Output ($/1K) |
|-------|-------------|---------------|
| `gpt-4-turbo` | 0.01 | 0.03 |
| `gpt-3.5-turbo` | 0.0005 | 0.0015 |
| `claude-3-opus-20240229` | 0.015 | 0.075 |
| `claude-3-haiku-20240307` | 0.00025 | 0.00125 |
| `gemini-1.5-pro` | 0.0035 | 0.0105 |
| `gemini-1.5-flash` | 0.00035 | 0.00105 |

**Maintenance note:** These are hardcoded and must be manually updated. A comment in the source links to each provider's pricing page.

---

## File 8: api/headers.rs - The Transparent Protocol

This is the heart of the Nexus-Transparent Protocol — metadata about routing decisions exposed through HTTP headers without touching the response body.

### RouteReason — Why This Backend?

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouteReason {
    CapabilityMatch,      // "capability-match"    — Backend has the model/features
    CapacityOverflow,     // "capacity-overflow"   — Primary full, overflowed here
    PrivacyRequirement,   // "privacy-requirement" — Privacy zone filtered options
    Failover,             // "failover"            — Previous backend failed
}
```

**What `#[serde(rename_all = "kebab-case")]` does:** When serialized, `CapabilityMatch` becomes `"capability-match"`. This is a Serde attribute that automatically applies kebab-case naming.

### Header Constants

```rust
pub const HEADER_BACKEND: &str = "x-nexus-backend";
pub const HEADER_BACKEND_TYPE: &str = "x-nexus-backend-type";
pub const HEADER_ROUTE_REASON: &str = "x-nexus-route-reason";
pub const HEADER_PRIVACY_ZONE: &str = "x-nexus-privacy-zone";
pub const HEADER_COST_ESTIMATED: &str = "x-nexus-cost-estimated";
```

Lowercase `x-nexus-*` for HTTP/2 compatibility (HTTP/2 requires lowercase header names).

### Injecting Headers Into a Response

```rust
pub fn inject_into_response<B>(&self, response: &mut Response<B>) {
    let headers = response.headers_mut();

    // X-Nexus-Backend: "openai-gpt4"
    headers.insert(
        HeaderName::from_static(HEADER_BACKEND),
        HeaderValue::from_str(&self.backend)
            .expect("backend name should be valid ASCII"),
    );

    // X-Nexus-Backend-Type: "local" or "cloud"
    let backend_type_str = match self.backend_type {
        BackendType::Ollama | BackendType::VLLM | BackendType::LlamaCpp
        | BackendType::Exo | BackendType::LMStudio | BackendType::Generic => "local",
        BackendType::OpenAI | BackendType::Anthropic | BackendType::Google => "cloud",
    };
    headers.insert(
        HeaderName::from_static(HEADER_BACKEND_TYPE),
        HeaderValue::from_static(backend_type_str),
    );

    // X-Nexus-Route-Reason: "capability-match"
    headers.insert(
        HeaderName::from_static(HEADER_ROUTE_REASON),
        HeaderValue::from_static(self.route_reason.as_str()),
    );

    // X-Nexus-Privacy-Zone: "restricted" or "open"
    let privacy_zone_str = match self.privacy_zone {
        PrivacyZone::Restricted => "restricted",
        PrivacyZone::Open => "open",
    };
    headers.insert(
        HeaderName::from_static(HEADER_PRIVACY_ZONE),
        HeaderValue::from_static(privacy_zone_str),
    );

    // X-Nexus-Cost-Estimated: "0.0042" (only for cloud backends)
    if let Some(cost) = self.cost_estimated {
        headers.insert(
            HeaderName::from_static(HEADER_COST_ESTIMATED),
            HeaderValue::from_str(&format!("{:.4}", cost))
                .expect("cost should format to valid ASCII"),
        );
    }
}
```

**What `<B>` means:** This is a **generic type parameter**. `Response<B>` is Axum's HTTP response type where `B` is the body type. By using a generic, this method works with any body type — JSON responses, streaming SSE responses, error responses, etc. The caller doesn't need to convert their response before adding headers.

**What `from_static` vs `from_str` means:** `from_static` takes a `&'static str` (a compile-time string literal) — zero allocation. `from_str` takes a runtime string and may allocate. We use `from_static` for known constants and `from_str` for dynamic values like the backend name.

---

## File 9: api/error.rs - Actionable 503 Errors

When Nexus can't find a healthy backend, it doesn't just say "503 Service Unavailable". It tells you **why** and **what you can do about it**.

### The Context Object

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionableErrorContext {
    /// Tier required for the requested model (1-5)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_tier: Option<u8>,

    /// Names of backends currently available (may be empty)
    pub available_backends: Vec<String>,

    /// Estimated time to recovery in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_seconds: Option<u64>,

    /// Privacy zone constraint that couldn't be met
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacy_zone_required: Option<String>,
}
```

**What `#[serde(skip_serializing_if = "Option::is_none")]` does:** If the field is `None`, it's omitted entirely from the JSON output. So a response might look like:

```json
{
  "error": {
    "message": "No healthy backend available for model 'gpt-4'",
    "type": "service_unavailable",
    "code": "service_unavailable"
  },
  "context": {
    "available_backends": ["ollama-local"],
    "required_tier": 5
  }
}
```

Notice `eta_seconds` and `privacy_zone_required` are absent (not `null`) — cleaner JSON.

### Convenience Constructors

```rust
impl ServiceUnavailableError {
    // Generic 503
    pub fn new(message: String, context: ActionableErrorContext) -> Self { /* ... */ }

    // "No tier-5 backend available"
    pub fn tier_unavailable(required_tier: u8, available: Vec<String>) -> Self { /* ... */ }

    // "No backend in the 'restricted' zone"
    pub fn privacy_unavailable(zone: &str, available: Vec<String>) -> Self { /* ... */ }

    // "Everything is down"
    pub fn all_backends_down() -> Self { /* ... */ }
}
```

### IntoResponse — Making It an HTTP Response

```rust
impl axum::response::IntoResponse for ServiceUnavailableError {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::SERVICE_UNAVAILABLE, Json(self)).into_response()
    }
}
```

**What `IntoResponse` is:** This is Axum's trait for "things that can become HTTP responses." By implementing it, we can `return Ok(error.into_response())` directly from a handler — Axum handles the HTTP 503 status code and JSON serialization for us.

---

## File 10: api/completions.rs - Wiring It All Together

This is the main request handler. Three pieces were added for F12:

### 1. Cost Estimation (after receiving response)

```rust
// After the agent returns a response with usage data...
let cost_estimated = response.usage.as_ref().and_then(|u| {
    state
        .pricing
        .estimate_cost(&actual_model, u.prompt_tokens, u.completion_tokens)
});
```

**What `.and_then()` does:** It's like `.map()` but the inner function also returns an `Option`. Here: if `usage` exists AND the model has pricing data → `Some(cost)`. Otherwise → `None`.

**Why compute cost here, not in the agent?** Because we need the response's actual token counts. The agent returns the response; the handler has the response + pricing table → computes cost → injects header. This avoids coupling agents to the pricing module.

### 2. Header Injection (both streaming and non-streaming)

```rust
// Build the NexusTransparentHeaders struct
let nexus_headers = NexusTransparentHeaders::new(
    backend.id.clone(),       // "anthropic-cloud"
    backend.backend_type,     // BackendType::Anthropic
    route_reason,             // RouteReason::CapabilityMatch
    privacy_zone,             // PrivacyZone::Open
    cost_estimated,           // Some(0.0042) or None
);

// Inject into the already-built response (headers only, body untouched!)
nexus_headers.inject_into_response(&mut resp);
```

This happens in **two places** — once for non-streaming responses (line ~330) and once for streaming responses (line ~600). Same `NexusTransparentHeaders` struct, same `inject_into_response` method. One code path, zero inconsistency.

### 3. Actionable 503 Handling (no healthy backend)

```rust
crate::routing::RoutingError::NoHealthyBackend { model } => {
    // Collect available backend names for the context
    let available_backends = available_backend_names(&state);

    // Build actionable context
    let context = ActionableErrorContext {
        required_tier: None,
        available_backends,
        eta_seconds: None,
        privacy_zone_required: None,
    };

    // Log with structured fields for observability
    warn!(
        model = %model,
        available_backends = ?context.available_backends,
        "Routing failure: no healthy backend"
    );

    // Return 503 with context (not a generic error!)
    let error = ServiceUnavailableError::new(
        format!("No healthy backend available for model '{}'", model),
        context,
    );
    return Ok(error.into_response());
}
```

**Why `return Ok(error.into_response())` instead of `return Err(...)`?** The handler returns `Result<Response, ApiError>`. A 503 is a **valid response** (we're sending it on purpose), not an unexpected error. Returning `Ok(503_response)` gives us full control over the response shape. Returning `Err(...)` would go through the generic error handler, which might format it differently.

---

## Understanding the Tests

### Test Distribution

| Category | Count | Location |
|----------|-------|----------|
| Transparent protocol headers | 14 | `tests/transparent_protocol_test.rs` |
| OpenAI compatibility contract | 3 | `tests/openai_compatibility_contract.rs` |
| Actionable errors (unit) | 10 | `tests/actionable_errors_unit.rs` |
| Actionable errors (integration) | 5 | `tests/actionable_errors_integration.rs` |
| Agent unit tests | ~50 | `src/agent/{openai,anthropic,google,pricing}.rs` |
| Headers unit tests | ~15 | `src/api/headers.rs` |
| Error unit tests | ~10 | `src/api/error.rs` |
| **Total F12-related** | **~107** | |

### Contract Tests — The Most Important Tests

The contract tests verify the **one thing that must never break**: the response JSON body is identical to what the backend sent.

```rust
#[tokio::test]
async fn test_openai_response_body_unchanged() {
    // Set up mock backend that returns a known JSON response
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "model": "gpt-4",
            "choices": [{ "message": { "content": "Hello!" }, "finish_reason": "stop" }]
        })))
        .mount(&mock_server)
        .await;

    // Send request through Nexus
    let response = app.call(request).await.unwrap();

    // The body MUST be byte-identical to what the mock returned
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["id"], "chatcmpl-123");
    assert_eq!(body["choices"][0]["message"]["content"], "Hello!");

    // But X-Nexus-* headers ARE present
    assert!(response.headers().contains_key("x-nexus-backend"));
    assert!(response.headers().contains_key("x-nexus-backend-type"));
}
```

**Why this test matters:** Constitution Principle III says "metadata in X-Nexus-* headers only, never modify response JSON body." If this test fails, we've broken API compatibility for every client.

### Integration Tests — 503 With Context

```rust
#[tokio::test]
async fn test_503_with_required_tier() {
    // Mock returns 503 with service_unavailable code
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({
            "error": {
                "message": "Service unavailable",
                "type": "service_unavailable",
                "code": "service_unavailable"
            }
        })))
        .mount(&mock_server)
        .await;

    let response = app.call(request).await.unwrap();

    // Verify we get a 503 (not 500, not 502)
    assert_eq!(response.status(), 503);

    // Verify the context object is present
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["context"].is_object());
    assert!(body["context"]["available_backends"].is_array());
}
```

### Unit Tests — Serialization Round-Trip

```rust
#[test]
fn test_actionable_error_serialization() {
    let context = ActionableErrorContext {
        required_tier: Some(5),
        available_backends: vec!["ollama-local".to_string()],
        eta_seconds: None,
        privacy_zone_required: None,
    };

    let json = serde_json::to_string(&context).unwrap();
    let deserialized: ActionableErrorContext = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.required_tier, Some(5));
    assert_eq!(deserialized.available_backends, vec!["ollama-local"]);

    // None fields should be absent (not null)
    assert!(!json.contains("eta_seconds"));
}
```

### Header Tests — Every Value Is Checked

```rust
#[tokio::test]
async fn test_all_five_headers_present() {
    // ... set up mock backend returning 200 ...

    let response = app.call(request).await.unwrap();

    // All 5 X-Nexus-* headers must be present
    assert!(response.headers().contains_key("x-nexus-backend"));
    assert!(response.headers().contains_key("x-nexus-backend-type"));
    assert!(response.headers().contains_key("x-nexus-route-reason"));
    assert!(response.headers().contains_key("x-nexus-privacy-zone"));
    // cost-estimated is optional (only for cloud backends with pricing)
}

#[tokio::test]
async fn test_backend_type_local_vs_cloud() {
    // Local backend → "local"
    // Cloud backend → "cloud"
    let type_value = response.headers()
        .get("x-nexus-backend-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(type_value == "local" || type_value == "cloud");
}
```

---

## Key Rust Concepts

| Concept | What It Means | Example in This Code |
|---------|---------------|----------------------|
| `filter_map` | Filter + map in one step (`None` = skip, `Some(x)` = keep) | System message extraction in `translate_request()` |
| `as_deref()` | Convert `Option<String>` → `Option<&str>` for pattern matching | `response.stop_reason.as_deref()` in Anthropic/Google agents |
| `format!("{:.4}", cost)` | Format float with 4 decimal places | Cost header value `"0.0042"` |
| `and_then()` | Chain `Option`-returning operations (flatMap) | `usage.as_ref().and_then(\|u\| pricing.estimate_cost(...))` |
| `<B>` generic parameter | Function works with any body type | `inject_into_response<B>(response: &mut Response<B>)` |
| `from_static` vs `from_str` | Compile-time string (zero alloc) vs runtime string | Header constants vs backend name |
| `skip_serializing_if` | Omit field from JSON if condition is true | `None` fields absent from error context |
| `IntoResponse` trait | Make custom types directly returnable from Axum handlers | `ServiceUnavailableError::into_response()` |
| `Arc<HashMap>` | Shared immutable data across threads | `PricingTable.prices` — read-only after init |
| `#[serde(rename_all = "kebab-case")]` | Auto-rename variants for serialization | `CapabilityMatch` → `"capability-match"` |

### Why `Ok(error.into_response())` Instead of `Err(...)`?

This confused me at first. In Axum, handlers return `Result<Response, Error>`:

```rust
// This is an EXPECTED outcome (we chose to return 503):
return Ok(error.into_response());  // ← We control the exact response

// This is an UNEXPECTED error (something crashed):
return Err(ApiError::internal());  // ← Goes through generic error handler
```

The difference: `Ok(503)` means "everything worked, but there's no backend available — here's a helpful response." `Err(...)` means "something went wrong internally."

### Understanding `&mut Response<B>` (Header Injection)

```rust
// Step 1: Build the response body (JSON)
let mut resp = Json(response).into_response();

// Step 2: Modify the response in-place (add headers)
nexus_headers.inject_into_response(&mut resp);

// Step 3: Return the modified response
Ok(resp)
```

This is the **builder pattern without a builder**. We create the response, then mutate it to add headers. The `&mut` means we're borrowing the response mutably — we can change it, but only one piece of code can do so at a time. This is Rust's way of preventing data races.

---

## Common Patterns in This Codebase

### Pattern 1: Translate-In-Agent (Not Standalone Translators)

```rust
// Translation lives IN the agent, not in a separate translator module
impl AnthropicAgent {
    fn translate_request(&self, req: &ChatCompletionRequest) -> AnthropicRequest { ... }
    fn translate_response(&self, resp: AnthropicResponse) -> ChatCompletionResponse { ... }
    fn translate_stream_chunk(&self, event: &str, data: &str) -> Option<String> { ... }
}
```

We considered having standalone `AnthropicTranslator` and `GoogleTranslator` structs, but that violates the Anti-Abstraction Principle. Translation is an implementation detail of the agent — no other code needs it.

### Pattern 2: Compute-Then-Inject (Response Headers)

```rust
// Step 1: Get the response from the agent
let response = agent.chat_completion(request, headers).await?;

// Step 2: Compute cost from response data
let cost = pricing.estimate_cost(&model, usage.prompt_tokens, usage.completion_tokens);

// Step 3: Build the HTTP response (JSON body)
let mut resp = Json(response).into_response();

// Step 4: Inject headers AFTER body is built
nexus_headers.inject_into_response(&mut resp);
```

This ordering is critical: the JSON body is built first (step 3), then headers are added on top (step 4). This guarantees we **never** modify the JSON body. If we mixed steps 3 and 4, we might accidentally include header data in the body.

### Pattern 3: Option Chaining for Conditional Values

```rust
// Cost is only present when: usage exists AND model has pricing data
let cost_estimated = response.usage
    .as_ref()                              // Option<&Usage>
    .and_then(|u| {                        // → Option<f64>
        state.pricing.estimate_cost(
            &actual_model,
            u.prompt_tokens,
            u.completion_tokens,
        )
    });
// cost_estimated: Some(0.0042) for cloud, None for local
```

This is Rust's alternative to nested `if` statements:
```rust
// The imperative equivalent (less idiomatic):
let cost_estimated = if let Some(usage) = &response.usage {
    state.pricing.estimate_cost(&actual_model, usage.prompt_tokens, usage.completion_tokens)
} else {
    None
};
```

### Pattern 4: Match-All Privacy/Type Classification

```rust
// Every BackendType MUST be handled (compiler enforces exhaustive match)
let backend_type_str = match self.backend_type {
    BackendType::Ollama | BackendType::VLLM | BackendType::LlamaCpp
    | BackendType::Exo | BackendType::LMStudio | BackendType::Generic => "local",
    BackendType::OpenAI | BackendType::Anthropic | BackendType::Google => "cloud",
};
```

If someone adds a new `BackendType` variant in the future (say `BackendType::Azure`), the compiler will error on every `match` that doesn't handle it. This prevents bugs from forgotten cases.

### Pattern 5: Convenience Constructors for Error Types

```rust
// Instead of building the error struct manually every time...
let error = ServiceUnavailableError {
    error: ApiErrorBody {
        message: "...".to_string(),
        r#type: "service_unavailable".to_string(),
        code: Some("service_unavailable".to_string()),
        param: None,
    },
    context: ActionableErrorContext {
        required_tier: Some(5),
        available_backends: vec!["ollama".into()],
        eta_seconds: None,
        privacy_zone_required: None,
    },
};

// ...use a named constructor that encapsulates the details:
let error = ServiceUnavailableError::tier_unavailable(5, vec!["ollama".into()]);
```

---

## Next Steps

Now that you understand Cloud Backend Support, explore:

1. **NII Agent Abstraction** (`specs/012-nii-extraction/walkthrough.md`) — The InferenceAgent trait these cloud agents implement
2. **Intelligent Router** (`src/routing/`) — How backends are scored and selected before an agent is invoked
3. **Health Checker** (`src/health/mod.rs`) — Background loop that calls `agent.health_check()` for all backends including cloud

### Try It Yourself

1. Look at the agent factory to see how all backend types are created:
   ```bash
   cargo test agent::factory -- --nocapture
   ```

2. Run the transparent protocol tests:
   ```bash
   cargo test --test transparent_protocol_test -- --nocapture
   ```

3. Run the actionable error tests:
   ```bash
   cargo test --test actionable_errors_integration -- --nocapture
   ```

4. Read the Anthropic agent top to bottom — it's the best example of full API translation:
   ```bash
   cat src/agent/anthropic.rs
   ```

5. See the cost estimation in action:
   ```bash
   cargo test agent::pricing -- --nocapture
   ```

6. Search for all X-Nexus header injection points:
   ```bash
   grep -n "inject_into_response" src/api/completions.rs
   ```
