# Data Model: Structured Request Logging

**Feature**: F11 Structured Request Logging  
**Date**: 2025-02-14  
**Phase**: 1 - Design

## Overview

This document defines the data structures and field mappings for structured request logging in Nexus. All log entries are emitted as tracing spans with structured fields, formatted as JSON or human-readable output depending on configuration.

## Core Entities

### 1. RequestLogEntry

Represents a single structured log entry for a request. **Not a Rust struct** - these are fields attached to tracing spans.

**Lifecycle**: Created when request enters completions handler, fields populated during processing, emitted when request completes (success or failure).

**Span Hierarchy**:
```
request_span (INFO level)
  ├─ routing_span (DEBUG level)  
  ├─ backend_attempt_span (DEBUG level, repeated for retries)
  └─ response_span (INFO level)
```

#### Fields

| Field Name | Type | Required | Description | Example | Source |
|------------|------|----------|-------------|---------|--------|
| `timestamp` | RFC3339 string | Yes | Request start time in UTC | `"2024-01-15T14:32:01.123Z"` | Automatic (tracing) |
| `level` | String | Yes | Log level | `"INFO"`, `"WARN"`, `"ERROR"` | Automatic (tracing) |
| `target` | String | Yes | Module path | `"nexus::api::completions"` | Automatic (tracing) |
| `request_id` | UUID string | Yes | Unique correlation ID | `"550e8400-e29b-41d4-a716-446655440000"` | Generated (uuid v4) |
| `model` | String | Yes | Requested model name | `"gpt-4"`, `"llama3:70b"` | Request body |
| `actual_model` | String | Conditional | Resolved model (if alias or fallback) | `"llama3:70b"` | Router result |
| `backend` | String | Conditional | Selected backend ID | `"ollama-local"`, `"none"` | Router result |
| `backend_type` | String | Conditional | Backend category | `"local"`, `"cloud"` | Backend config |
| `status` | String | Yes | Request outcome | `"success"`, `"error"`, `"timeout"` | Response status |
| `status_code` | u16 | Conditional | HTTP status code | `200`, `503`, `500` | HTTP response |
| `error_message` | String | Conditional | Error description (if failed) | `"Backend timeout after 30s"` | Error context |
| `latency_ms` | u64 | Yes | Total request duration | `1234` (milliseconds) | Measured (Instant) |
| `tokens_prompt` | u32 | Conditional | Input token count | `150` | Backend response |
| `tokens_completion` | u32 | Conditional | Output token count | `85` | Backend response |
| `tokens_total` | u32 | Conditional | Total tokens used | `235` | Calculated |
| `stream` | bool | Yes | Streaming mode indicator | `true`, `false` | Request body |
| `route_reason` | String | Conditional | Backend selection rationale | `"highest_score:0.95"` | Router decision |
| `retry_count` | u32 | Yes | Number of retry attempts | `0`, `1`, `2` | Retry logic |
| `fallback_chain` | String | Conditional | Ordered list of backends tried | `"ollama-local,vllm-remote"` | Retry history |

**Required Fields**: Always present in every log entry  
**Conditional Fields**: Present when applicable (e.g., backend only if routing succeeds, tokens only for successful completions)

#### Field Population Timeline

```
1. Request Entry:
   ├─ request_id: Generated (uuid::Uuid::new_v4())
   ├─ model: From request.model
   ├─ stream: From request.stream
   └─ timestamp: Automatic (span creation)

2. Routing Decision:
   ├─ backend: From RoutingResult.backend.id
   ├─ backend_type: From Backend.backend_type
   ├─ actual_model: From RoutingResult.actual_model
   ├─ route_reason: From RoutingResult.route_reason
   └─ retry_count: From retry loop iteration

3. Request Completion:
   ├─ status: "success" | "error" | "timeout"
   ├─ status_code: HTTP response code
   ├─ latency_ms: start_time.elapsed().as_millis()
   ├─ tokens_prompt: From backend response usage field
   ├─ tokens_completion: From backend response usage field
   ├─ tokens_total: tokens_prompt + tokens_completion
   ├─ error_message: If status != "success"
   └─ fallback_chain: Comma-separated backend IDs if retries occurred
```

