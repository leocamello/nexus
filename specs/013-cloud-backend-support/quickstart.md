# Quickstart: Cloud Backend Support

**Feature**: F12 - Cloud Backend Support with Nexus-Transparent Protocol  
**Audience**: Developers implementing and testing cloud backend integration  
**Prerequisites**: Nexus v0.3+, API keys for cloud providers

This guide walks you through registering cloud backends, testing routing, and interpreting X-Nexus-* headers.

---

## Table of Contents

1. [Configuration](#configuration)
2. [Environment Setup](#environment-setup)
3. [Testing with cURL](#testing-with-curl)
4. [Interpreting Headers](#interpreting-headers)
5. [Format Translation](#format-translation)
6. [Adding New Providers](#adding-new-providers)
7. [Troubleshooting](#troubleshooting)

---

## Configuration

### 1. Register Cloud Backend in nexus.toml

Add cloud backends to the `[[backends]]` array:

```toml
# OpenAI Backend (Tier 5 - highest capability)
[[backends]]
name = "openai-gpt4"
url = "https://api.openai.com/v1"
type = "openai"
api_key_env = "OPENAI_API_KEY"
zone = "open"
tier = 5
priority = 100

# Anthropic Backend (Tier 4)
[[backends]]
name = "anthropic-claude"
url = "https://api.anthropic.com"
type = "anthropic"
api_key_env = "ANTHROPIC_API_KEY"
zone = "open"
tier = 4
priority = 90

# Google AI Backend (Tier 4)
[[backends]]
name = "google-gemini"
url = "https://generativelanguage.googleapis.com"
type = "google"
api_key_env = "GOOGLE_API_KEY"
zone = "open"
tier = 4
priority = 85

# Local backend for comparison (Tier 2)
[[backends]]
name = "ollama-local"
url = "http://localhost:11434"
type = "ollama"
zone = "restricted"
tier = 2
priority = 50
```

### Configuration Fields Explained

| Field | Required | Description | Example |
|-------|----------|-------------|---------|
| `name` | Yes | Unique backend identifier | `"openai-gpt4"` |
| `url` | Yes | API base URL (HTTPS for cloud) | `"https://api.openai.com/v1"` |
| `type` | Yes | Backend type enum | `"openai"` \| `"anthropic"` \| `"google"` |
| `api_key_env` | Yes* | Environment variable containing API key | `"OPENAI_API_KEY"` |
| `zone` | No | Privacy zone (defaults based on type) | `"open"` \| `"restricted"` |
| `tier` | No | Capability tier 1-5 (default: 3) | `5` (highest) |
| `priority` | No | Selection priority (default: 50) | `100` (prefer this backend) |

\* Required for cloud backends (openai, anthropic, google)

---

## Environment Setup

### 2. Set API Keys

Cloud backends require API keys set via environment variables:

```bash
# OpenAI
export OPENAI_API_KEY="sk-proj-..."

# Anthropic
export ANTHROPIC_API_KEY="sk-ant-..."

# Google AI
export GOOGLE_API_KEY="AIza..."
```

**Security Note**: NEVER commit API keys to config files. Always use environment variables.

### 3. Verify Configuration

Start Nexus and check backend registration:

```bash
# Start Nexus
cargo run -- serve

# In another terminal, list backends
curl http://localhost:3000/v1/models | jq '.data[] | {id, backend}'
```

Expected output:
```json
[
  {"id": "gpt-4", "backend": "openai-gpt4"},
  {"id": "claude-3-opus", "backend": "anthropic-claude"},
  {"id": "gemini-1.5-pro", "backend": "google-gemini"},
  {"id": "llama2", "backend": "ollama-local"}
]
```

### 4. Health Check

Verify backends are healthy:

```bash
curl http://localhost:3000/v1/backends | jq '.backends[] | {name, status, type}'
```

Expected output:
```json
[
  {"name": "openai-gpt4", "status": "healthy", "type": "openai"},
  {"name": "anthropic-claude", "status": "healthy", "type": "anthropic"},
  {"name": "google-gemini", "status": "healthy", "type": "google"}
]
```

If status is `"unhealthy"`, check:
- API key environment variable is set
- API key is valid and not expired
- Network connectivity to cloud provider

---

## Testing with cURL

### 5. Basic Request (Non-Streaming)

Test OpenAI backend:

```bash
curl -i http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4",
    "messages": [
      {"role": "user", "content": "What is Nexus?"}
    ]
  }'
```

**Verify**:
- HTTP 200 status
- JSON response body matches OpenAI format
- Headers include: `X-Nexus-Backend`, `X-Nexus-Backend-Type`, `X-Nexus-Route-Reason`, `X-Nexus-Privacy-Zone`, `X-Nexus-Cost-Estimated`

### 6. Streaming Request

Test streaming with Anthropic:

```bash
curl -N http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-opus",
    "messages": [{"role": "user", "content": "Count to 5"}],
    "stream": true
  }'
```

**Verify**:
- Headers appear before first `data:` line
- SSE format matches OpenAI (`data: {...}\n\n`)
- Final chunk is `data: [DONE]`

### 7. Privacy Zone Filtering

Request with privacy requirement:

```bash
curl -i http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "X-Privacy-Zone: restricted" \
  -d '{
    "model": "llama2",
    "messages": [{"role": "user", "content": "Sensitive data"}]
  }'
```

**Expected**:
- Routed to local backend (Ollama)
- `X-Nexus-Backend-Type: local`
- `X-Nexus-Privacy-Zone: restricted`
- `X-Nexus-Cost-Estimated` header NOT present (local is free)

### 8. Fallback Chain

Simulate local backend failure:

```bash
# Stop Ollama
systemctl stop ollama  # or docker stop ollama

# Request model available on both local and cloud
curl -i http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama2",  # Available locally and as cloud fallback
    "messages": [{"role": "user", "content": "Test"}]
  }'
```

**Expected**:
- `X-Nexus-Route-Reason: failover` (failed over to cloud)
- `X-Nexus-Backend-Type: cloud`

---

## Interpreting Headers

### 9. Header Analysis

Extract and analyze headers from response:

```bash
curl -i http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4",
    "messages": [{"role": "user", "content": "Hi"}]
  }' | grep -i x-nexus
```

Example output:
```
X-Nexus-Backend: openai-gpt4
X-Nexus-Backend-Type: cloud
X-Nexus-Route-Reason: capability-match
X-Nexus-Privacy-Zone: open
X-Nexus-Cost-Estimated: 0.0042
```

### Interpretation Guide

| Header | Value | Meaning |
|--------|-------|---------|
| `X-Nexus-Backend` | `openai-gpt4` | Request handled by OpenAI backend named "openai-gpt4" |
| `X-Nexus-Backend-Type` | `cloud` | Backend is a cloud service (privacy implications) |
| `X-Nexus-Route-Reason` | `capability-match` | Backend selected because it supports the requested model |
| `X-Nexus-Privacy-Zone` | `open` | Data was processed by third-party service (OpenAI) |
| `X-Nexus-Cost-Estimated` | `0.0042` | Estimated cost: $0.0042 USD (4.2 cents per 1000 requests) |

### 10. Cost Monitoring

Track costs across requests:

```bash
# Python script to accumulate costs
import requests
import json

total_cost = 0.0
requests_count = 0

for i in range(100):
    response = requests.post(
        "http://localhost:3000/v1/chat/completions",
        json={
            "model": "gpt-4",
            "messages": [{"role": "user", "content": f"Test {i}"}]
        }
    )
    
    if 'X-Nexus-Cost-Estimated' in response.headers:
        cost = float(response.headers['X-Nexus-Cost-Estimated'])
        total_cost += cost
        requests_count += 1

print(f"Total cost: ${total_cost:.4f} over {requests_count} requests")
print(f"Average cost per request: ${total_cost/requests_count:.4f}")
```

---

## Format Translation

### 11. Anthropic Translation (Internal)

**OpenAI Request**:
```json
{
  "model": "gpt-4",
  "messages": [
    {"role": "system", "content": "You are helpful"},
    {"role": "user", "content": "Hi"}
  ]
}
```

**Translated to Anthropic** (internal):
```json
{
  "model": "claude-3-opus-20240229",
  "system": "You are helpful",
  "messages": [
    {"role": "user", "content": "Hi"}
  ],
  "max_tokens": 4096
}
```

**Anthropic Response** (internal):
```json
{
  "id": "msg_123",
  "type": "message",
  "role": "assistant",
  "content": [{"type": "text", "text": "Hello!"}],
  "stop_reason": "end_turn",
  "usage": {"input_tokens": 10, "output_tokens": 5}
}
```

**Translated to OpenAI** (returned to client):
```json
{
  "id": "msg_123",
  "object": "chat.completion",
  "created": 1677652288,
  "model": "claude-3-opus-20240229",
  "choices": [{
    "index": 0,
    "message": {"role": "assistant", "content": "Hello!"},
    "finish_reason": "stop"
  }],
  "usage": {
    "prompt_tokens": 10,
    "completion_tokens": 5,
    "total_tokens": 15
  }
}
```

### 12. Google Translation (Internal)

**OpenAI Request**:
```json
{
  "model": "gemini-1.5-pro",
  "messages": [
    {"role": "system", "content": "Be concise"},
    {"role": "user", "content": "What is AI?"}
  ]
}
```

**Translated to Google** (internal):
```json
{
  "contents": [{
    "role": "user",
    "parts": [{"text": "System: Be concise\n\nUser: What is AI?"}]
  }],
  "generationConfig": {
    "maxOutputTokens": 2048
  }
}
```

**Google Response** (internal):
```json
{
  "candidates": [{
    "content": {
      "parts": [{"text": "AI is artificial intelligence."}],
      "role": "model"
    },
    "finishReason": "STOP"
  }],
  "usageMetadata": {
    "promptTokenCount": 12,
    "candidatesTokenCount": 8,
    "totalTokenCount": 20
  }
}
```

**Translated to OpenAI** (returned to client):
```json
{
  "id": "google-<uuid>",
  "object": "chat.completion",
  "created": 1677652288,
  "model": "gemini-1.5-pro",
  "choices": [{
    "index": 0,
    "message": {"role": "assistant", "content": "AI is artificial intelligence."},
    "finish_reason": "stop"
  }],
  "usage": {
    "prompt_tokens": 12,
    "completion_tokens": 8,
    "total_tokens": 20
  }
}
```

**Key Point**: All translation happens transparently. Clients always see OpenAI format.

---

## Adding New Providers

### 13. Add a New Cloud Provider

To add support for a new cloud provider (e.g., Cohere, AI21):

**Step 1: Extend BackendType enum**
```rust
// src/registry/backend.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendType {
    // ... existing types
    Cohere,  // NEW
}
```

**Step 2: Create Agent Implementation**
```rust
// src/agent/cohere.rs
pub struct CohereAgent {
    id: String,
    name: String,
    base_url: String,
    api_key: String,
    http_client: reqwest::Client,
    translator: CohereTranslator,
    pricing: Arc<PricingTable>,
}

impl InferenceAgent for CohereAgent {
    // Implement all trait methods
    // See src/agent/anthropic.rs as reference
}
```

**Step 3: Create Translator**
```rust
// src/agent/translation.rs
pub struct CohereTranslator;

impl CohereTranslator {
    pub fn openai_to_cohere(&self, req: ChatCompletionRequest) -> Result<CohereRequest, TranslationError> {
        // Convert OpenAI format to Cohere chat format
    }
    
    pub fn cohere_to_openai(&self, resp: CohereResponse) -> Result<ChatCompletionResponse, TranslationError> {
        // Convert Cohere response to OpenAI format
    }
}
```

**Step 4: Update Factory**
```rust
// src/agent/factory.rs
pub fn create_agent(backend: &BackendConfig) -> Result<Arc<dyn InferenceAgent>, AgentError> {
    match backend.backend_type {
        // ... existing cases
        BackendType::Cohere => {
            let api_key = read_api_key(&backend.api_key_env)?;
            Ok(Arc::new(CohereAgent::new(
                backend.name.clone(),
                backend.url.clone(),
                api_key,
            )))
        }
    }
}
```

**Step 5: Add Pricing**
```rust
// src/agent/pricing.rs
impl PricingTable {
    pub fn new() -> Self {
        // ... existing prices
        prices.insert("command-r-plus".to_string(), ModelPricing {
            input_price_per_1k: 0.003,
            output_price_per_1k: 0.015,
        });
    }
}
```

**Step 6: Add Tests**
```rust
// tests/integration/cohere_test.rs
#[tokio::test]
async fn test_cohere_translation() {
    // Test request/response translation
    // Test streaming
    // Test error handling
}
```

---

## Troubleshooting

### Common Issues

#### Issue: Backend shows as "unhealthy"

**Symptoms**:
```bash
$ curl http://localhost:3000/v1/backends
{
  "backends": [
    {"name": "openai-gpt4", "status": "unhealthy"}
  ]
}
```

**Solutions**:
1. **Check API key environment variable**:
   ```bash
   echo $OPENAI_API_KEY  # Should output key
   ```
   If empty, set it and restart Nexus.

2. **Verify API key validity**:
   ```bash
   curl https://api.openai.com/v1/models \
     -H "Authorization: Bearer $OPENAI_API_KEY"
   ```
   If 401 error, API key is invalid/expired.

3. **Check network connectivity**:
   ```bash
   ping api.openai.com
   ```

4. **Check Nexus logs**:
   ```bash
   tail -f nexus.log | grep -i health
   ```

---

#### Issue: "Model not found" error

**Symptoms**:
```bash
$ curl -X POST http://localhost:3000/v1/chat/completions -d '{"model": "gpt-5"}'
{
  "error": {
    "message": "Model 'gpt-5' not found on any backend",
    "type": "model_not_found"
  }
}
```

**Solutions**:
1. **List available models**:
   ```bash
   curl http://localhost:3000/v1/models | jq '.data[].id'
   ```

2. **Check model name spelling** (case-sensitive)

3. **Verify backend supports model**:
   - OpenAI: gpt-4, gpt-3.5-turbo
   - Anthropic: claude-3-opus, claude-3-sonnet
   - Google: gemini-1.5-pro, gemini-1.5-flash

---

#### Issue: Headers missing from response

**Symptoms**:
Response doesn't include X-Nexus-* headers.

**Solutions**:
1. **Check Nexus version**: Headers added in v0.3+
   ```bash
   nexus --version
   ```

2. **Use `-i` flag with cURL** to see headers:
   ```bash
   curl -i http://localhost:3000/v1/chat/completions ...
   ```

3. **Check for middleware stripping headers** (reverse proxy, etc.)

---

#### Issue: Cost estimate missing

**Symptoms**:
`X-Nexus-Cost-Estimated` header not present on cloud backend responses.

**Explanation**: Cost estimation requires exact token counting. Only OpenAI backend has this capability currently.

**Expected Behavior**:
- OpenAI requests: Header present
- Anthropic/Google requests: Header absent (heuristic counting insufficient)

---

### Debug Mode

Enable detailed logging:

```bash
RUST_LOG=debug cargo run -- serve
```

Filter for specific modules:
```bash
RUST_LOG=nexus::agent=debug,nexus::routing=debug cargo run -- serve
```

---

## Next Steps

- **Read**: [data-model.md](data-model.md) for entity details
- **Read**: [contracts/](contracts/) for API format specifications
- **Implement**: Follow TDD approach (tests first)
- **Test**: Use integration tests in `tests/integration/cloud_backends_test.rs`

---

## Support

- **Issues**: File bugs at https://github.com/leocamello/nexus/issues
- **Docs**: Full documentation at https://docs.nexus-orchestrator.dev
- **Constitution**: See `.specify/memory/constitution.md` for design principles

---

**Version**: 1.0 | **Last Updated**: 2024-02-11
