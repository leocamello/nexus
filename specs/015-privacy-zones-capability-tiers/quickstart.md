# Quickstart: Privacy Zones & Capability Tiers

**Feature**: F13 - Privacy Zones & Capability Tiers  
**Audience**: System administrators, DevOps engineers  
**Time to Complete**: 10-15 minutes

---

## Overview

This guide shows you how to configure privacy zones and capability tiers in Nexus to:

1. **Enforce privacy boundaries**: Prevent sensitive models from routing to cloud backends
2. **Control quality tiers**: Ensure requests get appropriate model capabilities
3. **Enable explicit failover**: Return actionable 503 errors instead of silent downgrades

---

## Prerequisites

- Nexus installed and running
- Access to `nexus.toml` configuration file
- Basic understanding of Nexus backend configuration

---

## Quick Example

**Goal**: Configure a local Ollama backend as "restricted" (privacy-sensitive) and a cloud OpenAI backend as "open" (can receive overflow).

### 1. Configure Backends with Privacy Zones

Edit `nexus.toml`:

```toml
[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
priority = 100              # Higher priority = preferred
zone = "restricted"         # Local-only, no cloud failover
tier = 2                    # Medium capability

[[backends]]
name = "cloud-gpt4"
url = "https://api.openai.com/v1"
type = "openai"
api_key_env = "OPENAI_API_KEY"
zone = "open"              # Can receive overflow (default for cloud)
tier = 5                   # Highest capability
```

### 2. Define Traffic Policies

Add privacy requirements for specific models:

```toml
[[traffic_policies]]
model_pattern = "llama*"           # All llama models
privacy_constraint = "restricted"  # Must stay local
min_tier = 2                       # Minimum capability tier

[[traffic_policies]]
model_pattern = "gpt-4*"           # GPT-4 models
min_tier = 4                       # Premium models only
# No privacy_constraint = can route to any zone
```

### 3. Test Privacy Enforcement

**Request to restricted model with local backend available**:

```bash
curl http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3:70b",
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

**Response** (200 OK):
```http
HTTP/1.1 200 OK
X-Nexus-Backend: local-ollama
X-Nexus-Backend-Type: local
X-Nexus-Privacy-Zone: restricted
X-Nexus-Route-Reason: capability-match

{
  "id": "chatcmpl-123",
  "object": "chat.completion",
  ...
}
```

**Request to restricted model with local backend OFFLINE**:

```bash
# Same request as above, but local-ollama is down
```

**Response** (503 Service Unavailable):
```http
HTTP/1.1 503 Service Unavailable
Retry-After: 30

{
  "error": {
    "message": "No backends available for model llama3:70b",
    "type": "service_unavailable",
    "code": null,
    "context": {
      "privacy_zone_required": "restricted",
      "rejection_reasons": [
        {
          "agent_id": "cloud-gpt4",
          "reconciler": "PrivacyReconciler",
          "reason": "Backend privacy zone 'open' violates constraint 'restricted'",
          "suggested_action": "Configure a restricted backend or modify TrafficPolicy"
        }
      ]
    }
  }
}
```

**Key Insight**: Cloud backend was excluded even though it's healthy. Privacy enforcement prevented cross-zone failover.

---

## Configuration Options

### Backend Privacy Zones

| Zone | Meaning | Typical Use Case |
|------|---------|------------------|
| `restricted` | Local-only, never receives cloud overflow | Sensitive data, airgapped environments, compliance requirements |
| `open` | Can receive overflow from any zone | Cloud backends, non-sensitive workloads |

**Defaults**:
- Local backends (Ollama, vLLM, llama.cpp): `restricted`
- Cloud backends (OpenAI, Anthropic, Google): `open`
- Explicit config overrides backend type default

### Capability Tiers

| Tier | Quality Level | Example Models |
|------|--------------|----------------|
| 1 | Basic | Small local models (7B parameters) |
| 2 | Moderate | Medium local models (13B parameters) |
| 3 | Standard | Large local models (70B), GPT-3.5 |
| 4 | Advanced | GPT-4, Claude 3 Sonnet |
| 5 | Premium | GPT-4 Turbo, Claude 3 Opus |

**Default**: Tier 1 if not specified

**Validation**: Tier must be 1-5 inclusive (validated at startup)

### Traffic Policies

```toml
[[traffic_policies]]
model_pattern = "<glob-pattern>"        # e.g., "gpt-4*", "llama*"
privacy_constraint = "<restricted|open>" # Optional
min_tier = <1-5>                        # Optional
```

**Zero-Config Behavior**: If no `traffic_policies` section exists, all backends participate in routing (backward compatible).

---

## Request Headers

### Strict Mode (Default)

**Behavior**: Only route to backends with exact or higher tier than policy requirement.

```bash
# Explicit strict mode (optional, this is the default)
curl http://localhost:3000/v1/chat/completions \
  -H "X-Nexus-Strict: true" \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-4", "messages": [...]}'