### 2. LoggingConfig (Extended)

Existing struct in `src/config/logging.rs`, extended with new fields.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    /// Global log level threshold (e.g., "info", "debug", "warn")
    pub level: String,
    
    /// Output format (pretty-printed or JSON)
    pub format: LogFormat,  // enum: Pretty | Json
    
    /// Component-specific log levels (NEW)
    /// Example: {"routing": "debug", "api": "info", "health": "warn"}
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_levels: Option<HashMap<String, String>>,
    
    /// Enable debug content logging (opt-in, defaults to false) (NEW)
    /// When true, request/response message content is logged
    #[serde(default)]
    pub enable_content_logging: bool,
}
```

**Defaults**:
- `level`: `"info"`
- `format`: `LogFormat::Pretty`
- `component_levels`: `None` (no component-specific overrides)
- `enable_content_logging`: `false` (privacy-safe by default)

### 3. RoutingResult (Extended)

Existing struct in `src/routing/mod.rs`, extended with routing decision context.

```rust
#[derive(Debug)]
pub struct RoutingResult {
    /// The selected backend
    pub backend: Arc<Backend>,
    
    /// The actual model name used (may differ if fallback)
    pub actual_model: String,
    
    /// True if a fallback model was used
    pub fallback_used: bool,
    
    /// Explanation of backend selection decision (NEW)
    /// Examples:
    /// - "highest_score:0.95"
    /// - "round_robin:index_3"
    /// - "fallback:primary_unhealthy"
    /// - "only_healthy_backend"
    pub route_reason: String,
}
```

## State Transitions

### Request Status State Machine

```
                 ┌─────────────┐
                 │   received  │ (Initial: request enters handler)
                 └──────┬──────┘
                        │
                        ▼
                 ┌─────────────┐
                 │   routing   │ (Router selecting backend)
                 └──────┬──────┘
                        │
         ┌──────────────┼──────────────┐
         ▼              ▼              ▼
   ┌─────────┐   ┌──────────┐   ┌──────────┐
   │ success │   │  error   │   │  retry   │
   └─────────┘   └──────────┘   └────┬─────┘
                                      │
                                      │ (Loop back to routing)
                                      ▼
                               ┌──────────────┐
                               │ fallback     │ (Try next backend)
                               └──────┬───────┘
                                      │
                         ┌────────────┼────────────┐
                         ▼            ▼            ▼
                   ┌─────────┐ ┌──────────┐ ┌──────────┐
                   │ success │ │  error   │ │exhausted │
                   └─────────┘ └──────────┘ └──────────┘
