# Quickstart: Inference Budget Management

**Feature**: F14 - Inference Budget Management  
**Audience**: System operators and DevOps engineers  
**Version**: v0.3 MVP (heuristic tokenization)

## Overview

Nexus budget management prevents runaway cloud inference costs through:
- **Real-time cost tracking**: Estimate per-request costs based on token counts and model pricing
- **Graceful degradation**: Automatically prefer local agents when approaching budget limits
- **Hard limit enforcement**: Block or queue cloud requests when budget is exhausted
- **Prometheus metrics**: Monitor spending, utilization, and budget status in Grafana

This guide covers basic setup, configuration, monitoring, and troubleshooting.

---

## Quick Setup (5 minutes)

### 1. Enable Budget Enforcement

Add a `[budget]` section to your `nexus.toml`:

```toml
[budget]
# Monthly spending limit in USD (required to enable budget enforcement)
monthly_limit = 100.00

# Percentage threshold for soft limit warning (default: 80)
soft_limit_percent = 80

# Action when hard limit (100%) is reached:
# - "local-only": Route only to local agents (block cloud)
# - "queue": Queue requests requiring cloud agents (future: v0.4)
# - "reject": Return 429 error for requests requiring cloud agents
hard_limit_action = "local-only"

# Billing cycle start day (1-31, default: 1)
# Budget resets on this day each month at 00:00 UTC
billing_cycle_start_day = 1
```

### 2. Restart Nexus

```bash
nexus serve --config nexus.toml
```

You should see in the logs:
```
INFO nexus: Budget enforcement enabled: $100.00/month, soft limit 80%, action local-only
```

### 3. Verify Metrics

Check that budget metrics are exposed:

```bash
curl http://localhost:8000/metrics | grep budget
```

Expected output:
```
nexus_budget_current_spending_usd 0.00
nexus_budget_limit_usd 100.00
nexus_budget_percent_used 0.00
```

---

## How It Works

### Cost Estimation (v0.3 MVP)

Nexus uses **heuristic tokenization** for v0.3:
- **Algorithm**: `tokens = (text.length / 4) * 1.15`
- **Conservative multiplier**: 1.15x to avoid under-estimation
- **Accuracy**: ±30% variance compared to exact tokenizers (acceptable for budget enforcement)

**Example**:
```
Prompt: "Explain quantum computing in 200 words" (45 characters)
Input tokens: (45 / 4) * 1.15 = ~13 tokens
Output tokens: 13 * 0.5 = ~7 tokens (50% of input, heuristic)
Cost (GPT-4-turbo): (13 * $0.01/1K) + (7 * $0.03/1K) = $0.00034
```

**v0.4 upgrade**: Exact tokenizers (tiktoken-rs, HuggingFace tokenizers) will be added in v0.4 for audit-grade accuracy.

### Budget Status Transitions

```
Normal (0-79% spent)
  ↓ [spend reaches 80%]
SoftLimit (80-99% spent)
  ↓ [spend reaches 100%]
HardLimit (100%+ spent)
```

| Status | Routing Behavior | Cloud Requests |
|--------|------------------|----------------|
| **Normal** | Local-first, cloud for overflow | ✅ Allowed |
| **SoftLimit** | Strongly prefer local agents | ✅ Allowed (with warning) |
| **HardLimit** | Depends on `hard_limit_action` | ❌ Blocked (local-only) or 429 (reject) |

### Billing Cycle Reset

- **When**: First day of each month at 00:00 UTC (configurable via `billing_cycle_start_day`)
- **What**: Spending counter resets to $0.00, status returns to Normal
- **How**: Background reconciliation loop checks date every 60 seconds

---

## Configuration Reference

### Budget Settings

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `monthly_limit` | float or null | null | Monthly spending limit in USD. If null, budget enforcement is disabled. |
| `soft_limit_percent` | integer | 80 | Percentage threshold (0-100) for soft limit warning. |
| `hard_limit_action` | enum | local-only | Action when hard limit is reached: `local-only`, `queue`, `reject`. |
| `billing_cycle_start_day` | integer | 1 | Day of month (1-31) when billing cycle resets. |

### Hard Limit Actions

**local-only** (Recommended):
- Routes only to local agents (Ollama, vLLM, etc.)
- Blocks all cloud agents (OpenAI, Anthropic)
- Maintains service availability with reduced capacity
- Best for: Home labs, small teams with local GPU capacity

**queue** (Future: v0.4):
- Queues requests requiring cloud agents
- Processes queue when budget resets
- Currently returns 429 error (queuing not yet implemented)
- Best for: Batch workloads, non-urgent requests