```

**Result**: If policy requires tier 4, only tier 4+ backends are candidates.

### Flexible Mode

**Behavior**: Allow higher-tier substitution when exact tier unavailable, but never downgrade.

```bash
# Flexible mode - allow higher tier substitution
curl http://localhost:3000/v1/chat/completions \
  -H "X-Nexus-Flexible: true" \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-3.5-turbo", "messages": [...]}'
```

**Result**: If policy requires tier 3 and only tier 4 backends available, route to tier 4.

---

## Common Scenarios

### Scenario 1: Airgapped Environment (All Local)

**Goal**: Ensure no requests ever reach cloud backends.

```toml
[[backends]]
name = "ollama-local"
url = "http://localhost:11434"
type = "ollama"
zone = "restricted"
tier = 2

# No traffic policies needed - single zone = no cross-zone routing
```

**Result**: All requests stay local. No external network calls.

---

### Scenario 2: Hybrid Local + Cloud with Privacy Boundaries

**Goal**: Use local for sensitive models, cloud for everything else.

```toml
# Local backend for sensitive work
[[backends]]
name = "local-llama"
url = "http://localhost:11434"
type = "ollama"
zone = "restricted"
tier = 2

# Cloud backend for general use
[[backends]]
name = "openai-gpt4"
url = "https://api.openai.com/v1"
type = "openai"
api_key_env = "OPENAI_API_KEY"
zone = "open"
tier = 5

# Policy: Keep llama models local
[[traffic_policies]]
model_pattern = "llama*"
privacy_constraint = "restricted"
min_tier = 2

# Policy: GPT models can use cloud
[[traffic_policies]]
model_pattern = "gpt-*"
min_tier = 3
# No privacy_constraint = can route to any zone
```

**Result**: 
- `llama3:70b` → local-llama only (even if offline → 503)
- `gpt-4` → openai-gpt4 (cloud allowed)
- `gpt-3.5-turbo` → openai-gpt4 (meets tier 3 requirement)

---

### Scenario 3: Quality Tiers with Failover Control

**Goal**: Ensure premium requests never downgrade to lower-quality models.

```toml
[[backends]]
name = "local-llama2"
url = "http://localhost:11434"
type = "ollama"
tier = 2

[[backends]]
name = "local-llama3"
url = "http://192.168.1.100:11434"
type = "ollama"
tier = 3

[[backends]]
name = "cloud-gpt4"
url = "https://api.openai.com/v1"
type = "openai"
api_key_env = "OPENAI_API_KEY"
tier = 5

# Require tier 3+ for all requests
[[traffic_policies]]
model_pattern = "*"
min_tier = 3
```

**Strict Mode** (default):
- `gpt-4` request → cloud-gpt4 (tier 5) ✅
- `llama3:70b` request → local-llama3 (tier 3) ✅
- If local-llama3 offline → 503 (won't use tier 2 backend) ❌

**Flexible Mode** (`X-Nexus-Flexible: true`):
- Same as strict if tier 3+ backends available
- If only tier 5 available → use tier 5 (higher OK) ✅
- If only tier 2 available → 503 (never downgrade) ❌

---

### Scenario 4: Development vs Production Tiers

**Goal**: Use fast local models in dev, premium cloud models in prod.

**Development** (`nexus-dev.toml`):
```toml
[[backends]]
name = "dev-ollama"
url = "http://localhost:11434"
type = "ollama"
zone = "restricted"
tier = 1  # Fast, lower quality OK

[[traffic_policies]]
model_pattern = "*"
min_tier = 1  # Accept any tier in dev
```

**Production** (`nexus-prod.toml`):
```toml
[[backends]]
name = "prod-gpt4"
url = "https://api.openai.com/v1"
type = "openai"
api_key_env = "OPENAI_API_KEY"
zone = "open"
tier = 5  # Premium only

[[traffic_policies]]
model_pattern = "*"
min_tier = 4  # Require premium tiers in prod
```

---

## Response Headers

All successful responses include privacy zone information:

```http
HTTP/1.1 200 OK
X-Nexus-Backend: local-ollama
X-Nexus-Backend-Type: local
X-Nexus-Privacy-Zone: restricted
X-Nexus-Route-Reason: capability-match
X-Nexus-Cost-Estimated: 0.0000

Content-Type: application/json

