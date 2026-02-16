# Quickstart: F14 Inference Budget Management

**Feature**: Inference Budget Management  
**Branch**: `016-inference-budget-mgmt`  
**Date**: 2025-01-24

## Overview

This guide shows how to configure and test budget management with cost-aware routing. You'll learn to set budget limits, monitor spending via Prometheus metrics, and observe graceful degradation as limits are approached.

---

## Prerequisites

- Nexus built from `016-inference-budget-mgmt` branch
- At least one cloud backend configured (OpenAI or Anthropic)
- At least one local backend available (Ollama)
- Prometheus for metrics scraping (optional but recommended)

---

## Configuration

### Basic Configuration (Soft Limit Only)

Create `nexus.toml` with budget settings:

```toml
[routing.budget]
# Monthly budget limit in USD
monthly_limit_usd = 100.0

# Soft limit threshold (percentage of monthly limit)
# At 80%, routing shifts to prefer local agents
soft_limit_percent = 80.0

# Reconciliation interval (how often to check spending)
reconciliation_interval_secs = 60
```

**Start Nexus:**
```bash
nexus serve --config nexus.toml
```

**Expected behavior:**
- Spending tracked automatically per request
- At $80 (80% utilization), routing prefers local agents
- Cloud agents still available as fallback
- Budget status visible at `GET /v1/stats`

---

### Strict Budget Enforcement (Hard Limit)

Add hard limit action to block cloud agents when budget is exhausted:

```toml
[routing.budget]
monthly_limit_usd = 100.0
soft_limit_percent = 80.0
hard_limit_action = "block_cloud"  # or "warn" (default) or "block_all"
reconciliation_interval_secs = 60
```

**Hard limit actions:**
- `warn` - Log warning but allow all requests (default)
- `block_cloud` - Block cloud agents (OpenAI, Anthropic), keep local agents
- `block_all` - Block all agents → requests fail with 503

**Testing hard limit:**
```bash
# Send requests until budget exhausted
for i in {1..200}; do
  curl -X POST http://localhost:8000/v1/chat/completions \
    -H "Content-Type: application/json" \
    -d '{"model": "gpt-4-turbo", "messages": [{"role": "user", "content": "Hello"}]}'
done

# After $100 spent, cloud requests should fail
curl -v http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-4-turbo", "messages": [{"role": "user", "content": "Test after limit"}]}'

# Expected: 503 Service Unavailable (no cloud agents available)
# Or: Success if local agent can handle the model
```

---

## Testing Scenarios

### Scenario 1: Zero-Config (No Budget)

**Config:** Omit `[routing.budget]` section entirely

**Expected:**
- No budget enforcement
- All requests proceed normally
- No budget metrics emitted
- Response headers do not include `X-Nexus-Budget-*`

**Verify:**
```bash
curl http://localhost:8000/v1/stats | jq '.budget'
# Expected: null (budget field not present when disabled)
```

---

### Scenario 2: Soft Limit Triggers Local Preference

**Setup:**
```toml
[routing.budget]
monthly_limit_usd = 10.0  # Low limit for quick testing
soft_limit_percent = 75.0
```

**Test steps:**
1. Send $5 worth of requests (50% utilization) → Normal routing
2. Send $3 more (80% utilization) → Soft limit triggers
3. Observe routing shift to local agents

**Commands:**
```bash
# Step 1: Below soft limit
for i in {1..10}; do
  curl -X POST http://localhost:8000/v1/chat/completions \
    -H "Content-Type: application/json" \
    -d '{"model": "gpt-4-turbo", "messages": [{"role": "user", "content": "Generate 500 tokens of text"}]}' \
    > /dev/null 2>&1
done

# Check status (should be Normal)
curl -s http://localhost:8000/v1/stats | jq '.budget.status'
# Expected: "Normal"

# Step 2: Cross soft limit
for i in {1..5}; do
  curl -X POST http://localhost:8000/v1/chat/completions \
    -H "Content-Type: application/json" \
    -d '{"model": "gpt-4-turbo", "messages": [{"role": "user", "content": "Generate 500 tokens"}]}' \
    > /dev/null 2>&1
done

# Check status again (should be SoftLimit)
curl -s http://localhost:8000/v1/stats | jq '.budget.status'
# Expected: "SoftLimit"

# Step 3: Verify response headers
curl -i http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "llama3", "messages": [{"role": "user", "content": "Hello"}]}'

# Expected headers:
# X-Nexus-Budget-Status: SoftLimit
# X-Nexus-Budget-Utilization: 85.3
# X-Nexus-Backend-Type: local
```

