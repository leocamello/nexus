# Quickstart: Using the Nexus Inference Interface

**Feature**: F12 NII Extraction (RFC-001 Phase 1)  
**Audience**: Nexus developers implementing or extending agent functionality  
**Date**: 2026-02-15

This guide shows how to use the InferenceAgent abstraction layer in different scenarios.

---

## For Developers: Adding a New Backend Type

### 1. Define Your Agent Struct

```rust
// src/agent/mybackend.rs

use super::{AgentProfile, InferenceAgent, AgentError, HealthStatus, ModelCapability};
use async_trait::async_trait;
use std::sync::Arc;

pub struct MyBackendAgent {
    id: String,
    name: String,
    base_url: String,
    client: Arc<reqwest::Client>,
}

impl MyBackendAgent {
    pub fn new(
        id: String,
        name: String,
        base_url: String,
        client: Arc<reqwest::Client>,
    ) -> Self {
        Self { id, name, base_url, client }
    }
}
```

### 2. Implement Required Methods

```rust
#[async_trait]
impl InferenceAgent for MyBackendAgent {
    fn id(&self) -> &str {
        &self.id
    }
    
    fn name(&self) -> &str {
        &self.name
    }
    
    fn profile(&self) -> AgentProfile {
        AgentProfile {
            backend_type: "mybackend".to_string(),
            version: None,
            privacy_zone: PrivacyZone::Restricted,
            capabilities: AgentCapabilities::default(),
        }
    }
    
    async fn health_check(&self) -> Result<HealthStatus, AgentError> {
        // Call your backend's health endpoint
        let response = self.client
            .get(&format!("{}/health", self.base_url))
            .send()
            .await
            .map_err(|e| AgentError::Network(e.to_string()))?;
        
        if response.status().is_success() {
            Ok(HealthStatus::Healthy { model_count: 1 })
        } else {
            Ok(HealthStatus::Unhealthy)
        }
    }
    
    async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
        // Query your backend's model list endpoint
        // Apply capability detection (API response or heuristics)
        Ok(vec![/* discovered models */])
    }
    
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
        headers: Option<&HeaderMap>,
    ) -> Result<ChatCompletionResponse, AgentError> {
        // Forward to OpenAI-compatible endpoint
        let url = format!("{}/v1/chat/completions", self.base_url);
        
        let mut req = self.client.post(&url).json(&request);
        
        if let Some(hdrs) = headers {
            if let Some(auth) = hdrs.get("authorization") {
                req = req.header("Authorization", auth);
            }
        }
        
        let response = req
            .send()
            .await
            .map_err(|e| AgentError::Network(e.to_string()))?;
        
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AgentError::Upstream {
                status: response.status().as_u16(),
                message: body,
            });
        }
        
        response
            .json()
            .await
            .map_err(|e| AgentError::InvalidResponse(e.to_string()))
    }
    
    async fn chat_completion_stream(
        &self,
        request: ChatCompletionRequest,
        headers: Option<&HeaderMap>,
    ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
        // Implement SSE streaming (see data-model.md for example)
        todo!("Implement streaming")
    }
}
```

### 3. Update Agent Factory

```rust
// src/agent/factory.rs

pub fn create_agent(
    config: &BackendConfig,
    client: Arc<reqwest::Client>,
) -> Result<Arc<dyn InferenceAgent>, AgentError> {
    match config.backend_type {
        // ... existing types
        
        BackendType::MyBackend => Ok(Arc::new(MyBackendAgent::new(
            config.id.clone(),
            config.name.clone(),
            config.url.clone(),
            client,
        ))),
    }
}
```

### 4. Add Tests

```rust
// src/agent/tests/mybackend_tests.rs

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;
    
    #[tokio::test]
    async fn test_health_check_success() {
        let mut server = Server::new_async().await;
        let mock = server.mock("GET", "/health")
            .with_status(200)
            .create_async()
            .await;
        
        let client = Arc::new(reqwest::Client::new());
        let agent = MyBackendAgent::new(
            "test-1".to_string(),
            "Test Backend".to_string(),
            server.url(),
            client,
        );
        
        let result = agent.health_check().await;
        
        assert!(matches!(result, Ok(HealthStatus::Healthy { .. })));
        mock.assert_async().await;
    }
    
    // Add tests for list_models, chat_completion, streaming
}
```

---

## For Consumers: Using Agents in Nexus Components

