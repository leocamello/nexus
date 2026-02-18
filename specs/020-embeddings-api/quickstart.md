# Quickstart: Embeddings API

**Feature**: F17 - Embeddings API  
**Date**: 2025-02-17  
**Status**: Retrospective Documentation  

> **Note**: This guide shows how to use the implemented embeddings API with real examples.

---

## Overview

The Nexus Embeddings API provides OpenAI-compatible text embedding generation through a unified endpoint. It routes requests to capable backends (OpenAI, Ollama) and returns vector representations of text for use in similarity search, clustering, classification, and other ML applications.

**Endpoint**: `POST /v1/embeddings`  
**Compatibility**: OpenAI Embeddings API v1  
**Supported Backends**: OpenAI, Ollama  

---

## Prerequisites

### Backend Setup

**Option 1: OpenAI Backend**
```bash
# Ensure Nexus is configured with OpenAI backend
# Set API key in environment or config file
export OPENAI_API_KEY="sk-..."

# Start Nexus
./nexus serve
```

**Option 2: Ollama Backend**
```bash
# Install and start Ollama
ollama serve

# Pull an embedding model
ollama pull all-minilm
# Or: ollama pull nomic-embed-text

# Start Nexus (auto-discovers Ollama via mDNS)
./nexus serve
```

### Verify Backend Availability

```bash
# List available models
curl http://localhost:7777/v1/models | jq .

# Check for embedding-capable models
curl http://localhost:7777/v1/models | jq '.data[] | select(.id | contains("embedding"))'
```

---

## Basic Usage

### Single Text Embedding

**cURL**:
```bash
curl -X POST http://localhost:7777/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{
    "model": "text-embedding-ada-002",
    "input": "The quick brown fox jumps over the lazy dog"
  }'
```

**Response**:
```json
{
  "object": "list",
  "data": [
    {
      "object": "embedding",
      "embedding": [0.0023, -0.0091, 0.0062, ...],
      "index": 0
    }
  ],
  "model": "text-embedding-ada-002",
  "usage": {
    "prompt_tokens": 9,
    "total_tokens": 9
  }
}
```

### Batch Text Embedding

**cURL**:
```bash
curl -X POST http://localhost:7777/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{
    "model": "text-embedding-ada-002",
    "input": [
      "First text to embed",
      "Second text to embed",
      "Third text to embed"
    ]
  }'
```

**Response**:
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

## Client Libraries

### Python (OpenAI SDK)

**Installation**:
```bash
pip install openai
```

**Single Embedding**:
```python
from openai import OpenAI

# Point to Nexus instead of OpenAI
client = OpenAI(
    base_url="http://localhost:7777/v1",
    api_key="not-needed"  # Nexus is local-first, no auth required
)

response = client.embeddings.create(
    model="text-embedding-ada-002",
    input="Hello, world!"
)

embedding = response.data[0].embedding
print(f"Embedding dimensions: {len(embedding)}")
print(f"First 5 values: {embedding[:5]}")
```

**Batch Embedding**:
```python
response = client.embeddings.create(
    model="text-embedding-ada-002",
    input=[
        "First document to embed",
        "Second document to embed",
        "Third document to embed"
    ]
)

for item in response.data:
    print(f"Text {item.index}: {len(item.embedding)} dimensions")
```

**Compute Similarity**:
```python
import numpy as np

def cosine_similarity(a, b):
    return np.dot(a, b) / (np.linalg.norm(a) * np.linalg.norm(b))

# Embed two texts
response = client.embeddings.create(
    model="text-embedding-ada-002",
    input=["cat", "dog"]
)

emb1 = np.array(response.data[0].embedding)
emb2 = np.array(response.data[1].embedding)

similarity = cosine_similarity(emb1, emb2)
print(f"Similarity: {similarity:.4f}")
```

### JavaScript/TypeScript (OpenAI SDK)

**Installation**:
```bash
npm install openai
```

