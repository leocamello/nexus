# API Contract: Request Headers

**Feature**: Privacy Zones & Capability Tiers  
**Version**: 1.0  
**Date**: 2025-02-16

## Overview

This contract defines HTTP request headers for controlling routing behavior in Nexus. These headers are **optional** and provide client control over tier-equivalent model substitution during failover scenarios.

---

## Request Headers

### X-Nexus-Strict

**Purpose**: Enforce strict routing - only use the exact requested model, reject all alternatives.

**Format**:
```http
X-Nexus-Strict: true
```

**Values**:
- `true`: Enable strict mode (reject all alternatives)
- Omit header: Default to strict mode (same as `true`)

**Behavior**:
- When the requested model is unavailable, return 503 immediately
- No tier-equivalent alternatives considered
- No fallback chains evaluated
- Privacy zone constraints still apply (never flexible)

**Example Request**:
```http
POST /v1/chat/completions HTTP/1.1
Host: localhost:8000
Content-Type: application/json
X-Nexus-Strict: true

{
  "model": "llama3:70b",
  "messages": [
    {"role": "user", "content": "Write a function"}
  ]
}
```

---

### X-Nexus-Flexible

**Purpose**: Allow tier-equivalent model alternatives when primary model unavailable.

**Format**:
```http
X-Nexus-Flexible: true
```

**Values**:
- `true`: Enable flexible mode (allow tier-equivalent alternatives)
- Omit header: Default to strict mode

**Behavior**:
- When the requested model is unavailable, consider tier-equivalent alternatives
- Backend must meet **same-or-higher** capability scores on all dimensions
- Never downgrade (lower-tier alternatives still rejected)
- Privacy zone constraints still apply (never flexible)

---

## Client Integration Examples

### TypeScript (OpenAI SDK)

```typescript
import OpenAI from 'openai';

const client = new OpenAI({
  baseURL: 'http://localhost:8000/v1',
  apiKey: 'not-used',
});

// Flexible mode (allow alternatives)
const response = await client.chat.completions.create({
  model: 'llama3:70b',
  messages: [{role: 'user', content: 'Hello'}],
}, {
  headers: {
    'X-Nexus-Flexible': 'true',
  },
});
```

### Python (OpenAI SDK)

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:8000/v1",
    api_key="not-used",
)

# Flexible mode
response = client.chat.completions.create(
    model="llama3:70b",
    messages=[{"role": "user", "content": "Hello"}],
    extra_headers={"X-Nexus-Flexible": "true"},
)
```

---

**Contract Complete**: Request headers defined with behavior and client integration examples.