### Health Checker Migration

**Before (type-specific branching)**:
```rust
// src/health/mod.rs (OLD)

pub fn get_health_endpoint(backend_type: BackendType) -> &'static str {
    match backend_type {
        BackendType::Ollama => "/api/tags",
        BackendType::LlamaCpp => "/health",
        BackendType::VLLM | ... => "/v1/models",
    }
}

pub async fn check_backend(&self, backend: &Backend) -> HealthCheckResult {
    let endpoint = Self::get_health_endpoint(backend.backend_type);
    let url = format!("{}{}", backend.url, endpoint);
    // ... HTTP call, type-specific parsing
}
```

**After (agent-based)**:
```rust
// src/health/mod.rs (NEW)

pub async fn check_backend(&self, backend_id: &str) -> HealthCheckResult {
    let agent = self.registry
        .get_agent(backend_id)
        .ok_or(HealthCheckError::BackendNotFound)?;
    
    // Uniform interface — no type checking
    let health_status = agent.health_check().await?;
    let models = agent.list_models().await?;
    
    Ok(HealthCheckResult {
        status: health_status,
        models,
        latency_ms: start_time.elapsed().as_millis() as u32,
    })
}
```

### Completions Handler Migration

**Before (direct HTTP)**:
```rust
// src/api/completions.rs (OLD)

async fn proxy_request(
    state: &Arc<AppState>,
    backend: &Backend,
    headers: &HeaderMap,
    request: &ChatCompletionRequest,
) -> Result<ChatCompletionResponse, ApiError> {
    let url = format!("{}/v1/chat/completions", backend.url);
    let mut req = state.http_client.post(&url).json(request);
    
    if let Some(auth) = headers.get("authorization") {
        req = req.header("Authorization", auth);
    }
    
    let response = req.send().await?;
    response.json().await.map_err(|e| ApiError::bad_gateway(&e.to_string()))
}
```

**After (agent-based)**:
```rust
// src/api/completions.rs (NEW)

async fn proxy_request(
    state: &Arc<AppState>,
    backend_id: &str,
    headers: &HeaderMap,
    request: ChatCompletionRequest,
) -> Result<ChatCompletionResponse, ApiError> {
    let agent = state.registry
        .get_agent(backend_id)
        .ok_or_else(|| ApiError::internal("Backend not found"))?;
    
    // Agent handles HTTP construction, URL formation, response parsing
    let response = agent.chat_completion(request, Some(headers))
        .await
        .map_err(|e| e.into())?;  // AgentError -> ApiError
    
    Ok(response)
}
```

### Registry Integration

**Registering a backend with agent**:
```rust
// src/main.rs or src/discovery/mdns.rs

let backend = Backend::new(
    id.clone(),
    name.clone(),
    url.clone(),
    backend_type,
    vec![],  // Models populated by health checker
    DiscoverySource::Static,
    metadata,
);

// Create agent from config
let agent = agent::create_agent(&backend_config, http_client.clone())?;

// Store both in registry (dual storage)
registry.add_backend_with_agent(backend, agent)?;
```

**Querying agents**:
```rust
// Get specific agent
if let Some(agent) = registry.get_agent("backend-uuid") {
    let models = agent.list_models().await?;
}

// Iterate all agents
for agent in registry.get_all_agents() {
    println!("Agent: {} ({})", agent.name(), agent.profile().backend_type);
}
```

---

## Common Patterns

### Error Handling

```rust
match agent.chat_completion(request, headers).await {
    Ok(response) => {
        // Success — return to client
        Ok(Json(response))
    }
    Err(AgentError::Network(msg)) => {
        // Retry next backend
        warn!("Network error: {}", msg);
        try_next_backend()
    }
    Err(AgentError::Timeout(ms)) => {
        // Retry next backend
        warn!("Timeout after {}ms", ms);
        try_next_backend()
    }
    Err(AgentError::Upstream { status, message }) => {
        // Backend returned error — log and retry
        warn!("Backend error {}: {}", status, message);
        try_next_backend()
    }
    Err(AgentError::Unsupported(method)) => {
        // Don't retry — method not implemented
        Err(ApiError::not_implemented(&format!("{} not supported", method)))
    }
    Err(e) => {
        // Other errors — log and fail
        Err(ApiError::internal(&e.to_string()))
    }
}
```

### Streaming with Cancellation

