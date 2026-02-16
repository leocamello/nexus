# Prometheus Metrics Contract

## Budget Management Metrics

This document defines the Prometheus metrics exposed by the Budget Management feature (F14).

All metrics are exposed at `GET /metrics` in Prometheus text format.

---

## Gauges (Current State)

### nexus_budget_current_spending_usd
**Type**: Gauge  
**Unit**: USD  
**Description**: Current monthly spending in US dollars. Resets to 0.00 on billing cycle start day.

**Labels**: None

**Example**:
```
# HELP nexus_budget_current_spending_usd Current monthly spending in USD
# TYPE nexus_budget_current_spending_usd gauge
nexus_budget_current_spending_usd 45.23
```

**Update Frequency**: Updated on every inference request (atomic increment)

---

### nexus_budget_limit_usd
**Type**: Gauge  
**Unit**: USD  
**Description**: Configured monthly budget limit in US dollars. Static value from `nexus.toml`.

**Labels**: None

**Example**:
```
# HELP nexus_budget_limit_usd Configured monthly budget limit in USD
# TYPE nexus_budget_limit_usd gauge
nexus_budget_limit_usd 100.00
```

**Update Frequency**: Set once at startup, updated only on config reload

---

### nexus_budget_percent_used
**Type**: Gauge  
**Unit**: Percentage (0-100+)  
**Description**: Percentage of monthly budget consumed. Can exceed 100% if hard limit is not enforced.

**Labels**: None

**Calculation**: `(current_spending / monthly_limit) * 100.0`

**Example**:
```
# HELP nexus_budget_percent_used Percentage of budget consumed (0-100+)
# TYPE nexus_budget_percent_used gauge
nexus_budget_percent_used 45.23
```

**Update Frequency**: Updated on every inference request

---

## Counters (Events)

### nexus_budget_requests_blocked_total
**Type**: Counter  
**Unit**: Count  
**Description**: Total number of requests blocked by budget enforcement (hard limit).

**Labels**:
- `reason`: Why the request was blocked
  - `hard_limit_local_only`: Blocked by hard limit with local-only action
  - `hard_limit_reject`: Blocked by hard limit with reject action
  - `hard_limit_queue`: Queued by hard limit with queue action (v0.4+)

**Example**:
```
# HELP nexus_budget_requests_blocked_total Total requests blocked by budget enforcement
# TYPE nexus_budget_requests_blocked_total counter
nexus_budget_requests_blocked_total{reason="hard_limit_local_only"} 12
nexus_budget_requests_blocked_total{reason="hard_limit_reject"} 3
```

**Update Frequency**: Incremented when request is blocked

---

### nexus_budget_soft_limit_activations_total
**Type**: Counter  
**Unit**: Count  
**Description**: Total number of times soft limit (default: 80%) was activated.

**Labels**: None

**Example**:
```
# HELP nexus_budget_soft_limit_activations_total Total times soft limit was activated
# TYPE nexus_budget_soft_limit_activations_total counter
nexus_budget_soft_limit_activations_total 5
```

**Update Frequency**: Incremented when budget crosses soft limit threshold (once per activation, not per request)

---

### nexus_budget_hard_limit_activations_total
**Type**: Counter  
**Unit**: Count  
**Description**: Total number of times hard limit (100%) was activated.

**Labels**: None

**Example**:
```
# HELP nexus_budget_hard_limit_activations_total Total times hard limit was activated
# TYPE nexus_budget_hard_limit_activations_total counter
nexus_budget_hard_limit_activations_total 2
```

**Update Frequency**: Incremented when budget crosses hard limit threshold (once per activation, not per request)

---

## Histograms (Distributions)

### nexus_cost_estimate_usd
**Type**: Histogram  
**Unit**: USD  
**Description**: Per-request cost estimates in US dollars. Includes input tokens, estimated output tokens, and total cost.

**Labels**:
- `provider`: Provider name (e.g., `openai`, `anthropic`, `local`)
- `model`: Model name (e.g., `gpt-4-turbo`, `claude-3-opus`, `llama3:70b`)
- `tier`: Token counting accuracy tier
  - `Exact`: Provider-specific tokenizer (v0.4+)
  - `Approximation`: Known variance tokenizer (e.g., Anthropic cl100k_base)
  - `Estimated`: Heuristic tokenizer (chars/4 * 1.15)

**Buckets**: `[0.0001, 0.001, 0.01, 0.1, 1.0, 10.0, 100.0, +Inf]`

**Example**:
```
# HELP nexus_cost_estimate_usd Per-request cost estimates in USD
# TYPE nexus_cost_estimate_usd histogram
nexus_cost_estimate_usd_bucket{provider="openai",model="gpt-4-turbo",tier="Estimated",le="0.0001"} 0
nexus_cost_estimate_usd_bucket{provider="openai",model="gpt-4-turbo",tier="Estimated",le="0.001"} 0
nexus_cost_estimate_usd_bucket{provider="openai",model="gpt-4-turbo",tier="Estimated",le="0.01"} 5
nexus_cost_estimate_usd_bucket{provider="openai",model="gpt-4-turbo",tier="Estimated",le="0.1"} 42
nexus_cost_estimate_usd_bucket{provider="openai",model="gpt-4-turbo",tier="Estimated",le="1.0"} 48
nexus_cost_estimate_usd_bucket{provider="openai",model="gpt-4-turbo",tier="Estimated",le="10.0"} 50
nexus_cost_estimate_usd_bucket{provider="openai",model="gpt-4-turbo",tier="Estimated",le="100.0"} 50
nexus_cost_estimate_usd_bucket{provider="openai",model="gpt-4-turbo",tier="Estimated",le="+Inf"} 50
nexus_cost_estimate_usd_sum{provider="openai",model="gpt-4-turbo",tier="Estimated"} 2.35
nexus_cost_estimate_usd_count{provider="openai",model="gpt-4-turbo",tier="Estimated"} 50
```