**reject**:
- Returns 429 error immediately for cloud requests
- Logs rejection reason
- Best for: Strict cost control, fail-fast behavior

---

## Monitoring

### Prometheus Metrics

Add these metrics to your Grafana dashboard:

| Metric | Type | Description |
|--------|------|-------------|
| `nexus_budget_current_spending_usd` | gauge | Current monthly spending (resets on billing cycle) |
| `nexus_budget_limit_usd` | gauge | Configured monthly limit |
| `nexus_budget_percent_used` | gauge | Percentage of budget consumed (0-100+) |
| `nexus_budget_requests_blocked_total` | counter | Requests blocked by budget (hard limit) |
| `nexus_budget_soft_limit_activations_total` | counter | Times soft limit (80%) was triggered |
| `nexus_budget_hard_limit_activations_total` | counter | Times hard limit (100%) was triggered |
| `nexus_cost_estimate_usd` | histogram | Per-request cost estimates by provider/model |

### Sample Grafana Dashboard

**Panel 1: Budget Utilization**
```promql
(nexus_budget_current_spending_usd / nexus_budget_limit_usd) * 100
```

**Panel 2: Spending Over Time**
```promql
rate(nexus_budget_current_spending_usd[5m])
```

**Panel 3: Cost Distribution by Model**
```promql
histogram_quantile(0.95, sum(rate(nexus_cost_estimate_usd_bucket[5m])) by (model, le))
```

**Panel 4: Blocked Requests**
```promql
rate(nexus_budget_requests_blocked_total[5m])
```

### Alert Rules

**70% Budget Warning**:
```yaml
- alert: BudgetNearing70Percent
  expr: nexus_budget_percent_used > 70
  for: 5m
  annotations:
    summary: "Budget utilization at {{ $value }}%"
    description: "Consider reducing cloud usage or increasing budget"
```

**Soft Limit Reached**:
```yaml
- alert: BudgetSoftLimitReached
  expr: rate(nexus_budget_soft_limit_activations_total[5m]) > 0
  annotations:
    summary: "Budget soft limit reached, preferring local agents"
```

**Hard Limit Reached** (Critical):
```yaml
- alert: BudgetHardLimitReached
  expr: rate(nexus_budget_hard_limit_activations_total[1m]) > 0
  severity: critical
  annotations:
    summary: "Budget hard limit reached, cloud requests blocked"
    description: "Immediate action required: increase budget or wait for reset"
```

---

## Troubleshooting

### Problem: Budget never resets

**Symptoms**: Spending counter stays at 100% after billing cycle date

**Causes**:
1. Server timezone is not UTC
2. `billing_cycle_start_day` is misconfigured (e.g., 31 in February)
3. Reconciliation loop crashed

**Solutions**:
1. Check server time: `date -u` (should show UTC)
2. Verify config: `billing_cycle_start_day` should be 1-28 for reliable monthly resets
3. Check logs for reconciliation loop errors: `journalctl -u nexus | grep -i budget`

### Problem: Cost estimates seem inaccurate

**Symptoms**: Prometheus metrics show costs 20-40% different from actual provider invoices

**Causes**:
1. Heuristic tokenization has ±30% variance (expected in v0.3)
2. Output token estimates are incorrect (using 50% of input, may vary)
3. Pricing tables are stale (provider changed pricing)

**Solutions**:
1. **Expected behavior**: v0.3 uses heuristic tokenization. Upgrade to v0.4 for exact tokenizers.
2. Monitor discrepancies: If variance exceeds 40%, file issue with example prompts
3. Check pricing tables: Compare `src/control/budget/pricing.rs` with provider websites

### Problem: Requests blocked despite budget available

**Symptoms**: 429 errors when `nexus_budget_percent_used < 100`

**Causes**:
1. Race condition: Multiple concurrent requests exceeded budget simultaneously
2. Cost estimate is higher than available budget
3. Hard limit action is misconfigured

**Solutions**:
1. **Expected behavior**: Acceptable overage = [concurrent_requests] × [avg_cost]
2. Check cost estimate: `curl http://localhost:8000/metrics | grep cost_estimate`
3. Verify config: `hard_limit_action = "local-only"` (not "reject")

### Problem: Soft limit not preferring local agents

**Symptoms**: Cloud requests continue at 80-99% budget

**Causes**:
1. No local agents available (all unhealthy)
2. Request requires cloud-only capabilities (e.g., vision, 200K context)
3. Soft limit routing not implemented (check version)

