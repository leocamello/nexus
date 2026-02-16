# Configuration Contract: TrafficPolicy TOML

**Feature**: Privacy Zones & Capability Tiers  
**Version**: 1.0  
**Date**: 2025-02-16

## Overview

TrafficPolicies define privacy and capability requirements for specific route patterns. They are configured in `nexus.toml` under `[routing.policies]` sections.

---

## Configuration Schema

### Basic Structure

```toml
[routing.policies."<pattern>"]
privacy = "<zone>"              # Optional: "restricted" | "open"
overflow_mode = "<mode>"         # Optional: "block-entirely" | "fresh-only"
min_reasoning = <0-10>           # Optional: minimum reasoning score
min_coding = <0-10>              # Optional: minimum coding score
min_context_window = <tokens>    # Optional: minimum context window
vision_required = <bool>         # Optional: requires vision capability
tools_required = <bool>          # Optional: requires tools capability
```

---

## Examples

### Example 1: Code Generation Routes (Strict Privacy)

```toml
[routing.policies."code-*"]
privacy = "restricted"           # Only local backends
min_reasoning = 7                # Strong reasoning required
min_coding = 8                   # Strong coding required
overflow_mode = "block-entirely" # Never overflow to cloud
```

**Matches**: `code-llama`, `code-generation`, `code-review`  
**Behavior**: Rejects cloud backends, requires high coding scores, blocks all overflow

---

### Example 2: General Chat Routes (Flexible)

```toml
[routing.policies."chat-*"]
min_reasoning = 5                # Moderate reasoning
overflow_mode = "fresh-only"     # Allow fresh conversations to cloud
```

**Matches**: `chat-gpt`, `chat-assistant`, `chatbot-v2`  
**Behavior**: Allows cloud overflow for fresh conversations only

---

### Example 3: Vision Tasks

```toml
[routing.policies."vision-*"]
vision_required = true           # Must support image inputs
min_reasoning = 6                # Moderate reasoning
```

**Matches**: `vision-llama`, `vision-qwen`  
**Behavior**: Filters backends without vision support

---

### Example 4: Production API (High Tier)

```toml
[routing.policies."prod-*"]
privacy = "restricted"
min_reasoning = 9
min_coding = 9
min_context_window = 128000
overflow_mode = "block-entirely"
```

**Matches**: `prod-api`, `prod-inference`  
**Behavior**: Premium tier, local-only, large context window

---

## Pattern Matching

### Glob Syntax

- `*`: Match any characters within a segment
- `**`: Match any characters across segments (not commonly needed for model names)
- `?`: Match a single character
- `[abc]`: Match one of the characters a, b, or c

### Priority Ordering

When multiple patterns match, the most specific pattern wins:

1. **Exact match** (no wildcards): Priority 100
2. **Prefix match** (`code-*`): Priority 50
3. **Suffix match** (`*-vision`): Priority 50
4. **Wildcard match** (`*`): Priority 10

**Example**:
```toml
[routing.policies."llama3:70b"]  # Priority 100 (exact)
min_reasoning = 9

[routing.policies."llama3*"]     # Priority 50 (prefix)
min_reasoning = 7

[routing.policies."*"]           # Priority 10 (wildcard)
min_reasoning = 5
```

**Match for `llama3:70b`**: Uses exact match policy (reasoning=9)  
**Match for `llama3:8b`**: Uses prefix match policy (reasoning=7)  
**Match for `mistral:7b`**: Uses wildcard policy (reasoning=5)

---

## Field Reference

### privacy

**Type**: String (enum)  
**Values**: `"restricted"` | `"open"`  
**Default**: Backend default zone  
**Description**: Required privacy zone for backends serving this route

**Example**:
```toml
[routing.policies."sensitive-*"]
privacy = "restricted"  # Only local backends
```

---

### overflow_mode

**Type**: String (enum)  
**Values**: `"block-entirely"` | `"fresh-only"`  
**Default**: `"block-entirely"`  
**Description**: Cross-zone overflow behavior

- `"block-entirely"`: Never allow overflow to different privacy zone
- `"fresh-only"`: Allow overflow only for requests with no conversation history