```

**Status Field Values**:
- `"received"`: Request received, not yet routed
- `"routing"`: Selecting backend
- `"success"`: Request completed successfully (status_code 200)
- `"error"`: Request failed with error (status_code 400, 500)
- `"retry"`: Attempt failed, retrying with same backend
- `"fallback"`: Attempt failed, trying next backend in chain
- `"exhausted"`: All retry/fallback attempts exhausted (status_code 503)
- `"timeout"`: Request exceeded timeout threshold

## Validation Rules

### Field Constraints

1. **request_id**: Must be valid UUID v4 format
2. **model**: Non-empty string, max 128 characters
3. **latency_ms**: Non-negative integer, max 300000 (5 minutes timeout)
4. **tokens_***: Non-negative integers, max 1000000 (reasonable context limit)
5. **retry_count**: Non-negative integer, max 10 (configurable retry limit)
6. **status_code**: Valid HTTP status code (100-599)
7. **timestamp**: RFC3339 format with timezone (must be UTC)

### Conditional Field Requirements

| Status | Required Fields | Optional Fields |
|--------|----------------|-----------------|
| `"success"` | request_id, model, backend, latency_ms, status_code=200 | tokens_*, route_reason |
| `"error"` | request_id, model, status_code, error_message, latency_ms | backend, retry_count |
| `"exhausted"` | request_id, model, fallback_chain, retry_count, latency_ms | - |
| `"timeout"` | request_id, model, backend, latency_ms | retry_count |

### Sentinel Values

When a field is not applicable, use these sentinel values:

| Field | Sentinel Value | Meaning |
|-------|---------------|---------|
| `backend` | `"none"` | No backend selected (routing failed) |
| `actual_model` | Same as `model` | No alias/fallback resolution |
| `tokens_*` | Field omitted | Token counting not available |
| `error_message` | Field omitted | No error occurred |
| `fallback_chain` | Empty string `""` | No fallbacks attempted |

## JSON Schema Example

See `contracts/log-schema.json` for complete JSON Schema definition.

**Example Log Entry (Success)**:
```json
{
  "timestamp": "2024-01-15T14:32:01.234Z",
  "level": "INFO",
  "target": "nexus::api::completions",
  "request_id": "550e8400-e29b-41d4-a716-446655440000",
  "model": "gpt-4",
  "actual_model": "llama3:70b",
  "backend": "ollama-local",
  "backend_type": "local",
  "status": "success",
  "status_code": 200,
  "latency_ms": 1234,
  "tokens_prompt": 150,
  "tokens_completion": 85,
  "tokens_total": 235,
  "stream": false,
  "route_reason": "highest_score:0.95",
  "retry_count": 0,
  "fallback_chain": ""
}
```

**Example Log Entry (Retry with Fallback)**:
```json
{
  "timestamp": "2024-01-15T14:32:05.678Z",
  "level": "WARN",
  "target": "nexus::api::completions",
  "request_id": "550e8400-e29b-41d4-a716-446655440000",
  "model": "gpt-4",
  "backend": "vllm-remote",
  "backend_type": "cloud",
  "status": "success",
  "status_code": 200,
  "latency_ms": 5432,
  "tokens_prompt": 150,
  "tokens_completion": 85,
  "tokens_total": 235,
  "stream": false,
  "route_reason": "fallback:primary_unhealthy",
  "retry_count": 2,
  "fallback_chain": "ollama-local,vllm-remote",
  "error_message": "Primary backend timeout after 30s"
}
```

**Example Log Entry (Error - No Backend)**:
```json
{
  "timestamp": "2024-01-15T14:32:10.123Z",
  "level": "ERROR",
  "target": "nexus::api::completions",
  "request_id": "550e8400-e29b-41d4-a716-446655440001",
  "model": "unknown-model",
  "backend": "none",
  "status": "error",
  "status_code": 404,
  "latency_ms": 12,
  "stream": false,
  "retry_count": 0,
  "fallback_chain": "",
  "error_message": "Model 'unknown-model' not found. Available: llama3:70b, gpt-3.5-turbo"
}
```

## Relationships

```
┌─────────────────┐
│ NexusConfig     │
└────────┬────────┘
         │ contains
         ▼
┌─────────────────┐
│ LoggingConfig   │
│ - level         │
│ - format        │
│ - component_    │
│   levels        │
│ - enable_       │
│   content_      │
│   logging       │
└─────────────────┘

┌─────────────────┐
│ Router          │
└────────┬────────┘
         │ returns
         ▼
┌─────────────────┐       ┌─────────────────┐
│ RoutingResult   │───────│ Backend         │
│ - backend       │ refs  │ - id            │
│ - actual_model  │       │ - backend_type  │
│ - fallback_used │       └─────────────────┘
│ - route_reason  │
└────────┬────────┘
         │ populates
         ▼
┌─────────────────┐
│ RequestLogEntry │
│ (tracing span)  │
│ - All fields    │
└─────────────────┘
```

## Implementation Notes

1. **No New Structs**: `RequestLogEntry` is a conceptual entity, not a Rust struct. All fields are attached to tracing spans using `#[instrument]` and `span::record()`.

2. **Field Extraction**: Helper functions in `src/logging/fields.rs` extract values from request/response objects:
   ```rust
   pub fn extract_tokens(response: &ChatCompletionResponse) -> (u32, u32, u32)
   pub fn extract_status(result: &Result<Response, ApiError>) -> (String, Option<String>)
   ```

3. **Timestamp Handling**: Tracing automatically adds timestamps. No manual timestamp management needed.

4. **Privacy**: By default, `request.messages` content is never logged. When `enable_content_logging = true`, add span field `prompt_preview` (first 100 chars).

5. **Performance**: All field recording uses `span::record()` which is zero-cost when logging is disabled. Total overhead budget: <1ms per request.