```rust
async fn handle_streaming(
    agent: Arc<dyn InferenceAgent>,
    request: ChatCompletionRequest,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let stream = agent
        .chat_completion_stream(request, Some(&headers))
        .await
        .map_err(|e| e.into())?;
    
    // Convert to SSE response
    let sse_stream = stream.map(|result| match result {
        Ok(chunk) => {
            let json = serde_json::to_string(&chunk).unwrap();
            Event::default().data(json)
        }
        Err(e) => {
            Event::default().event("error").data(e.to_string())
        }
    });
    
    Ok(Sse::new(sse_stream).into_response())
}

// If client disconnects, Axum drops the response, which drops the stream,
// which aborts the HTTP request to the backend (cancellation-safe).
```

### Capability Checking

```rust
let agent = registry.get_agent(backend_id)?;
let profile = agent.profile();

if profile.capabilities.embeddings {
    // This agent supports embeddings
    let embeddings = agent.embeddings(request).await?;
} else {
    // Return 501 Not Implemented
    return Err(ApiError::not_implemented("Embeddings not supported"));
}
```

---

## Testing Agents

### Unit Tests with Mock HTTP Backend

```rust
use mockito::Server;

#[tokio::test]
async fn test_ollama_agent_health_check() {
    let mut server = Server::new_async().await;
    
    let mock = server.mock("GET", "/api/tags")
        .with_status(200)
        .with_body(r#"{"models": [{"name": "llama3:8b"}]}"#)
        .create_async()
        .await;
    
    let client = Arc::new(reqwest::Client::new());
    let agent = OllamaAgent::new(
        "test-ollama".to_string(),
        "Test Ollama".to_string(),
        server.url(),
        client,
    );
    
    let result = agent.health_check().await;
    
    assert!(matches!(result, Ok(HealthStatus::Healthy { model_count: 1 })));
    mock.assert_async().await;
}
```

### Integration Tests with Real Backend

```rust
#[tokio::test]
#[ignore]  // Requires Ollama running on localhost:11434
async fn test_real_ollama_agent() {
    let client = Arc::new(reqwest::Client::new());
    let agent = OllamaAgent::new(
        "local-ollama".to_string(),
        "Local Ollama".to_string(),
        "http://localhost:11434".to_string(),
        client,
    );
    
    let health = agent.health_check().await.unwrap();
    assert!(matches!(health, HealthStatus::Healthy { .. }));
    
    let models = agent.list_models().await.unwrap();
    assert!(!models.is_empty());
}
```

---

## Performance Considerations

### Connection Pooling

**✅ DO**: Share one `reqwest::Client` across all agents
```rust
let http_client = Arc::new(reqwest::Client::new());

let agent1 = OllamaAgent::new(..., http_client.clone());
let agent2 = OpenAIAgent::new(..., http_client.clone());
```

**❌ DON'T**: Create one client per agent
```rust
// This fragments connection pools!
let agent1 = OllamaAgent::new(..., Arc::new(reqwest::Client::new()));
let agent2 = OpenAIAgent::new(..., Arc::new(reqwest::Client::new()));
```

### Caching Agent References

**✅ DO**: Store `Arc<dyn InferenceAgent>` in Registry, clone references
```rust
let agent = registry.get_agent(backend_id)?;  // Cheap Arc clone
agent.health_check().await?;
```

**❌ DON'T**: Recreate agents on every request
```rust
// Expensive! Don't do this
let agent = create_agent(&config, client)?;
```

### Streaming Performance

- **Stream chunks**: Process SSE chunks as they arrive, don't buffer entire response
- **Cancellation**: Drop stream on client disconnect to free resources
- **Backpressure**: Use bounded channels if buffering between agent and client

---

## Migration Checklist

When migrating a module to use agents:

- [ ] Replace `match backend_type {}` with agent trait calls
- [ ] Update function signatures to accept `Arc<dyn InferenceAgent>` or backend ID
- [ ] Convert errors: `AgentError` → `ApiError` / `HealthCheckError`
- [ ] Add unit tests with mock HTTP backends
- [ ] Verify existing integration tests still pass
- [ ] Update documentation and examples

---

## Next Steps

- Read `data-model.md` for detailed type definitions
- Read `contracts/README.md` for method contracts
- See `src/agent/tests/` for test examples
- See RFC-001 for architectural context

**Questions?** See `.specify/memory/constitution.md` for development principles.