**Example**:
```toml
[routing.policies."chat-*"]
overflow_mode = "fresh-only"  # Allow new conversations to cloud
```

---

### min_reasoning

**Type**: Integer  
**Range**: 0-10  
**Default**: No requirement  
**Description**: Minimum reasoning capability score

**Example**:
```toml
[routing.policies."analysis-*"]
min_reasoning = 8  # Strong analytical reasoning
```

---

### min_coding

**Type**: Integer  
**Range**: 0-10  
**Default**: No requirement  
**Description**: Minimum coding capability score

**Example**:
```toml
[routing.policies."code-*"]
min_coding = 9  # Expert-level coding
```

---

### min_context_window

**Type**: Integer  
**Unit**: Tokens  
**Default**: No requirement  
**Description**: Minimum context window size

**Example**:
```toml
[routing.policies."document-*"]
min_context_window = 128000  # Large documents
```

---

### vision_required

**Type**: Boolean  
**Default**: `false`  
**Description**: Requires vision/image input support

**Example**:
```toml
[routing.policies."image-*"]
vision_required = true
```

---

### tools_required

**Type**: Boolean  
**Default**: `false`  
**Description**: Requires tool/function calling support

**Example**:
```toml
[routing.policies."agent-*"]
tools_required = true
```

---

## Validation Rules

### Config Load Time

1. **Pattern syntax**: Must be valid glob pattern
2. **Score ranges**: min_reasoning, min_coding must be 0-10
3. **Context window**: Must be positive integer if specified
4. **Enum values**: privacy, overflow_mode must match allowed values

### Runtime Behavior

1. **No matching policy**: Use backend defaults (no enforcement)
2. **Multiple matching policies**: Use highest priority pattern
3. **Partial requirements**: Only specified fields are enforced
4. **Unknown fields**: Ignored (forward compatibility)

---

## Complete Configuration Example

```toml
# nexus.toml

[routing]
strategy = "smart"

# Code generation: strict local-only, high capability
[routing.policies."code-*"]
privacy = "restricted"
min_reasoning = 7
min_coding = 8
overflow_mode = "block-entirely"

# General chat: allow cloud overflow for fresh conversations
[routing.policies."chat-*"]
min_reasoning = 5
overflow_mode = "fresh-only"

# Vision tasks: require image support
[routing.policies."vision-*"]
vision_required = true
min_reasoning = 6

# Production API: premium tier, local-only
[routing.policies."prod-*"]
privacy = "restricted"
min_reasoning = 9
min_coding = 9
min_context_window = 128000
overflow_mode = "block-entirely"

# Default policy for all models
[routing.policies."*"]
min_reasoning = 5  # Baseline quality
```

---

## Configuration Hot-Reload

**Behavior**: Changes to TrafficPolicies take effect within one configuration refresh cycle (typically 5-30 seconds).

**Safe Changes**:
- Adding new policies
- Modifying capability requirements
- Changing overflow modes

**Unsafe Changes** (require restart):
- Changing backend privacy zones
- Modifying backend capability tiers

---

## Testing Configuration

### Validation Tool

```bash
# Validate configuration without starting server
nexus validate-config nexus.toml
```

**Output**:
```
✓ Routing configuration valid
✓ 5 traffic policies loaded
  - code-* (priority 50)
  - chat-* (priority 50)
  - vision-* (priority 50)
  - prod-* (priority 50)
  - * (priority 10)
✓ No circular dependencies
✓ All patterns compilable
```

---

## Observability

### Metrics

```prometheus
# Policy application rate
traffic_policy_applied_total{pattern="code-*"} 1234

# Policy rejection rate
traffic_policy_rejected_total{pattern="code-*", reason="tier_insufficient"} 56
```

### Logs

```json
{
  "timestamp": "2025-02-16T00:00:00Z",
  "level": "info",
  "message": "Traffic policy applied",
  "pattern": "code-*",
  "model": "code-llama",
  "privacy": "restricted",
  "min_reasoning": 7
}
```

---

**Contract Complete**: TrafficPolicy configuration schema, examples, validation rules, and observability defined.