**Usage**:
```typescript
import OpenAI from 'openai';

const openai = new OpenAI({
  baseURL: 'http://localhost:7777/v1',
  apiKey: 'not-needed',
});

async function embedText(text: string) {
  const response = await openai.embeddings.create({
    model: 'text-embedding-ada-002',
    input: text,
  });

  return response.data[0].embedding;
}

// Single embedding
const embedding = await embedText('Hello, world!');
console.log(`Dimensions: ${embedding.length}`);

// Batch embedding
const batchResponse = await openai.embeddings.create({
  model: 'text-embedding-ada-002',
  input: ['text1', 'text2', 'text3'],
});

batchResponse.data.forEach((item) => {
  console.log(`Index ${item.index}: ${item.embedding.length} dims`);
});
```

### Go

**Installation**:
```bash
go get github.com/sashabaranov/go-openai
```

**Usage**:
```go
package main

import (
    "context"
    "fmt"
    openai "github.com/sashabaranov/go-openai"
)

func main() {
    config := openai.DefaultConfig("not-needed")
    config.BaseURL = "http://localhost:7777/v1"
    client := openai.NewClientWithConfig(config)

    resp, err := client.CreateEmbeddings(context.Background(), openai.EmbeddingRequest{
        Input: []string{"Hello, world!"},
        Model: openai.AdaEmbeddingV2,
    })

    if err != nil {
        panic(err)
    }

    fmt.Printf("Dimensions: %d\n", len(resp.Data[0].Embedding))
}
```

### Rust (async-openai)

**Cargo.toml**:
```toml
[dependencies]
async-openai = "0.17"
tokio = { version = "1", features = ["full"] }
```

**Usage**:
```rust
use async_openai::{Client, config::OpenAIConfig, types::{CreateEmbeddingRequest}};

#[tokio::main]
async fn main() {
    let config = OpenAIConfig::new()
        .with_api_base("http://localhost:7777/v1");
    
    let client = Client::with_config(config);

    let request = CreateEmbeddingRequest {
        model: "text-embedding-ada-002".to_string(),
        input: vec!["Hello, world!".to_string()],
        ..Default::default()
    };

    let response = client.embeddings().create(request).await.unwrap();
    
    println!("Dimensions: {}", response.data[0].embedding.len());
}
```

---

## Model Selection

### OpenAI Models

**Available Models** (when using OpenAI backend):
| Model | Dimensions | Cost | Use Case |
|-------|-----------|------|----------|
| `text-embedding-ada-002` | 1536 | Low | General purpose, good quality |
| `text-embedding-3-small` | 512-1536 | Lowest | Fast, cost-effective |
| `text-embedding-3-large` | 256-3072 | Medium | Highest quality |

**Example**:
```bash
curl -X POST http://localhost:7777/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{
    "model": "text-embedding-3-small",
    "input": "Your text here"
  }'
```

### Ollama Models

**Available Models** (when using Ollama backend):
| Model | Dimensions | Size | Use Case |
|-------|-----------|------|----------|
| `all-minilm` | 384 | 23MB | Fast, lightweight, good for local |
| `nomic-embed-text` | 768 | 274MB | High quality, English-focused |
| `mxbai-embed-large` | 1024 | 670MB | Highest quality, multilingual |

**Pull and Use**:
```bash
# Pull model
ollama pull all-minilm

# Use with Nexus (Nexus routes to Ollama automatically)
curl -X POST http://localhost:7777/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{
    "model": "all-minilm",
    "input": "Your text here"
  }'
```

---

## Common Use Cases

### Semantic Search

```python
from openai import OpenAI
import numpy as np

client = OpenAI(base_url="http://localhost:7777/v1", api_key="not-needed")

def embed(text):
    response = client.embeddings.create(model="text-embedding-ada-002", input=text)
    return np.array(response.data[0].embedding)

# Embed documents
documents = [
    "Python is a programming language",
    "JavaScript is used for web development",
    "Machine learning uses neural networks",
]

doc_embeddings = [embed(doc) for doc in documents]

# Embed query
query = "What is Python?"
query_embedding = embed(query)

# Compute similarities
def cosine_similarity(a, b):
    return np.dot(a, b) / (np.linalg.norm(a) * np.linalg.norm(b))

similarities = [cosine_similarity(query_embedding, doc_emb) for doc_emb in doc_embeddings]

# Find most similar
best_match_idx = np.argmax(similarities)
print(f"Best match: {documents[best_match_idx]} (similarity: {similarities[best_match_idx]:.4f})")
```

### Text Classification