**Solutions**:
1. Check local agent health: `curl http://localhost:8000/v1/models | jq '.data[] | select(.owned_by != "openai")'`
2. Review request capabilities: Some requests require cloud (expected behavior)
3. Verify version: Soft limit routing added in v0.3

---

## Best Practices

### 1. Set Conservative Budgets

Start with a low monthly limit and increase as needed:

```toml
[budget]
monthly_limit = 50.00  # Start low, monitor for 1 month
soft_limit_percent = 75  # Earlier warning (instead of 80%)
```

**Why**: Easier to increase budget than recover from runaway costs.

### 2. Monitor Daily

Set up Prometheus alerts for:
- 50% budget consumed (early warning)
- 70% budget consumed (plan ahead)
- 90% budget consumed (critical threshold)

### 3. Test Hard Limit Behavior

Before production, test hard limit enforcement:

```bash
# Set low budget
nexus serve --config nexus-test.toml  # monthly_limit = 1.00

# Exhaust budget
for i in {1..100}; do
  curl -X POST http://localhost:8000/v1/chat/completions \
    -d '{"model":"gpt-4","messages":[{"role":"user","content":"test"}]}'
done

# Verify 429 errors or local-only routing
```

### 4. Use Local-Only Action

For most use cases, `hard_limit_action = "local-only"` is best:
- Maintains availability with local agents
- Prevents cost overruns
- No manual intervention required

Only use `reject` if:
- No local agents available
- Strict cost control required
- Application can handle 429 errors gracefully

### 5. Plan for Billing Cycle Reset

Budget resets automatically, but plan ahead:
- If you hit hard limit on day 28, you have 2-3 days without cloud access
- Option 1: Increase `monthly_limit` mid-cycle (requires restart)
- Option 2: Use local agents for remaining days
- Option 3: Configure `billing_cycle_start_day` to align with your usage patterns

---

## Migration Guide

### From No Budget to Budget Enforcement

**Step 1**: Enable monitoring without enforcement
```toml
[budget]
monthly_limit = 1000000.00  # Very high limit (effectively disabled)
soft_limit_percent = 80
hard_limit_action = "local-only"
```

**Step 2**: Monitor for 1 month
- Track `nexus_budget_current_spending_usd` in Grafana
- Identify spending patterns (peak hours, expensive models)

**Step 3**: Set realistic limit
```toml
[budget]
monthly_limit = 150.00  # 150% of observed monthly spend
soft_limit_percent = 80
```

**Step 4**: Gradually reduce
- Each month, reduce limit by 10-20%
- Monitor soft/hard limit activations
- Adjust `soft_limit_percent` to control degradation point

### From v0.3 to v0.4 (Exact Tokenization)

**No configuration changes required**. v0.4 will automatically use exact tokenizers when available:

- OpenAI models → tiktoken-rs (o200k_base or cl100k_base)
- Anthropic models → tiktoken-rs (cl100k_base approximation)
- Llama/Mistral → HuggingFace tokenizers (SentencePiece)

**Expected improvements**:
- Cost estimates accurate within 1-5% (vs ±30% in v0.3)
- `token_count_tier` metric changes from "Estimated" to "Exact"

---

## FAQ

**Q: Can I set per-backend budgets?**  
A: Not in v0.3. Use a single monthly limit for all backends. Per-backend budgets planned for v0.5.

**Q: Can I set per-model budgets?**  
A: Not directly. Use routing priorities to control which models are used (cheaper models = higher priority).

**Q: What happens if I exceed budget mid-request?**  
A: In-flight requests continue processing (pre-authorized). New requests after budget exhaustion are blocked.

**Q: Can I manually reset the budget counter?**  
A: Not via API in v0.3. Restart Nexus to reset counter (temporary workaround). Manual reset API planned for v0.4.

**Q: Do local models count against budget?**  
A: No. Local models (Ollama, vLLM, llama.cpp) have zero cost. Only cloud providers (OpenAI, Anthropic) are charged.

**Q: How accurate are cost estimates?**  
A: ±30% in v0.3 (heuristic tokenization). Upgrade to v0.4 for 1-5% accuracy (exact tokenizers).

**Q: Can I disable budget enforcement without removing config?**  
A: Yes. Set `monthly_limit = null` or omit `[budget]` section entirely.

---

## Support

- **Documentation**: https://github.com/user/nexus/blob/main/docs/FEATURES.md#budget-management
- **Issues**: https://github.com/user/nexus/issues/new?template=bug_report.md
- **Discussions**: https://github.com/user/nexus/discussions

For commercial support, contact: support@nexus.io
