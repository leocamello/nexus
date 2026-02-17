# Prometheus Metrics Contract

**Endpoint**: `GET /metrics`  
**Format**: Prometheus text format  
**Content-Type**: `text/plain; version=0.0.4`

## Quality Metrics

### nexus_agent_error_rate

**Type**: Gauge  
**Description**: Error rate over the last 1 hour (ratio of failed requests to total requests)  
**Labels**:
- `agent_id`: Unique identifier for backend+model combination (e.g., "ollama_local_llama3_8b")

**Range**: 0.0 (no errors) to 1.0 (all requests failed)

**Example**:
```prometheus
# HELP nexus_agent_error_rate Error rate over the last 1 hour
# TYPE nexus_agent_error_rate gauge
nexus_agent_error_rate{agent_id="ollama_local_llama3_8b"} 0.05
nexus_agent_error_rate{agent_id="openai_cloud_gpt4o"} 0.02
```

---

### nexus_agent_success_rate_24h

**Type**: Gauge  
**Description**: Success rate over the last 24 hours (ratio of successful requests to total requests)  
**Labels**:
- `agent_id`: Unique identifier for backend+model combination

**Range**: 0.0 (all failed) to 1.0 (all succeeded)

**Example**:
```prometheus
# HELP nexus_agent_success_rate_24h Success rate over the last 24 hours
# TYPE nexus_agent_success_rate_24h gauge
nexus_agent_success_rate_24h{agent_id="ollama_local_llama3_8b"} 0.98
nexus_agent_success_rate_24h{agent_id="openai_cloud_gpt4o"} 0.99
```

---

### nexus_agent_ttft_seconds

**Type**: Histogram  
**Description**: Time-to-first-token (TTFT) distribution in seconds  
**Labels**:
- `agent_id`: Unique identifier for backend+model combination

**Buckets**: `[0.05, 0.1, 0.5, 1.0, 5.0, +Inf]`

**Metrics Provided**:
- `nexus_agent_ttft_seconds_bucket{le="N"}`: Count of observations ≤ N seconds
- `nexus_agent_ttft_seconds_sum`: Sum of all TTFT observations
- `nexus_agent_ttft_seconds_count`: Total number of observations

**Example**:
```prometheus
# HELP nexus_agent_ttft_seconds Time-to-first-token distribution
# TYPE nexus_agent_ttft_seconds histogram
nexus_agent_ttft_seconds_bucket{agent_id="ollama_local_llama3_8b",le="0.05"} 5
nexus_agent_ttft_seconds_bucket{agent_id="ollama_local_llama3_8b",le="0.1"} 25
nexus_agent_ttft_seconds_bucket{agent_id="ollama_local_llama3_8b",le="0.5"} 100
nexus_agent_ttft_seconds_bucket{agent_id="ollama_local_llama3_8b",le="1.0"} 180
nexus_agent_ttft_seconds_bucket{agent_id="ollama_local_llama3_8b",le="5.0"} 200
nexus_agent_ttft_seconds_bucket{agent_id="ollama_local_llama3_8b",le="+Inf"} 210
nexus_agent_ttft_seconds_sum{agent_id="ollama_local_llama3_8b"} 125.5
nexus_agent_ttft_seconds_count{agent_id="ollama_local_llama3_8b"} 210
```

**Derived Metrics** (via PromQL):
```promql
# Average TTFT
nexus_agent_ttft_seconds_sum / nexus_agent_ttft_seconds_count

# P95 TTFT (approximate)
histogram_quantile(0.95, rate(nexus_agent_ttft_seconds_bucket[5m]))

# P99 TTFT (approximate)
histogram_quantile(0.99, rate(nexus_agent_ttft_seconds_bucket[5m]))
```

---

### nexus_agent_request_count_1h

**Type**: Gauge  
**Description**: Total number of requests processed in the last 1 hour  
**Labels**:
- `agent_id`: Unique identifier for backend+model combination

**Range**: 0 to unbounded

**Example**:
```prometheus
# HELP nexus_agent_request_count_1h Requests processed in the last 1 hour
# TYPE nexus_agent_request_count_1h gauge
nexus_agent_request_count_1h{agent_id="ollama_local_llama3_8b"} 120
nexus_agent_request_count_1h{agent_id="openai_cloud_gpt4o"} 30
```

---

## Reconciler Metrics

### nexus_reconciler_duration_seconds

**Type**: Histogram  
**Description**: Duration of reconciler execution  
**Labels**:
- `reconciler`: Name of the reconciler (e.g., "quality", "scheduler")

**Buckets**: `[0.00005, 0.0001, 0.00025, 0.0005, 0.001, 0.0025, 0.005, 0.01, +Inf]`

