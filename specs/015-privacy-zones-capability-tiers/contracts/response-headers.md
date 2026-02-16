# Response Headers Contract

**Feature**: F13 - Privacy Zones & Capability Tiers  
**Version**: 1.0  
**Status**: Specification

---

## Overview

This document specifies the `X-Nexus-Privacy-Zone` response header that communicates which privacy zone served a request. This header is part of the Nexus Transparent Protocol (F12) and is **always present** on successful responses from `/v1/chat/completions`.

---

## X-Nexus-Privacy-Zone

### Purpose
Inform the client which privacy zone's backend served their request, enabling auditability and compliance verification without modifying the OpenAI-compatible JSON response body.

### Specification

| Property | Value |
|----------|-------|
| Header Name | `X-Nexus-Privacy-Zone` |
| Direction | Response (server → client) |
| Presence | Always (on 200 responses) |
| Valid Values | `"restricted"`, `"open"` |
| Case Sensitivity | Header name is case-insensitive (HTTP standard) |
| Value Case | Always lowercase |
| Source | Backend configuration (`zone` field), NOT client request |

### Values

| Value | Meaning | Typical Backends |
|-------|---------|-----------------|
| `restricted` | Data stays local; no cloud overflow | Ollama, vLLM, llama.cpp, LM Studio, Exo |
| `open` | May traverse cloud providers | OpenAI, Anthropic, Google |

---

## Data Flow

The `X-Nexus-Privacy-Zone` header value originates from backend configuration and flows through the system without client influence.

### 1. Configuration (TOML)

```toml
[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
zone = "restricted"   # Explicit zone assignment

[[backends]]
name = "cloud-gpt4"
url = "https://api.openai.com/v1"
type = "openai"
api_key_env = "OPENAI_API_KEY"
# zone omitted → defaults to "open" (cloud backend type default)
```

### 2. Backend Registration (Startup)

```rust
// BackendConfig → AgentProfile (src/config/backend.rs)
let zone = config.effective_privacy_zone();
// effective_privacy_zone() returns:
//   - Explicit zone if set in TOML
//   - Backend type default otherwise (local=Restricted, cloud=Open)

// AgentProfile stores the resolved zone (src/agent/types.rs)
AgentProfile {
    privacy_zone: zone,      // PrivacyZone::Restricted or PrivacyZone::Open
    capability_tier: Some(config.effective_tier()),
    ..
}
```

### 3. Routing Decision (Request Time)

```rust
// Router selects a backend through the reconciler pipeline.
// The PrivacyReconciler may filter candidates by zone,
// but the selected backend's zone is carried through in its AgentProfile.
let routing_result = router.select_backend(...)?;
let selected_backend = routing_result.backend;
```

### 4. Header Injection (Response Time)

```rust
// NexusTransparentHeaders constructed from routing result (src/api/headers.rs)
let headers = NexusTransparentHeaders::new(
    backend.name.clone(),
    backend.backend_type,
    route_reason,
    backend.profile.privacy_zone,  // ← Zone from AgentProfile
    cost_estimated,
);

// Injected into HTTP response
headers.inject_into_response(&mut response);
```

### 5. Implementation (src/api/headers.rs)

```rust
pub const HEADER_PRIVACY_ZONE: &str = "x-nexus-privacy-zone";

// Inside NexusTransparentHeaders::inject_into_response()
let privacy_zone_str = match self.privacy_zone {
    PrivacyZone::Restricted => "restricted",
    PrivacyZone::Open => "open",
};
headers.insert(
    HeaderName::from_static(HEADER_PRIVACY_ZONE),
    HeaderValue::from_static(privacy_zone_str),
);
```

---

## Examples

### Local Backend (Restricted Zone)

```http
POST /v1/chat/completions HTTP/1.1
Host: localhost:3000
Content-Type: application/json

{
  "model": "llama3",
  "messages": [{"role": "user", "content": "Hello"}]
}
```

**Response**:
```http
HTTP/1.1 200 OK
Content-Type: application/json
X-Nexus-Backend: local-ollama
X-Nexus-Backend-Type: local
X-Nexus-Route-Reason: capability-match
X-Nexus-Privacy-Zone: restricted

{"id":"chatcmpl-123","object":"chat.completion",...}
```

### Cloud Backend (Open Zone)

```http
POST /v1/chat/completions HTTP/1.1
Host: localhost:3000
Content-Type: application/json

{
  "model": "gpt-4",
  "messages": [{"role": "user", "content": "Analyze this data"}]
}
```

**Response**:
```http
HTTP/1.1 200 OK
Content-Type: application/json
X-Nexus-Backend: cloud-gpt4
X-Nexus-Backend-Type: cloud
X-Nexus-Route-Reason: capability-match
X-Nexus-Privacy-Zone: open
X-Nexus-Cost-Estimated: 0.0042

{"id":"chatcmpl-456","object":"chat.completion",...}
```

### Privacy-Routed Request

When privacy policy forces routing to a restricted backend:

```http
POST /v1/chat/completions HTTP/1.1
Host: localhost:3000
Content-Type: application/json

{
  "model": "llama3",
  "messages": [{"role": "user", "content": "Sensitive data analysis"}]
}
```

**Response** (cloud backend excluded by PrivacyReconciler):
```http
HTTP/1.1 200 OK
Content-Type: application/json
X-Nexus-Backend: local-ollama
X-Nexus-Backend-Type: local
X-Nexus-Route-Reason: privacy-requirement
X-Nexus-Privacy-Zone: restricted

{"id":"chatcmpl-789","object":"chat.completion",...}
```

### Streaming Response

The header is present on the initial SSE response, before any event chunks:

