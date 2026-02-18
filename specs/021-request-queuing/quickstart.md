# Quickstart: Request Queuing & Prioritization

**Phase 1 Guide** | **Date**: 2026-02-15 (Retrospective) | **Feature**: F18

This guide shows how to use the request queuing feature in Nexus.

## What is Request Queuing?

When all backends reach their capacity limits, Nexus can queue incoming requests instead of immediately returning 503 errors. Queued requests are processed as capacity becomes available.

**Key Features**:
- 🔄 Automatic queuing when backends are saturated
- ⚡ Dual-priority support (High/Normal)
- ⏱️ Configurable timeout (default: 30 seconds)
- 📊 Prometheus metrics for monitoring
- 🛡️ Bounded queue prevents memory exhaustion

## Quick Start

### 1. Enable Queuing (Default Configuration)

Queuing is **enabled by default** with sensible defaults. No configuration required!

```toml
# Default configuration (auto-applied if not specified)
[queue]
enabled = true
max_size = 100
max_wait_seconds = 30
```

### 2. Send Requests

Requests are automatically queued when all backends are at capacity. No client changes required!

```bash
# Normal priority (default)
curl http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3:8b",
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

**Response** (if queued and processed successfully):
```json
{
  "id": "chatcmpl-123",
  "object": "chat.completion",
  "model": "llama3:8b",
  "choices": [{
    "index": 0,
    "message": {
      "role": "assistant",
      "content": "Hello! How can I help you today?"
    }
  }]
}
```

### 3. Monitor Queue Depth

Check current queue depth via Prometheus metrics:

```bash
curl http://localhost:9091/metrics | grep nexus_queue_depth
```

**Output**:
```text
# HELP nexus_queue_depth Current number of requests in queue
# TYPE nexus_queue_depth gauge
nexus_queue_depth 5
```

---

## Priority Requests

Use the `X-Nexus-Priority` header to prioritize urgent requests.

### High Priority Request

```bash
curl http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "X-Nexus-Priority: high" \
  -d '{
    "model": "llama3:8b",
    "messages": [{"role": "user", "content": "URGENT: Production issue"}]
  }'
```

**Behavior**:
- High-priority requests are processed before normal-priority requests
- FIFO ordering within each priority level
- No starvation (normal requests are still processed)

### Normal Priority Request (Default)

```bash
curl http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "X-Nexus-Priority: normal" \
  -d '{
    "model": "llama3:8b",
    "messages": [{"role": "user", "content": "Background analysis"}]
  }'
```

**Note**: `X-Nexus-Priority: normal` is the default. You can omit this header.

---

## Configuration

### Custom Queue Settings

Edit your `nexus.toml`:

```toml
[queue]
enabled = true
max_size = 200           # Hold up to 200 requests
max_wait_seconds = 60    # Wait up to 60 seconds
```

**Restart Nexus** to apply changes:
```bash
nexus serve --config nexus.toml
```

### Disable Queuing

**Option 1**: Set `enabled = false`
```toml
[queue]
enabled = false
```

**Option 2**: Set `max_size = 0`
```toml
[queue]
enabled = true
max_size = 0  # Effectively disables queuing
```

When queuing is disabled, requests return 503 immediately when backends are at capacity.

---

## Error Responses

### Queue Full (503)

When the queue reaches capacity, new requests are rejected:

```http
HTTP/1.1 503 Service Unavailable
Content-Type: application/json

{
  "error": {
    "message": "All backends at capacity and queue is full",
    "type": "service_unavailable",
    "code": 503
  }
}
```

**Retry Strategy**: Exponential backoff starting at 1 second.

### Request Timeout (503)

When a request waits longer than `max_wait_seconds`:

```http
HTTP/1.1 503 Service Unavailable
Retry-After: 30
Content-Type: application/json

{
  "error": {
    "message": "Request timed out in queue",
    "type": "service_unavailable",
    "code": 503
  }
}
```

**Retry Strategy**: Wait `Retry-After` seconds, then retry with exponential backoff.

### Queue Disabled (503)

When queuing is disabled in config:

```http
HTTP/1.1 503 Service Unavailable
Content-Type: application/json

{
  "error": {
    "message": "All backends at capacity",
    "type": "service_unavailable",
    "code": 503
  }
}
```

**Retry Strategy**: Exponential backoff starting at 1 second.

---

## Monitoring & Alerting

### Prometheus Metrics

**Metric**: `nexus_queue_depth`  
**Type**: Gauge  
**Description**: Current number of requests in queue (high + normal)

**Query Examples**:

```promql
# Current queue depth
nexus_queue_depth