**Example**:
```prometheus
# HELP nexus_reconciler_duration_seconds Reconciler execution duration
# TYPE nexus_reconciler_duration_seconds histogram
nexus_reconciler_duration_seconds_bucket{reconciler="quality",le="0.0001"} 150
nexus_reconciler_duration_seconds_bucket{reconciler="quality",le="0.001"} 200
nexus_reconciler_duration_seconds_sum{reconciler="quality"} 0.025
nexus_reconciler_duration_seconds_count{reconciler="quality"} 210
```

---

### nexus_reconciler_exclusions_total

**Type**: Counter  
**Description**: Total number of backend exclusions by reconciler  
**Labels**:
- `reconciler`: Name of the reconciler (e.g., "quality")
- `reason`: Exclusion reason (e.g., "high_error_rate")

**Example**:
```prometheus
# HELP nexus_reconciler_exclusions_total Backend exclusions by reconciler
# TYPE nexus_reconciler_exclusions_total counter
nexus_reconciler_exclusions_total{reconciler="quality",reason="high_error_rate"} 15
nexus_reconciler_exclusions_total{reconciler="quality",reason="insufficient_data"} 3
```

---

## Quality Loop Metrics

### nexus_quality_recompute_duration_seconds

**Type**: Histogram  
**Description**: Duration of quality metrics recomputation  
**Buckets**: `[0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1, +Inf]`

**Example**:
```prometheus
# HELP nexus_quality_recompute_duration_seconds Quality recompute duration
# TYPE nexus_quality_recompute_duration_seconds histogram
nexus_quality_recompute_duration_seconds_bucket{le="0.001"} 50
nexus_quality_recompute_duration_seconds_bucket{le="0.01"} 200
nexus_quality_recompute_duration_seconds_sum 0.5
nexus_quality_recompute_duration_seconds_count 210
```

---

## PromQL Query Examples

### Alerting Rules

```yaml
# Alert on high error rate
- alert: BackendHighErrorRate
  expr: nexus_agent_error_rate > 0.3
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "Backend {{ $labels.agent_id }} has high error rate"
    description: "Error rate is {{ $value | humanizePercentage }}"

# Alert on low success rate
- alert: BackendLowSuccessRate
  expr: nexus_agent_success_rate_24h < 0.9
  for: 15m
  labels:
    severity: critical
  annotations:
    summary: "Backend {{ $labels.agent_id }} has low success rate"
    description: "24h success rate is {{ $value | humanizePercentage }}"

# Alert on slow TTFT
- alert: BackendSlowTTFT
  expr: |
    (nexus_agent_ttft_seconds_sum / nexus_agent_ttft_seconds_count) > 3
  for: 10m
  labels:
    severity: warning
  annotations:
    summary: "Backend {{ $labels.agent_id }} has slow TTFT"
    description: "Average TTFT is {{ $value }}s"
```

### Dashboard Queries

```promql
# Fleet-wide error rate (average across all backends)
avg(nexus_agent_error_rate)

# Top 5 slowest backends by average TTFT
topk(5, nexus_agent_ttft_seconds_sum / nexus_agent_ttft_seconds_count)

# Request volume per backend (last 1 hour)
sum(nexus_agent_request_count_1h) by (agent_id)

# Quality score (composite metric)
# Higher is better: high success rate, low error rate, low TTFT
(nexus_agent_success_rate_24h * (1 - nexus_agent_error_rate)) / 
  ((nexus_agent_ttft_seconds_sum / nexus_agent_ttft_seconds_count) + 0.1)

# P95 TTFT across all backends
histogram_quantile(0.95, 
  sum(rate(nexus_agent_ttft_seconds_bucket[5m])) by (le)
)

# Backends with insufficient data (< 10 requests in last hour)
nexus_agent_request_count_1h < 10
```

---

## Metric Update Frequency

**Quality gauges** (`error_rate_1h`, `success_rate_24h`, `request_count_1h`):
- Updated every `metrics_interval_seconds` (default 30 seconds)
- Computed from rolling window of raw request outcomes

**TTFT histogram**:
- Updated on every request completion (real-time)

**Reconciler metrics**:
- Updated on every routing decision (per-request)

**Scrape Recommendations**:
- Prometheus scrape interval: 15 seconds
- Quality metrics may lag up to `metrics_interval_seconds` behind reality

---

## Label Cardinality

**agent_id cardinality**: `O(backends × models)`

**Example**:
- 10 backends × 20 models = 200 unique agent_id values
- 4 quality metrics per agent = 800 time series
- Plus histograms: ~1200 total time series (acceptable)

**Label Sanitization**:
- Invalid characters replaced with underscore (`_`)
- Example: `backend-prod:8080/llama3` → `backend_prod_8080_llama3`

---

## Backward Compatibility

This contract extends the existing Nexus metrics without breaking changes:
- All existing metrics remain unchanged
- New quality metrics are additive
- Backwards compatible with existing Prometheus dashboards

---

## References

- Prometheus naming conventions: https://prometheus.io/docs/practices/naming/
- Histogram best practices: https://prometheus.io/docs/practices/histograms/
- PromQL documentation: https://prometheus.io/docs/prometheus/latest/querying/basics/
