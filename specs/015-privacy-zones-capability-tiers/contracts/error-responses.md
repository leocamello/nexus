# Error Responses Contract

**Feature**: F13 - Privacy Zones & Capability Tiers  
**Version**: 1.0  
**Status**: Specification

---

## Overview

This document specifies the 503 Service Unavailable error response format when the reconciler pipeline rejects a request due to privacy zone or capability tier constraints. Error responses follow the OpenAI error envelope with a Nexus-specific `context` object that provides actionable information for clients.

---

## Error Envelope Structure

### JSON Schema

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "ServiceUnavailableError",
  "type": "object",
  "required": ["error", "context"],
  "properties": {
    "error": {
      "type": "object",
      "required": ["message", "type", "code"],
      "properties": {
        "message": {
          "type": "string",
          "description": "Human-readable error message describing why the request failed"
        },
        "type": {
          "type": "string",
          "const": "service_unavailable",
          "description": "Error type (always service_unavailable for 503)"
        },
        "param": {
          "type": ["string", "null"],
          "description": "Parameter that caused the error (usually null for 503)"
        },
        "code": {
          "type": ["string", "null"],
          "description": "Machine-readable error code"
        }
      }
    },
    "context": {
      "type": "object",
      "required": ["available_backends"],
      "properties": {
        "required_tier": {
          "type": ["integer", "null"],
          "minimum": 1,
          "maximum": 5,
          "description": "Tier required by traffic policy (present if rejection was tier-related)"
        },
        "available_backends": {
          "type": "array",
          "items": { "type": "string" },
          "description": "Names of backends currently registered (may be empty)"
        },
        "eta_seconds": {
          "type": ["integer", "null"],
          "description": "Estimated seconds until a backend may become available"
        },
        "privacy_zone_required": {
          "type": ["string", "null"],
          "enum": ["restricted", "open", null],
          "description": "Privacy zone required by traffic policy (present if rejection was privacy-related)"
        }
      }
    }
  }
}
```

### HTTP Response Format

All 503 responses use:

| Property | Value |
|----------|-------|
| Status Code | `503 Service Unavailable` |
| Content-Type | `application/json` |
| Body | `ServiceUnavailableError` JSON (see schema above) |

---

## Implementation

### Source Types

**Location**: `src/api/error.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceUnavailableError {
    /// Standard OpenAI error envelope
    pub error: ApiErrorBody,
    /// Nexus-specific actionable context
    pub context: ActionableErrorContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionableErrorContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_tier: Option<u8>,

    pub available_backends: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_seconds: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacy_zone_required: Option<String>,
}
```

**`ApiErrorBody`** (from `src/api/types.rs`):
```rust
pub struct ApiErrorBody {
    pub message: String,
    pub r#type: String,
    pub param: Option<String>,
    pub code: Option<String>,
}
```

### Construction Helpers

```rust
impl ServiceUnavailableError {
    /// Tier constraint rejection
    pub fn tier_unavailable(required_tier: u8, available_backends: Vec<String>) -> Self;

    /// Privacy zone constraint rejection
    pub fn privacy_unavailable(zone: &str, available_backends: Vec<String>) -> Self;

    /// All backends offline
    pub fn all_backends_down() -> Self;
}
```

### IntoResponse Implementation

```rust
impl axum::response::IntoResponse for ServiceUnavailableError {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::SERVICE_UNAVAILABLE, Json(self)).into_response()
    }
}
```

---

## Error Flow

### From Reconciler Pipeline to 503 Response

```
Reconciler Pipeline
  ├─ PrivacyReconciler → excludes zone-violating agents
  ├─ TierReconciler    → excludes under-tier agents
  └─ (other reconcilers)
       ↓
RoutingIntent (after pipeline)
  ├─ candidate_agents = []             (all excluded)
  ├─ rejection_reasons = [...]         (from each reconciler)
  ├─ privacy_constraint = Some(...)    (from matched policy)
  └─ min_capability_tier = Some(...)   (from matched policy)
       ↓
RoutingError::Reject
       ↓
ServiceUnavailableError
  ├─ error.message = "No backend available..."
  ├─ error.type = "service_unavailable"
  ├─ context.privacy_zone_required = "restricted"
  ├─ context.required_tier = 4
  └─ context.available_backends = [...]
       ↓