# Queue utilization (requires nexus_queue_max_size metric)
nexus_queue_depth / 100 * 100  # Assuming max_size=100

# Average queue depth over 5 minutes
avg_over_time(nexus_queue_depth[5m])

# Max queue depth over 1 hour
max_over_time(nexus_queue_depth[1h])
```

### Grafana Dashboard

Create a panel with this query:

```promql
nexus_queue_depth
```

**Visualization**: Time series graph  
**Y-Axis**: Queue depth (0 to max_size)  
**Threshold**: Alert at 80% capacity

### Alert Rules

**Alert 1**: Queue is 80% full
```promql
- alert: QueueNearCapacity
  expr: nexus_queue_depth > 80
  for: 2m
  annotations:
    summary: "Queue is 80% full ({{ $value }} requests)"
```

**Alert 2**: Queue has been non-empty for 5 minutes
```promql
- alert: QueueNotDraining
  expr: min_over_time(nexus_queue_depth[5m]) > 0
  for: 5m
  annotations:
    summary: "Queue not draining ({{ $value }} requests for 5m)"
```

---

## Use Cases

### Use Case 1: Burst Traffic Handling

**Scenario**: Team standup at 9 AM, everyone asks questions simultaneously.

**Without Queue**: 50% of requests return 503 (backends saturated)

**With Queue**:
```toml
[queue]
enabled = true
max_size = 100
max_wait_seconds = 30
```

**Result**: All requests are accepted, processed within 30 seconds as capacity allows.

---

### Use Case 2: Priority-Based Processing

**Scenario**: Production monitoring (high priority) + batch analytics (normal priority).

**Configuration**:
```toml
[queue]
enabled = true
max_size = 200
max_wait_seconds = 60
```

**Monitoring Requests** (high priority):
```bash
curl http://localhost:3000/v1/chat/completions \
  -H "X-Nexus-Priority: high" \
  -d '{"model": "llama3:8b", "messages": [...]}'
```

**Analytics Requests** (normal priority):
```bash
curl http://localhost:3000/v1/chat/completions \
  -H "X-Nexus-Priority: normal" \
  -d '{"model": "llama3:8b", "messages": [...]}'
```

**Result**: Monitoring requests are processed first, analytics waits but eventually completes.

---

### Use Case 3: Graceful Degradation

**Scenario**: One backend goes down, remaining backend is overloaded.

**Without Queue**: 70% of requests return 503 immediately

**With Queue**:
```toml
[queue]
enabled = true
max_size = 150
max_wait_seconds = 45
```

**Result**: Requests queue up, giving the remaining backend time to process them. Users see latency increase (up to 45s) but no hard failures until queue is full.

---

## Testing

### Test 1: Verify Queuing is Enabled

```bash
# Send request when backends are idle
curl -w "\nTime: %{time_total}s\n" \
  http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "llama3:8b", "messages": [{"role": "user", "content": "test"}]}'
```

**Expected**: Response in < 1 second (not queued).

### Test 2: Saturate Backends

```bash
# Send 10 concurrent requests (assuming 5 backend capacity)
for i in {1..10}; do
  curl http://localhost:3000/v1/chat/completions \
    -H "Content-Type: application/json" \
    -d "{\"model\": \"llama3:8b\", \"messages\": [{\"role\": \"user\", \"content\": \"Request $i\"}]}" &
done
wait
```

**Expected**: 
- First 5 requests: Processed immediately
- Next 5 requests: Queued (200 response after wait)
- Queue depth metric increases to 5, then drains

### Test 3: Test Priority

```bash
# Send normal priority request
curl http://localhost:3000/v1/chat/completions \
  -H "X-Nexus-Priority: normal" \
  -d '{"model": "llama3:8b", "messages": [{"role": "user", "content": "Normal"}]}' &

# Send high priority request (should be processed first)
sleep 0.1
curl http://localhost:3000/v1/chat/completions \
  -H "X-Nexus-Priority: high" \
  -d '{"model": "llama3:8b", "messages": [{"role": "user", "content": "High"}]}' &

wait
```

**Expected**: High-priority request completes before normal-priority request.

### Test 4: Test Timeout

```bash
# Configure short timeout for testing
# Edit nexus.toml:
# [queue]
# max_wait_seconds = 5

# Saturate all backends for > 5 seconds
# (simulate by stopping all backends)

curl -w "\nStatus: %{http_code}\n" \
  http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "llama3:8b", "messages": [{"role": "user", "content": "Timeout test"}]}'
