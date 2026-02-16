# API Contract: 503 Error Responses

**Feature**: Privacy Zones & Capability Tiers  
**Version**: 1.0  
**Date**: 2025-02-16

## Overview

When no backends are available due to privacy or capability constraints, Nexus returns a 503 Service Unavailable response with structured context for debugging.

---

## Response Format

### Standard OpenAI Error Envelope

```json
{
  "error": {
    "message": "<human-readable message>",
    "type": "insufficient_capacity",
    "code": 503,
    "context": {
      "rejection_reason": "<structured reason>",
      "...additional fields..."
    }
  }
}
```

**OpenAI Compatibility**: The `context` field is a Nexus extension. Standard OpenAI clients ignore it gracefully.

---

## Rejection Reasons

### privacy_zone_mismatch

**Cause**: No backends match required privacy zone

```json
{
  "error": {
    "message": "No backends available in required privacy zone 'restricted'",
    "type": "insufficient_capacity",
    "code": 503,
    "context": {
      "rejection_reason": "privacy_zone_mismatch",
      "required_zone": "restricted",
      "available_backends": [
        {
          "name": "openai-gpt4",
          "zone": "open",
          "status": "healthy"
        }
      ],
      "retry_after_seconds": 30
    }
  }
}
```

---

### tier_insufficient_reasoning

**Cause**: No backends meet minimum reasoning requirement

```json
{
  "error": {
    "message": "No backends meet minimum reasoning tier 8",
    "type": "insufficient_capacity",
    "code": 503,
    "context": {
      "rejection_reason": "tier_insufficient_reasoning",
      "required_tier": 8,
      "available_backends": [
        {
          "name": "local-ollama",
          "reasoning_tier": 6,
          "status": "healthy"
        }
      ],
      "retry_after_seconds": 30
    }
  }
}
```

---

### tier_insufficient_coding

**Cause**: No backends meet minimum coding requirement

```json
{
  "error": {
    "message": "No backends meet minimum coding tier 9",
    "type": "insufficient_capacity",
    "code": 503,
    "context": {
      "rejection_reason": "tier_insufficient_coding",
      "required_tier": 9,
      "available_backends": [
        {
          "name": "local-ollama",
          "coding_tier": 7,
          "status": "healthy"
        }
      ],
      "retry_after_seconds": 30
    }
  }
}
```

---

### overflow_blocked_with_history

**Cause**: Cross-zone overflow blocked due to conversation history

```json
{
  "error": {
    "message": "Cross-zone overflow blocked: request contains conversation history",
    "type": "insufficient_capacity",
    "code": 503,
    "context": {
      "rejection_reason": "overflow_blocked_with_history",
      "required_zone": "restricted",
      "available_zone": "open",
      "overflow_mode": "fresh-only",
      "conversation_messages": 5,
      "retry_after_seconds": 30
    }
  }
}
```

---

### strict_routing_enabled

**Cause**: X-Nexus-Strict header prevents alternatives

```json
{
  "error": {
    "message": "Requested model 'llama3:70b' unavailable and strict routing enabled",
    "type": "insufficient_capacity",
    "code": 503,
    "context": {
      "rejection_reason": "strict_routing_enabled",
      "requested_model": "llama3:70b",
      "routing_preference": "strict",
      "available_models": ["mistral:7b", "qwen2:72b"],
      "retry_after_seconds": 30
    }
  }
}
```

---

## HTTP Headers

### Retry-After

**Specification**: RFC 7231 Section 7.1.3  
**Format**: Integer (seconds)  
**Purpose**: Suggest when to retry the request

**Example**:
```http
HTTP/1.1 503 Service Unavailable
Content-Type: application/json
Retry-After: 30

{...error response...}
```

---

## Client Handling Examples

### TypeScript

```typescript
try {
  const response = await client.chat.completions.create({
    model: 'llama3:70b',
    messages: [{role: 'user', content: 'Hello'}],
  });
} catch (error) {
  if (error.status === 503) {
    const reason = error.context?.rejection_reason;
    const retryAfter = error.context?.retry_after_seconds || 30;
    
    switch (reason) {
      case 'privacy_zone_mismatch':
        console.error('Privacy zone constraint violated');
        // Don't retry - configuration issue
        break;
        
      case 'tier_insufficient_reasoning':
        console.warn(`Required tier ${error.context.required_tier} unavailable`);
        await sleep(retryAfter * 1000);
        // Retry with same request
        break;
        
      case 'strict_routing_enabled':
        console.warn('Exact model unavailable');
        // Option 1: Retry later
        // Option 2: Retry with X-Nexus-Flexible header
        break;
    }
  }
}
```

### Python

```python
try:
    response = client.chat.completions.create(
        model="llama3:70b",
        messages=[{"role": "user", "content": "Hello"}]
    )
except Exception as e:
    if e.status_code == 503:
        reason = e.context.get('rejection_reason')
        retry_after = e.context.get('retry_after_seconds', 30)
        
        if reason == 'privacy_zone_mismatch':
            print("Privacy zone constraint violated")
            # Configuration error - don't retry
        elif reason == 'tier_insufficient_reasoning':
            print(f"Required tier {e.context['required_tier']} unavailable")
            time.sleep(retry_after)
            # Retry with same request
        elif reason == 'strict_routing_enabled':
            print("Exact model unavailable")
            # Retry with flexible header or later
```

---

## Observability

### Metrics

```prometheus
# 503 responses by rejection reason
http_responses_total{status="503", reason="privacy_zone_mismatch"} 12
http_responses_total{status="503", reason="tier_insufficient_reasoning"} 45
```

### Logs

```json
{
  "timestamp": "2025-02-16T00:00:00Z",
  "level": "warn",
  "message": "Request rejected",
  "status": 503,
  "rejection_reason": "tier_insufficient_reasoning",
  "required_tier": 8,
  "available_backends": 2
}
```

---

**Contract Complete**: 503 error response format with structured context and client handling examples.