HTTP 503 Response (JSON body)
```

### RejectionReason Source

Each reconciler that excludes an agent records a `RejectionReason` (from `src/routing/reconciler/intent.rs`):

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RejectionReason {
    pub agent_id: String,
    pub reconciler: String,
    pub reason: String,
    pub suggested_action: String,
}
```

These accumulate on `RoutingIntent.rejection_reasons` and inform the error `message` and `context` fields when all candidates are eliminated.

---

## Rejection Scenarios

### Scenario 1: Privacy Zone Violation

**Condition**: Traffic policy requires `restricted` zone for a model, but only `open` (cloud) backends host it.

**Configuration**:
```toml
[[backends]]
name = "cloud-gpt4"
url = "https://api.openai.com/v1"
type = "openai"
api_key_env = "OPENAI_API_KEY"
# zone defaults to "open"

[[traffic_policies]]
model_pattern = "llama*"
privacy_constraint = "restricted"
```

**Request**:
```http
POST /v1/chat/completions HTTP/1.1
Content-Type: application/json

{
  "model": "llama3",
  "messages": [{"role": "user", "content": "Hello"}]
}
```

**Response**:
```http
HTTP/1.1 503 Service Unavailable
Content-Type: application/json
```

```json
{
  "error": {
    "message": "No backend available that satisfies privacy zone requirement: restricted",
    "type": "service_unavailable",
    "param": null,
    "code": "service_unavailable"
  },
  "context": {
    "available_backends": ["cloud-gpt4"],
    "privacy_zone_required": "restricted"
  }
}
```

**Why**: PrivacyReconciler excluded `cloud-gpt4` because its zone (`open`) violates the `restricted` constraint. No restricted backends are available.

---

### Scenario 2: Tier Requirement Not Met (Strict Mode)

**Condition**: Traffic policy requires minimum tier 4 for a model, but available backends are tier 2. Client uses strict mode (default).

**Configuration**:
```toml
[[backends]]
name = "ollama-llama2"
url = "http://localhost:11434"
type = "ollama"
tier = 2

[[traffic_policies]]
model_pattern = "gpt-4*"
min_tier = 4
```

**Request**:
```http
POST /v1/chat/completions HTTP/1.1
Content-Type: application/json

{
  "model": "gpt-4",
  "messages": [{"role": "user", "content": "Analyze this"}]
}
```

**Response**:
```http
HTTP/1.1 503 Service Unavailable
Content-Type: application/json
```

```json
{
  "error": {
    "message": "No backend available for requested model (tier 4 required)",
    "type": "service_unavailable",
    "param": null,
    "code": "service_unavailable"
  },
  "context": {
    "required_tier": 4,
    "available_backends": ["ollama-llama2"]
  }
}
```

**Why**: TierReconciler excluded `ollama-llama2` (tier 2) because strict mode requires exact or higher tier (≥ 4).

---

### Scenario 3: Combined Privacy + Tier Rejection

**Condition**: Policy requires both restricted zone and minimum tier 3. The only restricted backend is tier 1, and the tier 5 backend is cloud (open zone).

**Configuration**:
```toml
[[backends]]
name = "local-small"
url = "http://localhost:11434"
type = "ollama"
zone = "restricted"
tier = 1

[[backends]]
name = "cloud-gpt4"
url = "https://api.openai.com/v1"
type = "openai"
api_key_env = "OPENAI_API_KEY"
tier = 5

[[traffic_policies]]
model_pattern = "llama*"
privacy_constraint = "restricted"
min_tier = 3
```

**Request**:
```http
POST /v1/chat/completions HTTP/1.1
Content-Type: application/json

{
  "model": "llama3:70b",
  "messages": [{"role": "user", "content": "Sensitive analysis"}]
}
```

**Response**:
```http
HTTP/1.1 503 Service Unavailable
Content-Type: application/json
```

```json
{
  "error": {
    "message": "No backend available that satisfies privacy zone requirement: restricted",
    "type": "service_unavailable",
    "param": null,
    "code": "service_unavailable"
  },
  "context": {
    "required_tier": 3,
    "available_backends": [],
    "privacy_zone_required": "restricted"
  }
}
```

