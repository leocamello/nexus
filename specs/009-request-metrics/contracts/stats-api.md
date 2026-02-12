# Stats API Contract

This document defines the JSON statistics endpoint at `GET /v1/stats`.

## Endpoint

**URL**: `GET /v1/stats`  
**Content-Type**: `application/json`  
**Authentication**: None (trusted network)

---

## Response Format

### Success Response

**Status**: `200 OK`  
**Content-Type**: `application/json`

**Body**:
```json
{
  "uptime_seconds": 3600,
  "requests": {
    "total": 1000,
    "success": 950,
    "errors": 50
  },
  "backends": [
    {
      "id": "ollama-local",
      "requests": 500,
      "average_latency_ms": 1250.5,
      "pending": 2
    },
    {
      "id": "vllm-gpu-1",
      "requests": 300,
      "average_latency_ms": 850.2,
      "pending": 0
    },
    {
      "id": "openai-cloud",
      "requests": 200,
      "average_latency_ms": 2100.8,
      "pending": 1
    }
  ],
  "models": [
    {
      "name": "llama3:70b",
      "requests": 300,
      "average_duration_ms": 5000.0
    },
    {
      "name": "gpt-4",
      "requests": 400,
      "average_duration_ms": 3500.0
    },
    {
      "name": "mixtral:8x7b",
      "requests": 300,
      "average_duration_ms": 2200.0
    }
  ]
}
```

---

## Field Definitions

### Top-Level Fields

#### `uptime_seconds`
- **Type**: integer
- **Description**: Gateway uptime in seconds since startup
- **Computation**: `Instant::now() - start_time`
- **Example**: `3600` (1 hour)

#### `requests`
- **Type**: object
- **Description**: Aggregate request statistics across all backends and models
- **Fields**:
  - `total` (integer): Total requests processed (sum of all `nexus_requests_total`)
  - `success` (integer): Successful requests (status 2xx)
  - `errors` (integer): Failed requests (status 4xx, 5xx)

**Derivation**:
```rust
let total = sum_counters("nexus_requests_total");
let success = sum_counters_with_label("nexus_requests_total", "status", "200");
let errors = total - success;
```

#### `backends`
- **Type**: array of objects
- **Description**: Per-backend breakdown of request activity
- **Ordering**: Sorted by backend ID (alphabetical)

#### `models`
- **Type**: array of objects
- **Description**: Per-model breakdown of request activity
- **Ordering**: Sorted by request count (descending)

---

### Backend Object Fields

#### `id`
- **Type**: string
- **Description**: Backend identifier (as registered in Registry)
- **Example**: `"ollama-local"`, `"vllm-gpu-1"`

#### `requests`
- **Type**: integer
- **Description**: Number of requests served by this backend
- **Derivation**: Sum of `nexus_requests_total{backend="X"}` across all statuses and models

#### `average_latency_ms`
- **Type**: number (float)
- **Description**: Average request latency in milliseconds
- **Derivation**: 
  ```
  sum = nexus_request_duration_seconds_sum{backend="X"}
  count = nexus_request_duration_seconds_count{backend="X"}
  average_latency_ms = (sum / count) * 1000
  ```
- **Precision**: 1 decimal place
- **Special cases**: 
  - If `count == 0`, return `0.0`

#### `pending`
- **Type**: integer
- **Description**: Number of requests currently queued or in-flight for this backend
- **Derivation**: `nexus_pending_requests{backend="X"}` gauge value
- **Note**: For F09, this will always be `0` (no queue tracking yet)

---

### Model Object Fields

#### `name`
- **Type**: string
- **Description**: Model name (as seen in requests)
- **Example**: `"llama3:70b"`, `"gpt-4"`

#### `requests`
- **Type**: integer
- **Description**: Number of requests for this model
- **Derivation**: Sum of `nexus_requests_total{model="X"}` across all backends and statuses

#### `average_duration_ms`
- **Type**: number (float)
- **Description**: Average request duration in milliseconds
- **Derivation**:
  ```
  sum = nexus_request_duration_seconds_sum{model="X"}
  count = nexus_request_duration_seconds_count{model="X"}
  average_duration_ms = (sum / count) * 1000
  ```
- **Precision**: 1 decimal place
- **Special cases**:
  - If `count == 0`, return `0.0`

---

## Error Responses

### Service Unavailable

If metrics collection is not initialized:

**Status**: `503 Service Unavailable`  
**Content-Type**: `application/json`