```python
# Embed training examples
train_texts = ["I love this!", "This is terrible", "Absolutely amazing"]
train_labels = ["positive", "negative", "positive"]

train_embeddings = [embed(text) for text in train_texts]

# Embed new text
new_text = "This is great"
new_embedding = embed(new_text)

# Find nearest neighbor
similarities = [cosine_similarity(new_embedding, train_emb) for train_emb in train_embeddings]
nearest_idx = np.argmax(similarities)

print(f"Predicted label: {train_labels[nearest_idx]}")
```

### Clustering

```python
from sklearn.cluster import KMeans

# Embed documents
texts = ["doc1...", "doc2...", "doc3...", ...]
embeddings = [embed(text) for text in texts]

# Cluster
kmeans = KMeans(n_clusters=3)
clusters = kmeans.fit_predict(embeddings)

# Print clusters
for i, cluster in enumerate(clusters):
    print(f"Text {i}: cluster {cluster}")
```

---

## Error Handling

### Common Errors

**Empty Input**:
```bash
# Request
curl -X POST http://localhost:7777/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{"model": "text-embedding-ada-002", "input": ""}'

# Response: 400 Bad Request
{
  "error": {
    "message": "Input must not be empty",
    "type": "invalid_request_error"
  }
}
```

**Model Not Found**:
```bash
# Request with nonexistent model
curl -X POST http://localhost:7777/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{"model": "nonexistent-model", "input": "test"}'

# Response: 404 Not Found
{
  "error": {
    "message": "Model 'nonexistent-model' not found",
    "type": "invalid_request_error",
    "code": "model_not_found"
  }
}
```

**No Backends Available**:
```bash
# When no backends have embeddings capability
# Response: 503 Service Unavailable
{
  "error": {
    "message": "No healthy backend available for model 'text-embedding-ada-002'",
    "type": "service_unavailable"
  }
}
```

### Python Error Handling

```python
from openai import OpenAI, OpenAIError

client = OpenAI(base_url="http://localhost:7777/v1", api_key="not-needed")

try:
    response = client.embeddings.create(
        model="text-embedding-ada-002",
        input="Hello, world!"
    )
    embedding = response.data[0].embedding
except OpenAIError as e:
    if e.status_code == 404:
        print("Model not found")
    elif e.status_code == 503:
        print("Service unavailable")
    else:
        print(f"Error: {e}")
```

---

## Performance Tips

### Batching

**Efficient** (single request):
```python
# Good: Send all texts in one request
texts = ["text1", "text2", "text3", ...]
response = client.embeddings.create(model="...", input=texts)
```

**Inefficient** (multiple requests):
```python
# Bad: Separate request for each text
for text in texts:
    response = client.embeddings.create(model="...", input=text)  # Slow!
```

**Batch Size Limits**:
- OpenAI: Up to 2048 inputs per request
- Ollama: No native batching (Nexus iterates internally)
- Recommended: 10-100 texts per batch for optimal performance

### Caching

```python
import hashlib
import pickle

cache = {}

def cached_embed(text):
    # Use hash as cache key
    key = hashlib.md5(text.encode()).hexdigest()
    
    if key in cache:
        return cache[key]
    
    # Compute and cache
    response = client.embeddings.create(model="...", input=text)
    embedding = response.data[0].embedding
    cache[key] = embedding
    
    return embedding
```

### Backend Selection

**OpenAI Backend**:
- ✅ Best for: Production, high throughput, latest models
- ✅ Native batch support (single request for N inputs)
- ⚠️ Requires API key and internet connection

**Ollama Backend**:
- ✅ Best for: Local development, privacy, offline use
- ✅ Free and open source
- ⚠️ Slower batch processing (iterates per-input)

---

## Integration Examples

### LangChain (Python)

```python
from langchain.embeddings import OpenAIEmbeddings

embeddings = OpenAIEmbeddings(
    openai_api_base="http://localhost:7777/v1",
    openai_api_key="not-needed"
)

# Embed query
query_vector = embeddings.embed_query("What is Python?")

# Embed documents
doc_vectors = embeddings.embed_documents([
    "Python is a language",
    "JavaScript is for web"
])
```

### LlamaIndex