**Why**: PrivacyReconciler excluded `cloud-gpt4` (open zone), then TierReconciler excluded `local-small` (tier 1 < required 3). No candidates remain.

---

### Scenario 4: All Backends Down

**Condition**: All registered backends are unhealthy. No privacy or tier constraints involved.

**Request**:
```http
POST /v1/chat/completions HTTP/1.1
Content-Type: application/json

{
  "model": "llama3",
  "messages": [{"role": "user", "content": "Hello"}]
}
```

**Response**:
```http
HTTP/1.1 503 Service Unavailable
Content-Type: application/json
```

```json
{
  "error": {
    "message": "All backends are currently unavailable",
    "type": "service_unavailable",
    "param": null,
    "code": "service_unavailable"
  },
  "context": {
    "available_backends": []
  }
}
```

**Why**: No healthy backends registered. Context has no privacy or tier fields (they were not the cause of rejection).

---

### Scenario 5: Tier Rejection with Flexible Mode Suggested

**Condition**: Strict mode rejects a lower-tier backend. The suggested action hints at using flexible mode.

**Request**:
```http
POST /v1/chat/completions HTTP/1.1
Content-Type: application/json

{
  "model": "gpt-4",
  "messages": [{"role": "user", "content": "Quick question"}]
}
```

**Response**:
```http
HTTP/1.1 503 Service Unavailable
Content-Type: application/json
```

```json
{
  "error": {
    "message": "No backend available for requested model (tier 4 required)",
    "type": "service_unavailable",
    "param": null,
    "code": "service_unavailable"
  },
  "context": {
    "required_tier": 4,
    "available_backends": ["ollama-llama2"]
  }
}
```

**Client Recovery**: Client can retry with `X-Nexus-Flexible: true` to allow routing to a higher-tier backend if one becomes available, though strict mode will never downgrade to a lower tier. See [Request Headers Contract](./request-headers.md) for details on flexible mode.

---

## Context Field Behavior

### Conditional Presence

Context fields use `skip_serializing_if = "Option::is_none"` so they only appear when relevant:

| Field | Present When |
|-------|-------------|
| `required_tier` | Rejection involved tier constraints |
| `privacy_zone_required` | Rejection involved privacy zone constraints |
| `eta_seconds` | System can estimate recovery time |
| `available_backends` | Always (may be empty array) |

### Examples of Minimal Context

**No policy constraints (pure availability failure)**:
```json
{
  "context": {
    "available_backends": []
  }
}
```

**Only tier constraint**:
```json
{
  "context": {
    "required_tier": 4,
    "available_backends": ["ollama-llama2"]
  }
}
```

**Only privacy constraint**:
```json
{
  "context": {
    "available_backends": ["cloud-gpt4"],
    "privacy_zone_required": "restricted"
  }
}
```

**Both constraints**:
```json
{
  "context": {
    "required_tier": 3,
    "available_backends": [],
    "privacy_zone_required": "restricted"
  }
}
```

---

## OpenAI Compatibility

### Error Envelope

