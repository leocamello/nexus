# Data Model: Embeddings API

**Feature**: F17 - Embeddings API  
**Date**: 2025-02-17  
**Status**: Retrospective Documentation  

> **Note**: This document describes the data structures that were implemented for the embeddings API.

---

## Overview

The Embeddings API uses OpenAI-compatible data structures for request/response payloads. All types are defined in `src/api/embeddings.rs` and follow OpenAI's specification exactly to ensure drop-in compatibility with existing tools.

---

## Request Types

### EmbeddingInput (Enum)

**Purpose**: Represents the `input` field which can be either a single string or an array of strings.

**Definition** (`src/api/embeddings.rs`, lines 16-31):
```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    Single(String),
    Batch(Vec<String>),
}

impl EmbeddingInput {
    /// Convert to a Vec<String> regardless of variant.
    pub fn into_vec(self) -> Vec<String> {
        match self {
            EmbeddingInput::Single(s) => vec![s],
            EmbeddingInput::Batch(v) => v,
        }
    }
}
```

**Variants**:
- `Single(String)`: Represents `"input": "hello world"`
- `Batch(Vec<String>)`: Represents `"input": ["text1", "text2"]`

**Key Features**:
- `#[serde(untagged)]`: Allows transparent deserialization from JSON
- `into_vec()` method: Normalizes both variants to `Vec<String>` for processing

**Example Deserialization**:
```json
// Single variant
{"model": "...", "input": "hello"}  → EmbeddingInput::Single("hello")

// Batch variant
{"model": "...", "input": ["a", "b"]}  → EmbeddingInput::Batch(vec!["a", "b"])
```

### EmbeddingRequest (Struct)

**Purpose**: Represents the complete request payload for `POST /v1/embeddings`.

**Definition** (`src/api/embeddings.rs`, lines 33-40):
```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddingRequest {
    pub model: String,
    pub input: EmbeddingInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding_format: Option<String>,
}
```

**Fields**:
| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `model` | `String` | Yes | Model identifier (e.g., "text-embedding-ada-002") |
| `input` | `EmbeddingInput` | Yes | Text to embed (single or batch) |
| `encoding_format` | `Option<String>` | No | "float" or "base64" (parsed but not enforced) |

**Example JSON**:
```json
{
  "model": "text-embedding-ada-002",
  "input": "The quick brown fox",
  "encoding_format": "float"
}
```

**Validation**:
- Non-empty input: Checked in handler (not type system)
- Model existence: Validated by Router
- Encoding format: Accepted but not enforced (always returns float)

---

## Response Types

### EmbeddingObject (Struct)

**Purpose**: Represents a single embedding result in the response.

**Definition** (`src/api/embeddings.rs`, lines 42-48):
```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddingObject {
    pub object: String,
    pub embedding: Vec<f32>,
    pub index: usize,
}
```

**Fields**:
| Field | Type | Description |
|-------|------|-------------|
| `object` | `String` | Always "embedding" (OpenAI standard) |
| `embedding` | `Vec<f32>` | Vector representation (1536 floats for ada-002) |
| `index` | `usize` | Position in batch (0 for single input) |

**Example JSON**:
```json
{
  "object": "embedding",
  "embedding": [0.0023, -0.0091, 0.0062, ...],  // 1536 dimensions
  "index": 0
}
```

**Dimensions by Model** (OpenAI):
- `text-embedding-ada-002`: 1536 dimensions
- `text-embedding-3-small`: 512-1536 dimensions (configurable)
- `text-embedding-3-large`: 256-3072 dimensions (configurable)

### EmbeddingUsage (Struct)

**Purpose**: Reports token consumption for the request.

**Definition** (`src/api/embeddings.rs`, lines 50-55):
```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddingUsage {
    pub prompt_tokens: u32,
    pub total_tokens: u32,
}
```

**Fields**:
| Field | Type | Description |
|-------|------|-------------|
| `prompt_tokens` | `u32` | Number of tokens in input text |
| `total_tokens` | `u32` | Total tokens (equals prompt_tokens for embeddings) |

**Example JSON**:
```json
{
  "prompt_tokens": 15,
  "total_tokens": 15
}
```