```python
from llama_index import ServiceContext, OpenAIEmbedding

embed_model = OpenAIEmbedding(
    api_base="http://localhost:7777/v1",
    api_key="not-needed"
)

service_context = ServiceContext.from_defaults(embed_model=embed_model)
```

### Vector Databases

**Pinecone**:
```python
import pinecone
from openai import OpenAI

client = OpenAI(base_url="http://localhost:7777/v1", api_key="not-needed")

# Embed text
text = "Document to index"
response = client.embeddings.create(model="text-embedding-ada-002", input=text)
embedding = response.data[0].embedding

# Upsert to Pinecone
index = pinecone.Index("my-index")
index.upsert([("id1", embedding, {"text": text})])
```

**Weaviate**:
```python
import weaviate

client = weaviate.Client("http://localhost:8080")

# Weaviate can call Nexus for embeddings
client.schema.create_class({
    "class": "Document",
    "vectorizer": "text2vec-openai",
    "moduleConfig": {
        "text2vec-openai": {
            "baseURL": "http://localhost:7777/v1"
        }
    }
})
```

---

## Troubleshooting

### Issue: "Model not found"

**Symptom**: 404 error for embedding model

**Solution**:
```bash
# Check available models
curl http://localhost:7777/v1/models | jq '.data[].id'

# Ensure backend is running
# OpenAI: Check API key
# Ollama: Check `ollama list` for embedding models
```

### Issue: "Service unavailable"

**Symptom**: 503 error when requesting embeddings

**Causes**:
1. No backends have `embeddings: true` capability
2. All embedding-capable backends are unhealthy
3. Backend is running but not discovered

**Solution**:
```bash
# Check backend health
curl http://localhost:7777/health | jq .

# For Ollama: Ensure service is running
systemctl status ollama
# Or: ollama serve

# For OpenAI: Check API key
echo $OPENAI_API_KEY
```

### Issue: Slow batch processing

**Symptom**: Batch requests take N × single request time

**Cause**: Using Ollama backend (no native batching)

**Solutions**:
1. Switch to OpenAI backend for production
2. Reduce batch size (fewer items per request)
3. Use smaller Ollama models (`all-minilm` vs `mxbai-embed-large`)

---

## Best Practices

### 1. Use Batching

Always send multiple texts in a single request when possible:
```python
# Good
client.embeddings.create(model="...", input=["text1", "text2", ...])

# Bad
for text in texts:
    client.embeddings.create(model="...", input=text)
```

### 2. Handle Errors Gracefully

```python
try:
    response = client.embeddings.create(...)
except OpenAIError as e:
    # Log error, use fallback, or retry
    logger.error(f"Embedding failed: {e}")
```

### 3. Cache Results

Embeddings for the same text are deterministic — cache them:
```python
from functools import lru_cache

@lru_cache(maxsize=10000)
def embed_cached(text):
    response = client.embeddings.create(model="...", input=text)
    return tuple(response.data[0].embedding)  # Tuple for hashability
```

### 4. Monitor Token Usage

```python
response = client.embeddings.create(model="...", input=texts)
print(f"Tokens used: {response.usage.total_tokens}")
```

### 5. Choose Appropriate Model

- **Development/Testing**: Ollama `all-minilm` (fast, local)
- **Production/High Quality**: OpenAI `text-embedding-3-small` (best balance)
- **Maximum Quality**: OpenAI `text-embedding-3-large` (highest cost)

---

## References

**Nexus Documentation**:
- Feature Spec: `specs/020-embeddings-api/spec.md`
- Data Model: `specs/020-embeddings-api/data-model.md`
- API Contract: `specs/020-embeddings-api/contracts/embeddings.json`

**External Resources**:
- OpenAI Embeddings Guide: https://platform.openai.com/docs/guides/embeddings
- Ollama Embeddings: https://github.com/ollama/ollama/blob/main/docs/api.md#generate-embeddings
- OpenAI Cookbook: https://github.com/openai/openai-cookbook

**Client Libraries**:
- Python: https://github.com/openai/openai-python
- JavaScript: https://github.com/openai/openai-node
- Go: https://github.com/sashabaranov/go-openai
- Rust: https://github.com/64bit/async-openai

---

**Document Version**: 1.0  
**Created**: 2025-02-17  
**Type**: Retrospective Quickstart Guide