**Body**:
```json
{
  "error": {
    "message": "Metrics collection not initialized",
    "type": "service_unavailable",
    "code": "metrics_unavailable"
  }
}
```

---

## Example Request/Response

### Request
```http
GET /v1/stats HTTP/1.1
Host: localhost:8000
Accept: application/json
```

### Response
```http
HTTP/1.1 200 OK
Content-Type: application/json
Content-Length: 547

{
  "uptime_seconds": 7200,
  "requests": {
    "total": 5420,
    "success": 5250,
    "errors": 170
  },
  "backends": [
    {
      "id": "ollama-local",
      "requests": 2800,
      "average_latency_ms": 1150.3,
      "pending": 0
    },
    {
      "id": "vllm-gpu-1",
      "requests": 1620,
      "average_latency_ms": 820.5,
      "pending": 0
    },
    {
      "id": "vllm-gpu-2",
      "requests": 1000,
      "average_latency_ms": 850.2,
      "pending": 0
    }
  ],
  "models": [
    {
      "name": "llama3:70b",
      "requests": 2500,
      "average_duration_ms": 4800.0
    },
    {
      "name": "mixtral:8x7b",
      "requests": 1800,
      "average_duration_ms": 2100.0
    },
    {
      "name": "phi-3:medium",
      "requests": 1120,
      "average_duration_ms": 1200.0
    }
  ]
}
```

---

## Implementation Notes

### Computation Algorithm

1. **Query Prometheus handle** for all metric values
2. **Compute aggregate stats**:
   - Total requests: Sum all `nexus_requests_total` counters
   - Success/error split: Filter by status code
3. **Group by backend**:
   - Iterate unique backend labels
   - Aggregate request counts and compute averages
4. **Group by model**:
   - Iterate unique model labels
   - Aggregate request counts and compute averages
5. **Query Registry** for pending request counts (future)
6. **Serialize to JSON** and return

### Performance Considerations

- **Target latency**: < 2ms for typical workloads
- **Caching**: No caching - always return fresh data
- **Memory**: Allocate `Vec` for backends and models (~100 entries typical)
- **Sorting**: Backends alphabetically, models by request count descending

### Precision

- Latency/duration averages rounded to 1 decimal place
- Request counts are exact integers
- Uptime is exact seconds (no fractional seconds)

---

## Use Cases

### Operational Dashboards
- Quick glance at system health
- Identify slow backends
- Monitor request distribution

### Debugging
- Verify routing decisions
- Check which backends are receiving traffic
- Identify underutilized backends

### Capacity Planning
- Understand model popularity
- Plan backend allocation
- Identify bottlenecks

---

## Integration with F10 (Web Dashboard)

The `/v1/stats` endpoint is designed to be consumed by the F10 Web Dashboard feature:

- **Polling interval**: Dashboard will poll every 5 seconds
- **Data visualization**: JSON maps directly to dashboard charts
- **No breaking changes**: Schema is stable and extensible

Future dashboard enhancements (F10):
- Real-time charts for request rate
- Backend health visualization
- Model performance comparison

---

## Comparison with /metrics Endpoint

| Feature | /metrics (Prometheus) | /v1/stats (JSON) |
|---------|----------------------|------------------|
| Format | Text (Prometheus) | JSON |
| Audience | Prometheus scraper | Dashboards, humans |
| Data | Raw counters/histograms | Aggregated statistics |
| Latency | < 1ms | < 2ms |
| Cardinality | Full label dimensions | Grouped by backend/model |
| Computation | Minimal | Averages, grouping |
| Use case | Long-term monitoring | Real-time debugging |

---

## Testing Strategy

### Unit Tests
1. Test stats computation with mock Prometheus data
2. Test JSON serialization
3. Test edge cases (zero requests, missing backends)

### Integration Tests
1. Send requests → query `/v1/stats` → verify counts
2. Multiple backends → verify per-backend breakdown
3. Multiple models → verify per-model breakdown

### Contract Tests
1. Validate response against JSON schema
2. Verify field types and required fields
3. Test error responses (503 when metrics unavailable)

---

## Future Extensions (Not in F09 Scope)

### Potential Enhancements for Future Versions:
1. **Query parameters**: Filter by time range, backend, model
2. **Error breakdown**: Detailed error type statistics
3. **Percentiles**: P50, P95, P99 latencies
4. **Cost tracking**: Per-backend and per-model cost estimates
5. **Rate calculations**: Requests/second over last N minutes
6. **Historical trends**: Sparklines for last hour

These are noted for awareness but are NOT implemented in F09.