The `error` object follows the [OpenAI error format](https://platform.openai.com/docs/guides/error-codes):

| Field | Type | Description |
|-------|------|-------------|
| `message` | string | Human-readable error description |
| `type` | string | Always `"service_unavailable"` for 503 |
| `param` | string \| null | Parameter causing error (usually null) |
| `code` | string \| null | Machine-readable code |

### Extension Strategy

The `context` object is a Nexus extension placed **alongside** the `error` object (not inside it). Standard OpenAI clients that only read the `error` field will work correctly; clients aware of Nexus can read `context` for actionable information.

```json
{
  "error": { ... },     ← Standard OpenAI error envelope
  "context": { ... }    ← Nexus extension (ignored by unaware clients)
}
```

---

## Implementation Requirements

### FR-015: Privacy Zone Context
System MUST include `privacy_zone_required` in the 503 error context when a request is rejected due to privacy zone enforcement.

### FR-016: Tier Context
System MUST include `required_tier` in the 503 error context when a request is rejected due to tier enforcement.

### FR-020: Zero-Config Compatibility
When no traffic policies are configured, system MUST NOT produce privacy or tier rejection errors. Standard "no backends available" behavior applies.

---

## Testing Requirements

### Test Case 1: Privacy Zone Rejection Context
```
Given: Traffic policy requires "restricted" for model "llama*"
And: Only cloud (open) backends available
When: Request for model "llama3"
Then: 503 response with context.privacy_zone_required = "restricted"
And: context.available_backends includes the open backends
```

### Test Case 2: Tier Rejection Context
```
Given: Traffic policy requires min_tier = 4 for model "gpt-4*"
And: Only tier 2 backends available
When: Request for model "gpt-4"
Then: 503 response with context.required_tier = 4
And: context.available_backends includes the tier 2 backends
```

### Test Case 3: Combined Rejection Context
```
Given: Policy requires restricted zone AND min_tier = 3
And: Restricted backend is tier 1, open backend is tier 5
When: Request for matching model
Then: 503 response with both context.privacy_zone_required and context.required_tier set
```

### Test Case 4: No Policy (Zero-Config)
```
Given: No traffic_policies configured
And: All backends are unhealthy
When: Request for any model
Then: 503 response with context.available_backends = []
And: context.privacy_zone_required is absent
And: context.required_tier is absent
```

### Test Case 5: Serialization Correctness
```
Given: ServiceUnavailableError with required_tier = 3
When: Serialized to JSON
Then: JSON contains "required_tier": 3
And: JSON does NOT contain "eta_seconds" (null fields omitted)
And: JSON does NOT contain "privacy_zone_required" (null fields omitted)
```

### Test Case 6: IntoResponse Status Code
```
Given: ServiceUnavailableError instance
When: Converted via IntoResponse trait
Then: HTTP status code is 503
And: Content-Type is application/json
```

---

## OpenAPI Specification

```yaml
/v1/chat/completions:
  post:
    responses:
      503:
        description: |
          No backends available to serve the request. May be caused by:
          - Privacy zone constraints excluding available backends
          - Tier requirements not met by available backends
          - All backends unhealthy or offline
        content:
          application/json:
            schema:
              type: object
              required: [error, context]
              properties:
                error:
                  type: object
                  required: [message, type]
                  properties:
                    message:
                      type: string
                      example: "No backend available that satisfies privacy zone requirement: restricted"
                    type:
                      type: string
                      enum: [service_unavailable]
                    param:
                      type: string
                      nullable: true
                    code:
                      type: string
                      nullable: true
                      example: service_unavailable
                context:
                  type: object
                  required: [available_backends]
                  properties:
                    required_tier:
                      type: integer
                      minimum: 1
                      maximum: 5
                      nullable: true
                      description: Tier required by traffic policy
                      example: 4
                    available_backends:
                      type: array
                      items:
                        type: string
                      description: Currently registered backend names
                      example: ["ollama-llama2", "cloud-gpt4"]
                    eta_seconds:
                      type: integer
                      nullable: true
                      description: Estimated seconds until recovery
                    privacy_zone_required:
                      type: string
                      enum: [restricted, open]
                      nullable: true
                      description: Privacy zone required by traffic policy
                      example: restricted
            examples:
              privacy_rejection:
                summary: Privacy zone violation
                value:
                  error:
                    message: "No backend available that satisfies privacy zone requirement: restricted"
                    type: service_unavailable
                    param: null
                    code: service_unavailable
                  context:
                    available_backends: ["cloud-gpt4"]
                    privacy_zone_required: restricted
              tier_rejection:
                summary: Tier requirement not met
                value:
                  error:
                    message: "No backend available for requested model (tier 4 required)"
                    type: service_unavailable
                    param: null
                    code: service_unavailable
                  context:
                    required_tier: 4
                    available_backends: ["ollama-llama2"]
              all_down:
                summary: All backends unavailable
                value:
                  error:
                    message: "All backends are currently unavailable"
                    type: service_unavailable
                    param: null
                    code: service_unavailable
                  context:
                    available_backends: []
```

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2025-01-24 | Initial specification |

---

## Related Documents

- [Request Headers Contract](./request-headers.md)
- [Response Headers Contract](./response-headers.md)
- [Data Model](../data-model.md)
- [Feature Spec](../spec.md)
