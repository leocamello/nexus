# Request Headers Contract

**Feature**: F13 - Privacy Zones & Capability Tiers  
**Version**: 1.0  
**Status**: Specification

---

## Overview

This document specifies the request headers that control tier enforcement behavior during routing. These headers are **optional** and parsed by the `/v1/chat/completions` endpoint.

---

## X-Nexus-Strict

### Purpose
Explicitly enforce strict tier matching - only route to backends with exact or higher capability tier than required by policy.

### Specification

| Property | Value |
|----------|-------|
| Header Name | `X-Nexus-Strict` |
| Direction | Request (client → server) |
| Required | No |
| Default Behavior | Strict mode (FR-009) |
| Valid Values | `"true"`, `"false"`, absent |
| Case Sensitivity | Header name is case-insensitive (HTTP standard) |
| Value Sensitivity | Value is case-sensitive |

### Behavior

```
X-Nexus-Strict: true   → TierEnforcementMode::Strict
X-Nexus-Strict: false  → Use default or X-Nexus-Flexible
(absent)               → Use default or X-Nexus-Flexible
```

### Examples

**Strict Enforcement (Explicit)**:
```http
POST /v1/chat/completions HTTP/1.1
Host: localhost:3000
Content-Type: application/json
X-Nexus-Strict: true

{
  "model": "gpt-4",
  "messages": [...]
}
```

**Result**: If policy requires tier 4, only tier 4+ backends are candidates. Tier 3 backends are excluded even if no tier 4 backends are available.

---

## X-Nexus-Flexible

### Purpose
Allow tier-equivalent substitution - permit routing to higher-tier backends when exact tier is unavailable, but never downgrade to lower tiers.

### Specification

| Property | Value |
|----------|-------|
| Header Name | `X-Nexus-Flexible` |
| Direction | Request (client → server) |
| Required | No |
| Default Behavior | Strict mode (FR-009) |
| Valid Values | `"true"`, `"false"`, absent |
| Case Sensitivity | Header name is case-insensitive (HTTP standard) |
| Value Sensitivity | Value is case-sensitive |

### Behavior

```
X-Nexus-Flexible: true   → TierEnforcementMode::Flexible
X-Nexus-Flexible: false  → TierEnforcementMode::Strict
(absent)                 → TierEnforcementMode::Strict
```

### Examples

**Flexible Enforcement (Allow Higher Tier Substitution)**:
```http
POST /v1/chat/completions HTTP/1.1
Host: localhost:3000
Content-Type: application/json
X-Nexus-Flexible: true

{
  "model": "gpt-3.5-turbo",
  "messages": [...]
}
```

**Result**: If policy requires tier 3 and only tier 4 backends are available, route to tier 4 (higher tier acceptable in flexible mode).

---

## Conflict Resolution

### Both Headers Present

**Scenario**: Request contains both `X-Nexus-Strict: true` and `X-Nexus-Flexible: true`

**Rule**: **Strict takes precedence** (safer default, no surprises)

**Example**:
```http
POST /v1/chat/completions HTTP/1.1
X-Nexus-Strict: true
X-Nexus-Flexible: true

{
  "model": "gpt-4",
  "messages": [...]
}
```

**Result**: `TierEnforcementMode::Strict` (ignore X-Nexus-Flexible)

**Rationale**: Better to fail explicitly than silently substitute unexpected quality levels.

---

## Invalid Values

### Non-Boolean Values

**Rule**: Treat invalid values as "false"

**Examples**:
```http
X-Nexus-Strict: yes      → Strict (invalid value → false, use default)
X-Nexus-Flexible: 1      → Strict (invalid value → false)
X-Nexus-Flexible: TRUE   → Strict (case-sensitive, must be lowercase "true")
```

### Empty Values

**Rule**: Treat empty values as "false"

**Examples**:
```http
X-Nexus-Strict:          → Strict (empty → false, use default)
X-Nexus-Flexible:        → Strict (empty → false)
```

---

## Implementation Requirements

### FR-007: X-Nexus-Strict Parsing
System MUST parse `X-Nexus-Strict: true` header to enforce exact model matching.