**Verification:**
- Logs show "BudgetReconciler: soft limit reached, SchedulerReconciler will prefer local agents"
- Subsequent requests routed to local agents (check `X-Nexus-Backend-Type` header)
- `/v1/stats` shows `"status": "SoftLimit"`

---

### Scenario 3: Month Rollover Reset

**Setup:**
```toml
[routing.budget]
monthly_limit_usd = 100.0
reconciliation_interval_secs = 10  # Faster reconciliation for testing
```

**Test steps:**
1. Accumulate spending in current month
2. Manually advance system time to next month (for testing)
3. Verify budget counter resets

**Commands:**
```bash
# Step 1: Accumulate spending
for i in {1..50}; do
  curl -X POST http://localhost:8000/v1/chat/completions \
    -H "Content-Type: application/json" \
    -d '{"model": "gpt-4-turbo", "messages": [{"role": "user", "content": "Hello"}]}' \
    > /dev/null 2>&1
done

# Check spending
curl -s http://localhost:8000/v1/stats | jq '.budget.current_spending_usd'
# Expected: > 0 (some spending recorded)

# Step 2: Simulate month rollover (requires system clock change or wait until real rollover)
# NOTE: In production, this happens automatically on the 1st of each month

# Step 3: Verify reset after rollover
curl -s http://localhost:8000/v1/stats | jq '.budget'
# Expected: current_spending_usd back to 0.0, billing_month updated
```

**Production verification:**
- Check logs for "Budget month rollover, resetting spending"
- Prometheus metric `nexus_budget_events_total{event_type="month_rollover"}` increments

---

### Scenario 4: Token Counting Accuracy

**Objective:** Verify exact vs approximation vs heuristic token counting

**Test requests:**
```bash
# OpenAI model (exact tiktoken counting)
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4-turbo",
    "messages": [{"role": "user", "content": "The quick brown fox jumps over the lazy dog"}]
  }'

# Anthropic model (approximation with cl100k_base)
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-opus-20240229",
    "messages": [{"role": "user", "content": "The quick brown fox jumps over the lazy dog"}]
  }'

# Unknown model (heuristic with 1.15x multiplier)
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "unknown-model-v1",
    "messages": [{"role": "user", "content": "The quick brown fox jumps over the lazy dog"}]
  }'
```

**Verification via Prometheus:**
```bash
curl -s http://localhost:9090/metrics | grep nexus_token_count_tier_total
```

**Expected output:**
```prometheus
nexus_token_count_tier_total{tier="exact",model="gpt_4_turbo"} 1
nexus_token_count_tier_total{tier="approximation",model="claude_3_opus_20240229"} 1
nexus_token_count_tier_total{tier="heuristic",model="unknown_model_v1"} 1
```

---

## Monitoring with Prometheus

### Metrics Endpoint

```bash
curl http://localhost:8000/metrics
```

**Key metrics to monitor:**
```prometheus
# Budget status (0=Normal, 1=SoftLimit, 2=HardLimit)
nexus_budget_status{billing_month="2024-01"} 0

# Current spending
nexus_budget_spending_usd{billing_month="2024-01"} 45.23

# Utilization percentage
nexus_budget_utilization_percent{billing_month="2024-01"} 45.23

# Per-request cost distribution
nexus_cost_per_request_usd_bucket{model="gpt_4_turbo",backend_type="cloud",le="0.01"} 120
nexus_cost_per_request_usd_bucket{model="gpt_4_turbo",backend_type="cloud",le="0.1"} 450

# Token counting tier breakdown
nexus_token_count_tier_total{tier="exact",model="gpt_4_turbo"} 12500
nexus_token_count_tier_total{tier="approximation",model="claude_3_opus"} 3400
nexus_token_count_tier_total{tier="heuristic",model="unknown_model"} 120
```

### PromQL Queries

**Current budget utilization:**
```promql
nexus_budget_utilization_percent
```

**Spending rate (USD per hour):**
```promql
rate(nexus_budget_spending_usd[1h]) * 3600
```

