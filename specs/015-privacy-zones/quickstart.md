# Quickstart: Privacy Zones & Capability Tiers

**Feature Branch**: `015-privacy-zones`  
**Date**: 2025-02-16

## What This Feature Does

Privacy Zones and Capability Tiers add **structural enforcement** of data locality and quality guarantees to Nexus:

- **Privacy Zones**: Ensure sensitive data never leaves your local infrastructure (no accidental cloud overflow)
- **Capability Tiers**: Prevent silent quality downgrades during failover (never surprise developers)
- **Flexible Routing**: Let clients choose strict (exact model) or flexible (tier-equivalent alternatives) routing

---

## Quick Start (5 minutes)

### 1. Configure Backend Privacy Zones

Edit `nexus.toml`:

```toml
# Local backend: restricted zone (default)
[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
zone = "Restricted"  # Explicit (same as default)

# Cloud backend: open zone (opt-in)
[[backends]]
name = "openai-gpt4"
url = "https://api.openai.com"
type = "openai"
api_key_env = "OPENAI_API_KEY"
zone = "Open"  # Allows overflow from any zone
```

---

### 2. Declare Backend Capabilities

Add capability scores to backends:

```toml
[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
zone = "Restricted"

[backends.local-ollama.capability_tier]
reasoning = 7
coding = 8
context_window = 8192
vision = false
tools = true
```

---

### 3. Define Traffic Policies (Optional)

Add route-specific requirements:

```toml
# Code routes: high capability, local-only
[routing.policies."code-*"]
privacy = "restricted"
min_reasoning = 7
min_coding = 8
overflow_mode = "block-entirely"

# Chat routes: moderate capability, allow cloud overflow
[routing.policies."chat-*"]
min_reasoning = 5
overflow_mode = "fresh-only"
```

---

### 4. Test Privacy Enforcement

**Scenario**: Request to restricted zone, cloud backend unavailable

```bash
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "code-llama",
    "messages": [{"role": "user", "content": "Write a function"}]
  }'
```

**Expected Behavior**:
- ✅ Routes to local backend (restricted zone)
- ❌ Never overflows to cloud backend (even if local is at capacity)
- Returns 503 if local backend unavailable (with Retry-After header)

---

### 5. Test Tier Enforcement

**Scenario**: Request requires high capability, only low-tier backend available

```bash
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "code-llama",
    "messages": [{"role": "user", "content": "Implement merge sort"}]
  }'
```

**Expected Behavior** (with policy `min_coding = 8`):
- ✅ Routes to backend with coding ≥ 8
- ❌ Rejects backend with coding < 8
- Returns 503 if no backends meet tier (with structured error)

---

### 6. Test Flexible Routing

**Scenario**: Client allows tier-equivalent alternatives

```bash
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "X-Nexus-Flexible: true" \
  -d '{
    "model": "llama3:70b",
    "messages": [{"role": "user", "content": "Explain quantum computing"}]
  }'
```

**Expected Behavior**:
- ✅ Routes to llama3:70b if available
- ✅ Falls back to qwen2:72b if llama3:70b unavailable (same-or-higher tier)
- ❌ Never falls back to mistral:7b (lower tier)
- Returns actual model used in `response.model` field

---

## Configuration Examples

### Example 1: Enterprise Hybrid Deployment

**Use Case**: Local GPU for sensitive data, cloud for public content

```toml
# Local GPU: restricted zone, high capability
[[backends]]
name = "gpu-server"
url = "http://192.168.1.100:8000"
type = "vllm"
zone = "Restricted"

[backends.gpu-server.capability_tier]
reasoning = 9
coding = 9
context_window = 128000
vision = true
tools = true

# OpenAI: open zone, fallback capacity
[[backends]]
name = "openai-gpt4"
url = "https://api.openai.com"
type = "openai"
api_key_env = "OPENAI_API_KEY"
zone = "Open"

[backends.openai-gpt4.capability_tier]
reasoning = 10
coding = 10
context_window = 128000
vision = true
tools = true

# Policy: Customer data stays local
[routing.policies."customer-*"]
privacy = "restricted"
min_reasoning = 9
overflow_mode = "block-entirely"

# Policy: Public content can overflow
[routing.policies."public-*"]
min_reasoning = 7
overflow_mode = "fresh-only"
```

---

### Example 2: Home Lab (Local-Only)

**Use Case**: No cloud backends, quality tiers for different tasks