```

**Expected**: 503 response after 5 seconds with `Retry-After: 5` header.

---

## Troubleshooting

### Problem: Requests always return 503 immediately

**Cause 1**: Queue is disabled
```bash
# Check config
grep -A3 '\[queue\]' nexus.toml
```
**Solution**: Set `enabled = true` or remove `enabled = false`.

**Cause 2**: `max_size = 0`
```bash
# Check config
grep 'max_size' nexus.toml
```
**Solution**: Set `max_size > 0` (e.g., `max_size = 100`).

---

### Problem: Requests timeout in queue

**Cause**: `max_wait_seconds` is too low
```bash
# Check config
grep 'max_wait_seconds' nexus.toml
```
**Solution**: Increase `max_wait_seconds` (e.g., `max_wait_seconds = 60`).

---

### Problem: Queue depth keeps growing

**Cause**: Backends are too slow or down
```bash
# Check backend health
curl http://localhost:3000/health

# Check backend status
curl http://localhost:3000/api/backends
```
**Solution**: 
1. Increase backend capacity (add more backends)
2. Increase `max_wait_seconds` to allow more time
3. Investigate backend performance issues

---

### Problem: High-priority requests not processed first

**Cause**: Header is misspelled or case-sensitive
```bash
# Incorrect (case-sensitive)
curl -H "X-Nexus-priority: High" ...  # Wrong: "priority" should be "Priority"

# Correct
curl -H "X-Nexus-Priority: high" ...  # Correct
```
**Solution**: Use exact header name `X-Nexus-Priority` (case-insensitive value).

---

## Best Practices

### 1. Size the Queue Appropriately

**Rule of Thumb**: `max_size = 2 * backend_capacity`

**Example**: If you have 5 backends with 10 capacity each (50 total):
```toml
[queue]
max_size = 100  # 2 * 50
```

### 2. Set Realistic Timeouts

**Rule of Thumb**: `max_wait_seconds = 2 * p95_latency`

**Example**: If your p95 latency is 10 seconds:
```toml
[queue]
max_wait_seconds = 20  # 2 * 10
```

### 3. Monitor Queue Depth

Set up alerts for:
- Queue depth > 80% capacity (warning)
- Queue depth = 100% capacity (critical)
- Queue not draining for > 5 minutes (critical)

### 4. Use Priority Sparingly

**Good Use Cases**:
- Production monitoring/alerting
- Critical user-facing requests
- Time-sensitive operations

**Bad Use Cases**:
- All requests (defeats purpose)
- Batch jobs (use normal priority)
- Background tasks (use normal priority)

### 5. Test Graceful Degradation

Regularly test:
1. One backend down (queue should absorb traffic)
2. All backends down (queue should timeout gracefully)
3. Queue full (should return 503 with clear message)

---

## Migration Guide

### From No Queuing to Queuing

**Step 1**: Add config (optional, defaults are good)
```toml
[queue]
enabled = true
max_size = 100
max_wait_seconds = 30
```

**Step 2**: Restart Nexus
```bash
nexus serve --config nexus.toml
```

**Step 3**: Monitor metrics
```bash
curl http://localhost:9091/metrics | grep nexus_queue_depth
```

**Step 4**: Adjust config based on usage
- If queue is always empty: No changes needed
- If queue often fills up: Increase `max_size`
- If requests timeout: Increase `max_wait_seconds`

---

### From Queuing to No Queuing

**Step 1**: Disable queue
```toml
[queue]
enabled = false
```

**Step 2**: Restart Nexus
```bash
nexus serve --config nexus.toml
```

**Step 3**: Handle 503 errors in clients
```python
# Add retry logic with exponential backoff
import time
import requests

def chat_with_retry(payload, max_retries=3):
    for attempt in range(max_retries):
        response = requests.post("http://localhost:3000/v1/chat/completions", json=payload)
        if response.status_code == 200:
            return response.json()
        elif response.status_code == 503:
            # Exponential backoff: 1s, 2s, 4s
            time.sleep(2 ** attempt)
        else:
            raise Exception(f"Error: {response.status_code}")
    raise Exception("Max retries exceeded")
```

---

## API Reference

See detailed API documentation:
- [Queue Types Contract](./contracts/queue-types.md)
- [Queue API Contract](./contracts/queue-api.md)

---

## Next Steps

- [Read the full specification](./spec.md)
- [Review implementation details](./data-model.md)
- [Explore research decisions](./research.md)
- [Check Prometheus metrics](http://localhost:9091/metrics)
- [View Nexus dashboard](http://localhost:3000/dashboard)

---

**Quickstart Complete** | **Phase 1** | **Ready for users**