**P95 request cost:**
```promql
histogram_quantile(0.95, rate(nexus_cost_per_request_usd_bucket[5m]))
```

**Token counting accuracy (percentage exact):**
```promql
sum(rate(nexus_token_count_tier_total{tier="exact"}[5m])) 
/ 
sum(rate(nexus_token_count_tier_total[5m])) * 100
```

### Grafana Dashboard

Import dashboard JSON from `specs/016-inference-budget-mgmt/grafana/budget-dashboard.json` (to be created separately if needed).

**Key panels:**
1. Budget utilization gauge (0-100%)
2. Spending timeline (cumulative)
3. Cost distribution heatmap
4. Token counting tier pie chart
5. Budget status timeline (Normal/Soft/Hard)

---

## API Response Headers

When budget utilization exceeds soft limit, responses include:

```http
HTTP/1.1 200 OK
X-Nexus-Backend-Type: local
X-Nexus-Route-Reason: budget-soft-limit
X-Nexus-Cost-Estimated: 0.0245
X-Nexus-Budget-Status: SoftLimit
X-Nexus-Budget-Utilization: 82.3
X-Nexus-Budget-Remaining: 17.75
```

**Header descriptions:**
- `X-Nexus-Budget-Status`: Current status (Normal/SoftLimit/HardLimit)
- `X-Nexus-Budget-Utilization`: Percentage of budget used
- `X-Nexus-Budget-Remaining`: USD remaining in current month
- `X-Nexus-Cost-Estimated`: Estimated cost for this request

---

## Troubleshooting

### Problem: Budget not enforcing

**Check:**
```bash
# Verify budget is configured
curl -s http://localhost:8000/v1/stats | jq '.budget'

# Check reconciliation loop is running
curl -s http://localhost:8000/metrics | grep nexus_budget_spending
```

**Solution:** Ensure `monthly_limit_usd` is set in config (not null)

---

### Problem: Spending resets unexpectedly

**Check logs:**
```bash
grep "Budget month rollover" /var/log/nexus.log
```

**Solution:** This is expected behavior on month boundaries (1st of each month UTC). To preserve spending across restarts, future enhancement will add optional file-based persistence.

---

### Problem: Token counting inaccurate

**Verify tier distribution:**
```bash
curl -s http://localhost:8000/metrics | grep nexus_token_count_tier_total
```

**Expected:** Majority should be "exact" or "approximation" for known models. High "heuristic" count indicates many unknown models.

**Solution:** Check model names match expected patterns (e.g., "gpt-4-turbo", "claude-3-opus"). Unknown models fall back to conservative heuristic (acceptable per spec).

---

### Problem: Hard limit not blocking requests

**Check hard_limit_action config:**
```bash
curl -s http://localhost:8000/v1/stats | jq '.budget.hard_limit_action'
```

**Expected:** "BlockCloud" or "BlockAll" (not "Warn")

**Solution:** Update config and restart Nexus.

---

## Performance Validation

### Latency Impact

Measure P95 latency before and after budget feature:

```bash
# Benchmark without budget
ab -n 1000 -c 10 -p request.json -T application/json http://localhost:8000/v1/chat/completions

# Benchmark with budget enabled
# (same command, check P95 difference)
```

**Expected:** <200ms overhead for token counting (per SC-007)

### Reconciliation Overhead

Check reconciliation loop performance:

```promql
rate(nexus_reconciler_duration_seconds_sum{reconciler="BudgetReconciler"}[5m]) 
/ 
rate(nexus_reconciler_duration_seconds_count{reconciler="BudgetReconciler"}[5m])
```

**Expected:** <10ms per reconciliation

---

## Next Steps

1. **Production deployment**: Set realistic `monthly_limit_usd` based on budget
2. **Alert setup**: Configure Prometheus alerts for SoftLimit/HardLimit
3. **Dashboard**: Import Grafana dashboard for visualization
4. **Review logs**: Monitor for "Budget reconciliation completed" messages
5. **Tune thresholds**: Adjust `soft_limit_percent` based on traffic patterns

---

**Related Documentation:**
- [Feature Spec](spec.md) - Full requirements and acceptance criteria
- [Data Model](data-model.md) - Entity relationships and validation rules
- [Metrics Spec](contracts/metrics.yml) - Prometheus metric definitions
- [Stats API Schema](contracts/stats-api.json) - /v1/stats JSON format