**Update Frequency**: Recorded on every inference request

---

## PromQL Query Examples

### Budget Utilization (Percentage)
```promql
(nexus_budget_current_spending_usd / nexus_budget_limit_usd) * 100
```

### Spending Rate (USD/minute)
```promql
rate(nexus_budget_current_spending_usd[5m])
```

### Average Cost Per Request
```promql
sum(rate(nexus_cost_estimate_usd_sum[5m])) / sum(rate(nexus_cost_estimate_usd_count[5m]))
```

### Cost by Provider (Top 5)
```promql
topk(5, sum by (provider) (rate(nexus_cost_estimate_usd_sum[5m])))
```

### Cost by Model (Top 10)
```promql
topk(10, sum by (model) (rate(nexus_cost_estimate_usd_sum[5m])))
```

### Requests Blocked Per Minute
```promql
rate(nexus_budget_requests_blocked_total[1m])
```

### Soft Limit Activations (Last 24h)
```promql
increase(nexus_budget_soft_limit_activations_total[24h])
```

### P95 Cost Estimate
```promql
histogram_quantile(0.95, sum(rate(nexus_cost_estimate_usd_bucket[5m])) by (le))
```

---

## Grafana Dashboard Panels

### Panel 1: Budget Gauge
**Type**: Gauge  
**Query**: `nexus_budget_percent_used`  
**Thresholds**:
- Green: 0-70%
- Yellow: 70-90%
- Red: 90-100%

---

### Panel 2: Spending Over Time
**Type**: Time series  
**Query 1**: `nexus_budget_current_spending_usd` (Current spending)  
**Query 2**: `nexus_budget_limit_usd` (Budget limit)

---

### Panel 3: Cost Distribution by Model
**Type**: Bar gauge  
**Query**: `sum by (model) (rate(nexus_cost_estimate_usd_sum[5m]))`

---

### Panel 4: Budget Status Timeline
**Type**: State timeline  
**Query**: 
```promql
(
  nexus_budget_percent_used < 80 => 0,  # Normal
  nexus_budget_percent_used >= 80 and nexus_budget_percent_used < 100 => 1,  # SoftLimit
  nexus_budget_percent_used >= 100 => 2  # HardLimit
)
```
**Value mappings**:
- 0 → Normal (green)
- 1 → SoftLimit (yellow)
- 2 → HardLimit (red)

---

### Panel 5: Blocked Requests
**Type**: Time series  
**Query**: `rate(nexus_budget_requests_blocked_total[1m])`

---

## Alert Rules

### BudgetNearing70Percent
```yaml
- alert: BudgetNearing70Percent
  expr: nexus_budget_percent_used > 70
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "Budget utilization at {{ $value }}%"
    description: "Consider reducing cloud usage or increasing budget"
```

---

### BudgetSoftLimitReached
```yaml
- alert: BudgetSoftLimitReached
  expr: rate(nexus_budget_soft_limit_activations_total[5m]) > 0
  for: 1m
  labels:
    severity: warning
  annotations:
    summary: "Budget soft limit reached, preferring local agents"
    description: "80% of budget consumed, routing shifted to local agents"
```

---

### BudgetHardLimitReached
```yaml
- alert: BudgetHardLimitReached
  expr: rate(nexus_budget_hard_limit_activations_total[1m]) > 0
  for: 1m
  labels:
    severity: critical
  annotations:
    summary: "Budget hard limit reached, cloud requests blocked"
    description: "100% of budget consumed. Action: {{ $labels.action }}"
```

---

### UnexpectedCostSpike
```yaml
- alert: UnexpectedCostSpike
  expr: rate(nexus_budget_current_spending_usd[5m]) > rate(nexus_budget_current_spending_usd[1h]) * 2
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "Spending rate increased 2x in last 5 minutes"
    description: "Possible runaway cost scenario detected"
```

---

## Compatibility

- **Prometheus Version**: 2.x+
- **Grafana Version**: 8.x+
- **Metric Format**: OpenMetrics (Prometheus text format)
- **Cardinality**: Low (< 100 unique label combinations expected)

---

## Testing Metrics

### Manual Testing
```bash
# Check metrics endpoint
curl http://localhost:8000/metrics | grep budget

# Query specific metric
curl http://localhost:8000/metrics | grep "nexus_budget_current_spending_usd"
```

### Integration Testing
```rust
#[tokio::test]
async fn test_budget_metrics_exposed() {
    let app = create_test_app().await;
    
    let response = app
        .get("/metrics")
        .send()
        .await
        .expect("Failed to get metrics");
    
    assert_eq!(response.status(), 200);
    
    let body = response.text().await.unwrap();
    assert!(body.contains("nexus_budget_current_spending_usd"));
    assert!(body.contains("nexus_budget_limit_usd"));
    assert!(body.contains("nexus_budget_percent_used"));
}
```

---

## References

- [Prometheus Metric Types](https://prometheus.io/docs/concepts/metric_types/)
- [OpenMetrics Specification](https://github.com/OpenObservability/OpenMetrics)
- [Grafana Prometheus Data Source](https://grafana.com/docs/grafana/latest/datasources/prometheus/)