```http
HTTP/1.1 200 OK
Content-Type: text/event-stream
X-Nexus-Backend: local-ollama
X-Nexus-Backend-Type: local
X-Nexus-Route-Reason: capability-match
X-Nexus-Privacy-Zone: restricted

data: {"id":"chatcmpl-abc","object":"chat.completion.chunk",...}

data: [DONE]
```

---

## Companion Response Headers

The `X-Nexus-Privacy-Zone` header is always emitted alongside other Nexus Transparent Protocol headers:

| Header | Format | Description |
|--------|--------|-------------|
| `X-Nexus-Backend` | String | Name of the backend that served the request |
| `X-Nexus-Backend-Type` | `"local"` \| `"cloud"` | Backend classification |
| `X-Nexus-Route-Reason` | kebab-case | Why this backend was selected |
| `X-Nexus-Privacy-Zone` | `"restricted"` \| `"open"` | Privacy zone of the serving backend |
| `X-Nexus-Cost-Estimated` | Decimal USD (4 places) | Estimated cost (cloud only, optional) |

---

## Security Considerations

### Privacy Zone is Server-Authoritative

**Rule**: The `X-Nexus-Privacy-Zone` response header reflects the backend's configured zone. It is NEVER influenced by client-provided headers (FR-024).

**Threat**: Client sends `X-Nexus-Privacy-Zone: open` as a request header to bypass restrictions.

**Mitigation**:
- System ignores all client-provided `X-Nexus-Privacy-Zone` headers
- Privacy zone is determined solely by `BackendConfig.zone` (or backend type default)
- The response header is injected by `NexusTransparentHeaders::inject_into_response()` from the resolved `AgentProfile.privacy_zone`

**Example** (attempted injection):
```http
POST /v1/chat/completions HTTP/1.1
X-Nexus-Privacy-Zone: open  ← IGNORED by server

{
  "model": "llama3",
  "messages": [...]
}
```

**Expected Response**:
```http
HTTP/1.1 200 OK
X-Nexus-Privacy-Zone: restricted  ← Reflects actual backend zone
```

### No Body Modification

Per the Nexus Transparent Protocol (F12), privacy zone information is conveyed exclusively through response headers. The OpenAI-compatible JSON response body is never modified with Nexus-specific fields.

---

## Implementation Requirements

### FR-010: Response Header Injection
System MUST include `X-Nexus-Privacy-Zone` header in all successful responses indicating the zone of the serving backend.

### FR-024: Client Privacy Headers Ignored
System MUST NOT allow client-provided privacy headers to override backend zone configuration. Privacy is a backend property only.

### SC-002: Consistent Injection
All response code paths (streaming and non-streaming) MUST use `NexusTransparentHeaders::inject_into_response()` as the single point of header injection.

---

## Testing Requirements

### Test Case 1: Restricted Backend Response
```http
POST /v1/chat/completions
(routed to backend with zone = "restricted")

Expected Response Header: X-Nexus-Privacy-Zone: restricted
```

### Test Case 2: Open Backend Response
```http
POST /v1/chat/completions
(routed to backend with zone = "open")

Expected Response Header: X-Nexus-Privacy-Zone: open
```

### Test Case 3: Default Zone (No Explicit Config)
```http
POST /v1/chat/completions
(routed to Ollama backend with no zone configured)

Expected Response Header: X-Nexus-Privacy-Zone: restricted
(Ollama is a local backend → default zone is Restricted)
```

### Test Case 4: Default Zone (Cloud Backend)
```http
POST /v1/chat/completions
(routed to OpenAI backend with no zone configured)

Expected Response Header: X-Nexus-Privacy-Zone: open
(OpenAI is a cloud backend → default zone is Open)
```

### Test Case 5: Client Injection Attempt
```http
POST /v1/chat/completions
X-Nexus-Privacy-Zone: open

(routed to Ollama backend with zone = "restricted")

Expected Response Header: X-Nexus-Privacy-Zone: restricted
(client header ignored; reflects actual backend zone)
```

### Test Case 6: Streaming Response
```http
POST /v1/chat/completions
{"model": "llama3", "messages": [...], "stream": true}

(routed to restricted backend)

Expected: X-Nexus-Privacy-Zone: restricted header present on initial SSE response
```

---

## OpenAPI Specification

```yaml
/v1/chat/completions:
  post:
    responses:
      200:
        description: Successful completion
        headers:
          X-Nexus-Privacy-Zone:
            required: true
            schema:
              type: string
              enum: [restricted, open]
            description: |
              Privacy zone of the backend that served this request.
              "restricted" indicates data stayed within local infrastructure.
              "open" indicates data may have traversed a cloud provider.
              This value is server-authoritative and cannot be influenced
              by client headers.
            example: restricted

          X-Nexus-Backend:
            required: true
            schema:
              type: string
            description: Name of the backend that served the request
            example: local-ollama

          X-Nexus-Backend-Type:
            required: true
            schema:
              type: string
              enum: [local, cloud]
            description: Backend classification
            example: local

          X-Nexus-Route-Reason:
            required: true
            schema:
              type: string
              enum: [capability-match, capacity-overflow, privacy-requirement, failover]
            description: Reason this backend was selected
            example: capability-match

          X-Nexus-Cost-Estimated:
            required: false
            schema:
              type: string
              pattern: '^\d+\.\d{4}$'
            description: Estimated cost in USD (cloud backends only)
            example: "0.0042"
```

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2025-01-24 | Initial specification |

---

## Related Documents

- [Request Headers Contract](./request-headers.md)
- [Error Responses Contract](./error-responses.md)
- [Data Model](../data-model.md)
- [Feature Spec](../spec.md)
