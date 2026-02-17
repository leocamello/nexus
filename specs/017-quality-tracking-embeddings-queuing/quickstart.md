# Quickstart: Quality Tracking, Embeddings & Request Queuing

**Feature**: Phase 2.5 — F15–F18  
**Audience**: Operators deploying Nexus v0.4

---

## Prerequisites

- Nexus v0.4+ binary (built from `feature/017-quality-embeddings-queuing` branch or release)
- At least one local backend (Ollama recommended for testing)
- `curl` and `jq` for API interaction

---

## 1. Configuration

Add the new `[quality]` and `[queue]` sections to your `nexus.toml`:

```toml
[server]
host = "127.0.0.1"
port = 3000

# === Quality Tracking (F16) ===
[quality]
error_rate_threshold = 0.5          # Exclude agents with >50% error rate
ttft_penalty_threshold_ms = 3000    # Penalize agents slower than 3s TTFT
metrics_interval_seconds = 30       # Background recompute interval

# === Request Queuing (F18) ===
[queue]
enabled = true
max_size = 100                      # Maximum queued requests
max_wait_seconds = 30               # Timeout before 503

# === Backends ===
[[backends]]
name = "ollama-local"
url = "http://localhost:11434"
backend_type = "ollama"
```

> **Note**: Both `[quality]` and `[queue]` sections are optional. Defaults are shown above.

---

## 2. Start Nexus

```bash
RUST_LOG=info cargo run -- serve --config nexus.toml
```

You should see log lines confirming quality tracking and queue initialization:

```
INFO nexus: Quality tracking enabled (threshold: 50%, interval: 30s)
INFO nexus: Request queue enabled (max_size: 100, max_wait: 30s)
```

---

## Scenario 1: Quality Tracking in Action

### Step 1: Generate Traffic

Send several requests to build quality history:

```bash
# Successful requests (assuming "llama3.2" is available)
for i in $(seq 1 10); do
  curl -s http://localhost:3000/v1/chat/completions \
    -H "Content-Type: application/json" \
    -d '{
      "model": "llama3.2",
      "messages": [{"role": "user", "content": "Hello"}]
    }' | jq -r '.choices[0].message.content' &
done
wait
```

### Step 2: Check Quality Metrics

After 30 seconds (one quality loop cycle), check metrics via the stats API:

```bash
curl -s http://localhost:3000/v1/stats | jq '.quality_metrics'
```

Expected output:

```json
{
  "ollama-local": {
    "error_rate_1h": 0.0,
    "avg_ttft_ms": 245,
    "success_rate_24h": 1.0,
    "request_count_1h": 10
  }
}
```

### Step 3: Check Prometheus Metrics

```bash
curl -s http://localhost:3000/metrics | grep -E "nexus_agent_(error_rate|ttft|request_count)"
```

Expected:

```
nexus_agent_error_rate{agent_id="ollama-local"} 0.0
nexus_agent_ttft_seconds{agent_id="ollama-local"} 0.245
nexus_agent_request_count_1h{agent_id="ollama-local"} 10
```

---

## Scenario 2: Embedding Requests

### Step 1: Send a Single Embedding

```bash
curl -s http://localhost:3000/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{
    "model": "nomic-embed-text",
    "input": "The quick brown fox jumps over the lazy dog"
  }' | jq
```

Expected response (OpenAI-compatible format):

```json
{
  "object": "list",
  "data": [
    {
      "object": "embedding",
      "embedding": [0.123, -0.456, ...],
      "index": 0
    }
  ],
  "model": "nomic-embed-text",
  "usage": {
    "prompt_tokens": 10,
    "total_tokens": 10
  }
}
```

### Step 2: Send a Batch Embedding

```bash
curl -s http://localhost:3000/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{
    "model": "nomic-embed-text",
    "input": [
      "First document to embed",
      "Second document to embed",
      "Third document to embed"
    ]
  }' | jq '.data | length'
```

Expected: `3`

### Step 3: Model Not Found

```bash
curl -s http://localhost:3000/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{
    "model": "nonexistent-model",
    "input": "test"
  }' | jq
```

Expected: HTTP 404 with OpenAI-compatible error response.

---

## Scenario 3: Request Queue Under Load

### Step 1: Saturate Backends

First, confirm queueing is enabled:

```bash
curl -s http://localhost:3000/v1/stats | jq '.queue'
```

Expected:

```json
{
  "enabled": true,
  "depth": 0,
  "max_size": 100
}
```

### Step 2: Send Many Concurrent Requests

```bash
# Send 20 concurrent requests
for i in $(seq 1 20); do
  curl -s -w "\nHTTP Status: %{http_code}\n" \
    http://localhost:3000/v1/chat/completions \
    -H "Content-Type: application/json" \
    -d '{
      "model": "llama3.2",
      "messages": [{"role": "user", "content": "Write a haiku about request #'"$i"'"}]
    }' &
done
wait
```

### Step 3: Use Priority Headers

High-priority requests drain before normal ones:

```bash
# High-priority request
curl -s http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "X-Nexus-Priority: high" \
  -d '{
    "model": "llama3.2",
    "messages": [{"role": "user", "content": "Urgent request"}]
  }' | jq
```

### Step 4: Check Queue Depth

During load:

```bash
# Via stats API
curl -s http://localhost:3000/v1/stats | jq '.queue.depth'

# Via Prometheus
curl -s http://localhost:3000/metrics | grep nexus_queue_depth
```

---

## Scenario 4: Quality-Based Exclusion

### Setup: Simulate a Failing Backend

If you have two backends, you can observe quality-based exclusion:

```toml
[[backends]]
name = "reliable-backend"
url = "http://localhost:11434"
backend_type = "ollama"

[[backends]]
name = "flaky-backend"
url = "http://localhost:11435"
backend_type = "ollama"
```

When `flaky-backend` exceeds the error rate threshold (50%), it will be excluded
from routing. The `X-Nexus-Route-Reason` header will indicate the remaining backends.

---

## Monitoring

### Key Prometheus Metrics to Watch

| Metric | Type | Description |
|--------|------|-------------|
| `nexus_agent_error_rate` | gauge | Per-agent error rate (1h window) |
| `nexus_agent_ttft_seconds` | gauge | Per-agent average TTFT |
| `nexus_agent_request_count_1h` | gauge | Per-agent request count |
| `nexus_queue_depth` | gauge | Current queue depth |
| `nexus_queue_enqueued_total` | counter | Total enqueued requests |
| `nexus_queue_timeout_total` | counter | Total timed-out requests |

### Example Grafana Dashboard Queries

```promql
# Backend health overview
nexus_agent_error_rate > 0.3

# Average TTFT across all agents
avg(nexus_agent_ttft_seconds)

# Queue utilization
nexus_queue_depth / 100  # As percentage of max_size
```

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| Embeddings return 404 | Model not found or not embedding-capable | Ensure model name contains "embed" or agent declares embedding capability |
| All backends excluded (503) | error_rate_threshold too low | Increase `quality.error_rate_threshold` in config |
| Queue full (503) | Too many concurrent requests | Increase `queue.max_size` or add backends |
| Queue timeout (503 with Retry-After) | Backend too slow | Increase `queue.max_wait_seconds` or optimize backends |
| Quality metrics all zero | Less than 30s since startup | Wait for first quality loop cycle |

---

## Performance Notes

- Quality recomputation: < 1ms per cycle (iterates all agents, prunes old data)
- Queue enqueue/dequeue: O(1) using mpsc channels
- Reconciler pipeline: < 1ms total (including QualityReconciler)
- Embedding forwarding: Adds ~2ms overhead to backend latency (routing + header injection)