### FR-008: X-Nexus-Flexible Parsing
System MUST parse `X-Nexus-Flexible: true` header to allow tier-equivalent alternatives.

### FR-009: Default Behavior
System MUST default to strict enforcement mode when neither header is present.

### FR-024: Client Privacy Headers Ignored
System MUST NOT allow client-provided privacy headers to override backend zone configuration. Privacy is a backend property only.

---

## Parsing Logic (Pseudocode)

```rust
fn extract_tier_enforcement_mode(headers: &HeaderMap) -> TierEnforcementMode {
    // 1. Check X-Nexus-Strict first (takes precedence)
    if let Some(value) = headers.get("x-nexus-strict") {
        if value.to_str().ok() == Some("true") {
            return TierEnforcementMode::Strict;
        }
    }
    
    // 2. Check X-Nexus-Flexible
    if let Some(value) = headers.get("x-nexus-flexible") {
        if value.to_str().ok() == Some("true") {
            return TierEnforcementMode::Flexible;
        }
    }
    
    // 3. Default to Strict (FR-009)
    TierEnforcementMode::Strict
}
```

---

## Security Considerations

### Privacy Header Injection Attempts

**Threat**: Client sends `X-Nexus-Privacy-Zone: open` to bypass restrictions

**Mitigation**: 
- System MUST ignore all client-provided privacy-related headers
- Privacy zone is ONLY determined by backend configuration
- Response header `X-Nexus-Privacy-Zone` reflects actual backend zone, not client request

**Test Case**:
```http
POST /v1/chat/completions HTTP/1.1
X-Nexus-Privacy-Zone: open  ← IGNORED

{
  "model": "llama3",
  "messages": [...]
}
```

**Expected**: System routes based on backend's configured zone, ignores client header.

---

## Testing Requirements

### Test Case 1: No Headers (Default Strict)
```http
POST /v1/chat/completions
(no tier enforcement headers)

Expected: TierEnforcementMode::Strict
```

### Test Case 2: Explicit Strict
```http
POST /v1/chat/completions
X-Nexus-Strict: true

Expected: TierEnforcementMode::Strict
```

### Test Case 3: Flexible Mode
```http
POST /v1/chat/completions
X-Nexus-Flexible: true

Expected: TierEnforcementMode::Flexible
```

### Test Case 4: Conflicting Headers (Strict Wins)
```http
POST /v1/chat/completions
X-Nexus-Strict: true
X-Nexus-Flexible: true

Expected: TierEnforcementMode::Strict
```

### Test Case 5: Invalid Values
```http
POST /v1/chat/completions
X-Nexus-Flexible: yes

Expected: TierEnforcementMode::Strict (invalid value ignored)
```

### Test Case 6: Privacy Header Ignored
```http
POST /v1/chat/completions
X-Nexus-Privacy-Zone: open

Expected: System uses backend's configured zone, ignores client header
```

---

## OpenAPI Specification

```yaml
/v1/chat/completions:
  post:
    summary: Create chat completion
    parameters:
      - name: X-Nexus-Strict
        in: header
        required: false
        schema:
          type: string
          enum: [true, false]
        description: |
          Enforce strict tier matching. Only route to backends with exact or 
          higher tier than policy requirement. Defaults to true if absent.
          
      - name: X-Nexus-Flexible
        in: header
        required: false
        schema:
          type: string
          enum: [true, false]
        description: |
          Allow flexible tier matching. Route to higher-tier backends when 
          exact tier unavailable, but never downgrade. Ignored if X-Nexus-Strict 
          is also present.
    
    requestBody:
      required: true
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/ChatCompletionRequest'
    
    responses:
      200:
        description: Successful completion
        headers:
          X-Nexus-Privacy-Zone:
            schema:
              type: string
              enum: [restricted, open]
            description: Privacy zone of the backend that served this request
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/ChatCompletionResponse'
      
      503:
        description: No backends available
        headers:
          Retry-After:
            schema:
              type: integer
            description: Seconds to wait before retrying
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/Error'
```

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2025-01-24 | Initial specification |

---

## Related Documents

- [Response Headers Contract](./response-headers.md)
- [Error Responses Contract](./error-responses.md)
- [Data Model](../data-model.md)
- [Feature Spec](../spec.md)