{
  "id": "chatcmpl-123",
  ...
}
```

**Key Headers**:
- `X-Nexus-Privacy-Zone`: `restricted` or `open` (reflects backend's configured zone)
- `X-Nexus-Backend-Type`: `local` or `cloud` (quick classification)
- `X-Nexus-Route-Reason`: Why this backend was chosen
- `X-Nexus-Cost-Estimated`: USD cost (cloud backends only)

---

## Error Responses

### Privacy Violation

```json
{
  "error": {
    "message": "No backends available for model llama3:70b",
    "type": "service_unavailable",
    "code": null,
    "context": {
      "privacy_zone_required": "restricted",
      "rejection_reasons": [
        {
          "agent_id": "cloud-gpt4",
          "reconciler": "PrivacyReconciler",
          "reason": "Backend privacy zone 'open' violates constraint 'restricted'",
          "suggested_action": "Configure a restricted backend or modify TrafficPolicy"
        }
      ]
    }
  }
}
```

### Tier Requirement Not Met

```json
{
  "error": {
    "message": "No backends available for model gpt-4",
    "type": "service_unavailable",
    "code": null,
    "context": {
      "required_tier": 4,
      "rejection_reasons": [
        {
          "agent_id": "local-llama2",
          "reconciler": "TierReconciler",
          "reason": "Backend tier 2 below required minimum tier 4",
          "suggested_action": "Use X-Nexus-Flexible header to allow tier fallback or configure higher-tier backend"
        }
      ]
    }
  }
}
```

---

## Validation & Troubleshooting

### Startup Validation

Nexus validates configuration at startup:

```bash
nexus serve
```

**Invalid tier value**:
```
Error: Backend 'local-ollama' has invalid tier 10, must be 1-5
```

**Cloud backend missing API key**:
```
Error: Backend 'cloud-gpt4' of type OpenAI requires 'api_key_env' field
```

### Debug Logging

Enable debug logging to see reconciler decisions:

```bash
RUST_LOG=nexus::routing::reconciler=debug nexus serve
```

**Example output**:
```
[DEBUG] PrivacyReconciler: Model llama3:70b matches policy with constraint=Restricted
[DEBUG] PrivacyReconciler: Excluding agent 'cloud-gpt4' (zone=Open, constraint=Restricted)
[DEBUG] TierReconciler: Model llama3:70b requires min_tier=2
[DEBUG] TierReconciler: Agent 'local-ollama' (tier=2) passes tier check
```

### Common Issues

**Issue**: Cloud backend being used for local-only models

**Cause**: Missing `privacy_constraint` in traffic policy

**Fix**:
```toml
[[traffic_policies]]
model_pattern = "llama*"
privacy_constraint = "restricted"  # ← Add this
```

---

**Issue**: Always getting 503 for GPT-4 requests

**Cause**: Backend tier too low for policy requirement

**Fix**: Either increase backend tier or lower policy requirement:
```toml
[[backends]]
name = "openai-gpt4"
tier = 5  # ← Increase from 3 to 5

[[traffic_policies]]
model_pattern = "gpt-4*"
min_tier = 4  # ← Or lower from 5 to 4
```

---

**Issue**: Flexible mode not working

**Cause**: Conflicting headers (`X-Nexus-Strict: true` takes precedence)

**Fix**: Remove `X-Nexus-Strict` header:
```bash
# Wrong (conflicting headers)
curl -H "X-Nexus-Strict: true" -H "X-Nexus-Flexible: true" ...

# Correct
curl -H "X-Nexus-Flexible: true" ...
```

---

## Best Practices

### 1. Explicit Zone Configuration

**Don't rely on defaults** - explicitly set `zone` for clarity:

```toml
# Good: Explicit zone
[[backends]]
name = "local-ollama"
type = "ollama"
zone = "restricted"  # Clear intent

# Bad: Relying on default
[[backends]]
name = "local-ollama"
type = "ollama"
# zone defaults to restricted... but not obvious
```

### 2. Tier Alignment with Model Capabilities

**Match tiers to actual model quality**:

```toml
# Good: Tier reflects model capability
[[backends]]
name = "ollama-llama3-70b"
tier = 3  # Large model = tier 3

[[backends]]
name = "ollama-llama2-7b"
tier = 1  # Small model = tier 1

# Bad: All backends same tier
[[backends]]
tier = 3  # GPT-4 and tiny local model both tier 3? Wrong!
```

### 3. Privacy-First Policies

**Start restrictive, relax as needed**:

```toml
# Good: Default to restricted, selectively open
[[traffic_policies]]
model_pattern = "*"
privacy_constraint = "restricted"

[[traffic_policies]]
model_pattern = "gpt-3.5-*"
# No constraint = can use cloud

# Bad: Default to open, try to restrict exceptions
# (Easy to forget a pattern and leak to cloud)
```

### 4. Use Flexible Mode Sparingly

**Flexible mode weakens guarantees** - only use when necessary:

```bash
# Prefer: Explicit tier requirements with strict enforcement
curl -H "X-Nexus-Strict: true" ...

# Use flexible only when: higher tier is acceptable substitute
curl -H "X-Nexus-Flexible: true" ...  # "I'm OK with GPT-4 if GPT-3.5 unavailable"
```

---

## Next Steps

- **Read**: [API Contracts](./contracts/) for detailed header/response specifications
- **Explore**: [Data Model](./data-model.md) for implementation details
- **Test**: Use `tests/privacy_tier_integration_test.rs` as examples
- **Monitor**: Check `X-Nexus-Privacy-Zone` headers in production responses

---

## Related Documentation

- [Feature Specification](./spec.md) - Complete requirements and user stories
- [Implementation Plan](./plan.md) - Technical implementation details
- [Request Headers Contract](./contracts/request-headers.md) - X-Nexus-Strict/Flexible spec
- [Response Headers Contract](./contracts/response-headers.md) - X-Nexus-Privacy-Zone spec
- [Error Responses Contract](./contracts/error-responses.md) - 503 error format

---

## Version

**Document Version**: 1.0  
**Feature Version**: v0.3.0  
**Last Updated**: 2025-01-24