**Token Calculation**:
- Routing: Estimated as `sum(input.len() / 4 for input in inputs)`
- Response: Uses estimated tokens (backend doesn't always report actual)
- Note: Embeddings have no "completion tokens" (only input processing)

### EmbeddingResponse (Struct)

**Purpose**: Complete response payload matching OpenAI format.

**Definition** (`src/api/embeddings.rs`, lines 57-64):
```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddingResponse {
    pub object: String,
    pub data: Vec<EmbeddingObject>,
    pub model: String,
    pub usage: EmbeddingUsage,
}
```

**Fields**:
| Field | Type | Description |
|-------|------|-------------|
| `object` | `String` | Always "list" (OpenAI standard) |
| `data` | `Vec<EmbeddingObject>` | Array of embedding results |
| `model` | `String` | Model used for generation |
| `usage` | `EmbeddingUsage` | Token consumption |

**Example JSON** (single input):
```json
{
  "object": "list",
  "data": [
    {
      "object": "embedding",
      "embedding": [0.0023, -0.0091, ...],
      "index": 0
    }
  ],
  "model": "text-embedding-ada-002",
  "usage": {
    "prompt_tokens": 5,
    "total_tokens": 5
  }
}
```

**Example JSON** (batch input):
```json
{
  "object": "list",
  "data": [
    {
      "object": "embedding",
      "embedding": [0.0023, -0.0091, ...],
      "index": 0
    },
    {
      "object": "embedding",
      "embedding": [0.0034, -0.0012, ...],
      "index": 1
    },
    {
      "object": "embedding",
      "embedding": [0.0045, -0.0023, ...],
      "index": 2
    }
  ],
  "model": "text-embedding-ada-002",
  "usage": {
    "prompt_tokens": 15,
    "total_tokens": 15
  }
}
```

---

## Agent Interface Types

### InferenceAgent::embeddings() Method

**Purpose**: Trait method for agents to implement embedding generation.

**Signature** (`src/agent/mod.rs`, line 213):
```rust
async fn embeddings(&self, _input: Vec<String>) -> Result<Vec<Vec<f32>>, AgentError>
```

**Parameters**:
- `input`: `Vec<String>` — Texts to embed (pre-normalized from `EmbeddingInput`)

**Returns**:
- `Ok(Vec<Vec<f32>>)`: Array of embedding vectors (one per input)
- `Err(AgentError)`: Error during generation

**Error Types**:
- `AgentError::Unsupported`: Backend doesn't support embeddings
- `AgentError::Timeout`: Request exceeded timeout (60s)
- `AgentError::Network`: Connection failure
- `AgentError::Upstream`: Backend returned error (status, message)
- `AgentError::InvalidResponse`: Malformed response from backend

**Default Implementation**:
```rust
async fn embeddings(&self, _input: Vec<String>) -> Result<Vec<Vec<f32>>, AgentError> {
    Err(AgentError::Unsupported("embeddings"))
}
```

**Implementations**:
- `OpenAIAgent`: Forwards to `/v1/embeddings`, returns vectors
- `OllamaAgent`: Iterates `/api/embed`, collects vectors
- `LMStudioAgent`: Uses default (Unsupported)
- `GenericAgent`: Uses default (Unsupported)

---

## Agent Capability Types

### AgentCapabilities::embeddings Field

**Purpose**: Declares whether an agent supports embedding generation.

**Definition** (`src/agent/types.rs`, line 40):
```rust
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCapabilities {
    /// Supports /v1/embeddings endpoint.
    pub embeddings: bool,
    
    // ... other capabilities ...
}
```

**Usage**:
- Set by agents in their `profile()` implementation
- Checked by Router to filter capable backends
- Checked by handler to ensure backend supports embeddings

**Values by Agent**:
| Agent | embeddings | Rationale |
|-------|-----------|-----------|
| OpenAI | `true` | Native `/v1/embeddings` support |
| Ollama | `true` | Has `/api/embed` endpoint |
| LMStudio | `false` | Not implemented (could be added) |
| Generic | `false` | Unknown capability (opt-in via config) |

---

## Internal Processing Types

### RequestRequirements (for Routing)

**Purpose**: Passed to Router for backend selection.

**Usage** (`src/api/embeddings.rs`, lines 87-94):
```rust
let requirements = RequestRequirements {
    model: request.model.clone(),
    estimated_tokens,
    needs_vision: false,
    needs_tools: false,
    needs_json_mode: false,
    prefers_streaming: false,
};
```

**Fields Used**:
- `model`: Model name from request
- `estimated_tokens`: Sum of input lengths / 4
- `needs_vision`: Always false (embeddings are text-only)
- `needs_tools`: Always false (no tool calling)
- `needs_json_mode`: Always false (embeddings output is structured)
- `prefers_streaming`: Always false (embeddings are atomic)

---

## Error Types

### ApiError (HTTP Errors)

**Relevant Variants** (`src/api/error.rs`):

```rust
// Empty input
ApiError::bad_request("Input must not be empty")
// → 400 Bad Request

// Model not found
ApiError::model_not_found(&model, &[])
// → 404 Not Found

// No capable backends
ApiError::service_unavailable("No healthy backend available")
// → 503 Service Unavailable

// Agent not registered
ApiError::bad_gateway("No agent registered for backend")
// → 502 Bad Gateway

// Backend error
ApiError::from_agent_error(agent_error)
// → 502 Bad Gateway (with backend error message)
```

**OpenAI Error Format**:
```json
{
  "error": {
    "message": "Model 'nonexistent-model' not found",
    "type": "invalid_request_error",
    "code": "model_not_found"
  }
}
```

---

## Type Relationships

### Data Flow

```text
Client Request (JSON)
    ↓
EmbeddingRequest (API type)
    ↓
EmbeddingInput.into_vec() → Vec<String>
    ↓
Router → RequestRequirements
    ↓
Agent.embeddings(Vec<String>) → Vec<Vec<f32>>
    ↓
Vec<Vec<f32>>.enumerate() → Vec<EmbeddingObject>
    ↓
EmbeddingResponse (API type)
    ↓
Client Response (JSON)
```

### Type Dependencies

```text
EmbeddingRequest
    ├── model: String
    ├── input: EmbeddingInput
    │   ├── Single(String)
    │   └── Batch(Vec<String>)
    └── encoding_format: Option<String>

EmbeddingResponse
    ├── object: String
    ├── data: Vec<EmbeddingObject>
    │   ├── object: String
    │   ├── embedding: Vec<f32>
    │   └── index: usize
    ├── model: String
    └── usage: EmbeddingUsage
        ├── prompt_tokens: u32
        └── total_tokens: u32
```

---

## Backend-Specific Formats

### OpenAI Backend Format

**Request to OpenAI** (`src/agent/openai.rs`, line 361):
```json
{
  "model": "text-embedding-ada-002",
  "input": ["text1", "text2"]
}
```

**Response from OpenAI**:
```json
{
  "object": "list",
  "data": [
    {
      "object": "embedding",
      "embedding": [0.1, 0.2, ...],
      "index": 0
    }
  ],
  "model": "text-embedding-ada-002",
  "usage": {
    "prompt_tokens": 10,
    "total_tokens": 10
  }
}
```

**Transformation**: None (already OpenAI format) — pass through directly

### Ollama Backend Format

**Request to Ollama** (`src/agent/ollama.rs`, line 297):
```json
{
  "model": "all-minilm",
  "input": "single text"
}
```

**Response from Ollama**:
```json
{
  "embeddings": [[0.1, 0.2, 0.3, ...]]
}
```

**Transformation** (in Nexus):
1. Extract `body["embeddings"][0]` → `Vec<f32>`
2. Collect all vectors from iteration
3. Enumerate with index
4. Build `EmbeddingObject` for each
5. Wrap in `EmbeddingResponse` with usage stats

---

## Validation Rules

### Request Validation

| Rule | Location | Error |
|------|----------|-------|
| Input not empty | Handler line 79 | 400 Bad Request |
| Valid JSON | Axum middleware | 422 Unprocessable Entity |
| Model exists | Router | 404 Not Found |
| Backend available | Router | 503 Service Unavailable |
| Agent registered | Handler line 115 | 502 Bad Gateway |
| Embeddings supported | Handler line 120 | 503 Service Unavailable |

### Response Validation

**Guaranteed Properties**:
- `object` always "list"
- `data` has same length as input array
- `index` values are 0..(length-1)
- `embedding` vectors are non-empty
- `prompt_tokens` equals `total_tokens` (no completion tokens)

**Not Validated** (passes through backend):
- Embedding dimensions (assumed correct for model)
- Vector normalization (backend-specific)
- Embedding value ranges (unconstrained floats)

---

## Memory Characteristics

### Size Estimates

**Request**:
- Single input: ~100 bytes (model name + text)
- Batch (10 inputs): ~500 bytes (10x text + overhead)

**Response**:
- Single embedding (1536 dims): ~6KB (1536 floats × 4 bytes)
- Batch (10 embeddings): ~60KB (10 × 6KB)
- JSON overhead: ~10% (field names, brackets)

**In-Flight Memory**:
- Request parsing: ~1KB
- Vec<String> conversion: ~1KB
- Agent call: ~1KB (serialization)
- Vector storage: ~6KB per embedding
- Response building: ~6KB per embedding

**Total for Batch of 10**: ~120KB peak memory

---

## Testing Types

### Unit Test Coverage

**Type Serialization** (8 tests in `src/api/embeddings.rs`):
- `embedding_request_deserialize_single_input`: Single string parsing
- `embedding_request_deserialize_batch_input`: Array parsing
- `embedding_request_with_encoding_format`: Optional field
- `embedding_input_into_vec_single`: Conversion
- `embedding_input_into_vec_batch`: Conversion
- `embedding_response_serialization_matches_openai`: Format compliance
- `embedding_response_roundtrip`: Serialize/deserialize
- `embedding_object_serialization`: Large vector handling

### Mock Types

**Integration Tests** (`tests/embeddings_test.rs`):
- Uses `wiremock::MockServer` to simulate backends
- Mock response format matches OpenAI structure
- Tests validate end-to-end type flow

---

## References

**Type Definitions**:
- `src/api/embeddings.rs`: Request/response types
- `src/agent/mod.rs`: Agent trait interface
- `src/agent/types.rs`: Agent capabilities
- `src/api/error.rs`: Error types

**OpenAI Specification**:
- https://platform.openai.com/docs/api-reference/embeddings

**Related Data Models**:
- Chat Completions: `src/api/completions.rs`
- Models List: `src/api/models.rs`

---

**Document Version**: 1.0  
**Created**: 2025-02-17  
**Type**: Retrospective Data Model Documentation