```toml
# Primary: High-end local model
[[backends]]
name = "llama3-70b"
url = "http://localhost:11434"
type = "ollama"
zone = "Restricted"

[backends.llama3-70b.capability_tier]
reasoning = 8
coding = 8
context_window = 8192

# Fallback: Faster, lower quality
[[backends]]
name = "llama3-8b"
url = "http://localhost:11435"
type = "ollama"
zone = "Restricted"

[backends.llama3-8b.capability_tier]
reasoning = 6
coding = 6
context_window = 8192

# Policy: Code requires high tier
[routing.policies."code-*"]
min_coding = 8
overflow_mode = "block-entirely"

# Policy: Chat can use lower tier
[routing.policies."chat-*"]
min_reasoning = 5
```

---

## Client Integration

### TypeScript (OpenAI SDK)

```typescript
import OpenAI from 'openai';

const client = new OpenAI({
  baseURL: 'http://localhost:8000/v1',
  apiKey: 'not-used',
});

// Strict mode (default): exact model only
try {
  const response = await client.chat.completions.create({
    model: 'code-llama',
    messages: [{role: 'user', content: 'Write a function'}],
  });
  console.log(response.choices[0].message.content);
} catch (error) {
  if (error.status === 503) {
    console.error('Model unavailable:', error.context?.rejection_reason);
    console.log('Retry after:', error.context?.retry_after_seconds, 'seconds');
  }
}

// Flexible mode: allow tier-equivalent alternatives
const response = await client.chat.completions.create({
  model: 'llama3:70b',
  messages: [{role: 'user', content: 'Explain concept'}],
}, {
  headers: {
    'X-Nexus-Flexible': 'true',
  },
});

console.log('Used model:', response.model);
```

---

### Python (OpenAI SDK)

```python
from openai import OpenAI
import time

client = OpenAI(
    base_url="http://localhost:8000/v1",
    api_key="not-used",
)

# Strict mode with retry logic
for attempt in range(3):
    try:
        response = client.chat.completions.create(
            model="code-llama",
            messages=[{"role": "user", "content": "Write a function"}],
        )
        print(response.choices[0].message.content)
        break
    except Exception as e:
        if e.status_code == 503:
            retry_after = e.context.get('retry_after_seconds', 30)
            print(f"Retrying after {retry_after}s...")
            time.sleep(retry_after)
        else:
            raise

# Flexible mode
response = client.chat.completions.create(
    model="llama3:70b",
    messages=[{"role": "user", "content": "Explain concept"}],
    extra_headers={"X-Nexus-Flexible": "true"},
)

print(f"Used model: {response.model}")
```

---

## Operational Guidance

### Monitoring Metrics

```prometheus
# Privacy zone rejections
privacy_zone_rejections_total{zone="restricted", backend="openai"} 123

# Tier rejections
tier_rejections_total{dimension="reasoning", required="8", backend="local"} 45

# Cross-zone overflow events
cross_zone_overflow_total{from="restricted", to="open", has_history="false"} 12

# Affinity breaks
affinity_break_total{backend="gpu-server", reason="backend_unhealthy"} 7
```

### Alert Rules

```yaml
# Alert: Privacy zone violations
- alert: PrivacyZoneViolations
  expr: rate(privacy_zone_rejections_total[5m]) > 0.1
  annotations:
    summary: "Frequent privacy zone rejections detected"

# Alert: Tier unavailable
- alert: TierUnavailable
  expr: rate(tier_rejections_total[5m]) > 0.5
  annotations:
    summary: "High-tier backends frequently unavailable"
```

---

## Troubleshooting

### Issue: All requests returning 503

**Check**:
1. Are backends healthy? `curl http://localhost:8000/v1/models`
2. Do backends match privacy zone? Check `zone` in config
3. Do backends meet tier requirements? Check `capability_tier` in config

**Fix**: Adjust TrafficPolicy requirements or add more backends

---

### Issue: Unexpected cloud overflow

**Check**:
1. Is backend configured as `zone = "Restricted"`? (uppercase R)
2. Is TrafficPolicy set to `overflow_mode = "block-entirely"`?
3. Does request have conversation history? (blocks fresh-only overflow)

**Fix**: Set `zone = "Restricted"` and `overflow_mode = "block-entirely"`

---

### Issue: No tier-equivalent alternatives found

**Check**:
1. Are capability tiers declared for all backends?
2. Is `X-Nexus-Flexible` header included?
3. Do alternatives have same-or-higher scores on ALL dimensions?

**Fix**: Declare capability tiers or use strict mode (default)

---

## Next Steps

1. **Review Configuration**: Ensure all backends have privacy zones and capability tiers
2. **Define Policies**: Create TrafficPolicies for route-specific requirements
3. **Test Scenarios**: Verify privacy enforcement and tier matching
4. **Monitor Metrics**: Set up alerts for rejections and overflow events
5. **Update Clients**: Add flexible routing headers where appropriate

---

**Quickstart Complete**: Configuration, testing, client integration, and troubleshooting guidance provided.
