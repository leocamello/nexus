# Research: Inference Budget Management

**Feature**: F14 - Inference Budget Management  
**Date**: 2025-01-22  
**Phase**: 0 (Research & Outline)

## Overview

This document captures research findings for implementing cost-aware routing with budget enforcement. The primary goal is to prevent runaway cloud inference costs while maintaining service availability through graceful degradation.

## Research Questions

### Q1: Tokenization Strategy for v0.3 MVP

**Question**: Should we use exact tokenizers (tiktoken-rs, tokenizers crate) or heuristic counting (chars/4) for the v0.3 MVP?

**Decision**: Use heuristic tokenization (chars/4) for v0.3 MVP, defer exact tokenizers to v0.4.

**Rationale**:
- **Avoids new dependencies**: User explicitly requested NO tiktoken-rs or tokenizers crate in v0.3
- **Faster delivery**: Heuristic is already implemented in InferenceAgent::count_tokens() default trait method
- **Adequate for budget enforcement**: 25% accuracy variance is acceptable for soft/hard limit triggers (80%/100%)
- **Conservative multiplier**: Apply 1.15x safety margin to heuristic counts for unknown models
- **Clear upgrade path**: v0.4 can add exact tokenizers without breaking changes

**Alternatives Considered**:
1. **tiktoken-rs for OpenAI models only**: Rejected to avoid partial dependency (all-or-nothing approach cleaner)
2. **External tokenization API**: Rejected due to latency (would add 50-200ms per request) and external dependency
3. **Pre-computed token tables**: Rejected due to maintenance burden (prompt templates change frequently)

**v0.4 Tokenizer Plan**:
- OpenAI models: `tiktoken-rs` with o200k_base (GPT-4) or cl100k_base (GPT-3.5)
- Anthropic models: `tiktoken-rs` with cl100k_base approximation (documented 5-10% variance)
- Llama/Mistral models: `tokenizers` crate with SentencePiece
- Unknown models: Fall back to heuristic with 1.15x multiplier

---

### Q2: Model Pricing Tables

**Question**: How do we obtain and maintain accurate pricing for OpenAI, Anthropic, and local models?

**Decision**: Hardcode pricing tables in Rust as constants, updated manually with releases.

**Rationale**:
- **No external API calls**: Pricing is static and changes infrequently (quarterly at most)
- **Fast lookups**: HashMap<model_pattern, (input_price_per_1k, output_price_per_1k)> for O(1) access
- **Regex patterns**: Match model names like "gpt-4-*", "claude-3-*" to handle variants
- **Conservative estimates**: For unknown models, default to highest tier pricing ($30/1M input, $60/1M output)

**Pricing Structure** (as of Jan 2025):

| Provider | Model | Input ($/1M tokens) | Output ($/1M tokens) |
|----------|-------|---------------------|----------------------|
| OpenAI | gpt-4-turbo | $10.00 | $30.00 |
| OpenAI | gpt-4 | $30.00 | $60.00 |
| OpenAI | gpt-3.5-turbo | $0.50 | $1.50 |
| Anthropic | claude-3-opus | $15.00 | $75.00 |
| Anthropic | claude-3-sonnet | $3.00 | $15.00 |
| Anthropic | claude-3-haiku | $0.25 | $1.25 |
| Local | all models | $0.00 | $0.00 |

**Alternatives Considered**:
1. **Fetch pricing from provider APIs**: Rejected because no standard pricing API exists (would require web scraping)
2. **Load from TOML config**: Rejected to avoid operator burden (most users won't customize pricing)
3. **External pricing service**: Rejected due to latency and external dependency

**Maintenance Strategy**:
- Monthly review of OpenAI/Anthropic pricing pages
- Update constants in `src/control/budget/pricing.rs` when changes detected
- Include pricing version in logs for audit trail

---

### Q3: Budget Reconciliation Pattern

**Question**: How do we track spending atomically across concurrent requests while maintaining <1ms routing overhead?

**Decision**: Use atomic counters (Arc<AtomicU64>) for current spending, background reconciliation loop at 60-second intervals.

**Rationale**:
- **Lock-free updates**: AtomicU64::fetch_add() for sub-microsecond increments
- **Acceptable overage**: Up to [concurrent_requests] × [avg_cost] due to eventual consistency (documented trade-off)
- **No distributed consensus**: Single-instance design (no Raft/etcd needed)
- **Background reconciliation**: Adjust spending based on provider-reported costs when available (future enhancement)

**Reconciliation Loop Responsibilities**:
1. Check if billing cycle has reset (first day of month UTC)
2. Reset spending counter to $0.00 if new cycle
3. (Future) Query provider APIs for actual costs and adjust counter
4. (Future) Detect spending anomalies (sudden spikes) and emit alerts

**Concurrency Model**:
```rust
// In-memory state
struct BudgetState {
    current_spending_cents: Arc<AtomicU64>,  // Stored as cents to avoid float atomics
    monthly_limit_cents: u64,
    soft_limit_percent: u8,
    hard_limit_action: HardLimitAction,
}

// Per-request cost tracking (in reconciler)
let cost_cents = (estimated_cost_usd * 100.0) as u64;
state.current_spending_cents.fetch_add(cost_cents, Ordering::Relaxed);

// Budget status calculation (lock-free read)
let current = state.current_spending_cents.load(Ordering::Relaxed);
let percentage = (current as f64 / state.monthly_limit_cents as f64) * 100.0;
```

**Alternatives Considered**:
1. **Mutex-protected counter**: Rejected due to contention (would serialize all requests)
2. **Per-request locking**: Rejected due to 10-50μs overhead per lock acquisition
3. **Database transactions**: Rejected (Nexus is stateless by design, no database)
4. **Redis atomic counters**: Rejected (external dependency violates Constitution Principle VII)

---

### Q4: Configuration Schema

**Question**: What budget parameters should be configurable, and what should be hardcoded?

**Decision**: Expose only essential knobs in nexus.toml, hardcode internal constants.

**Configurable** (in nexus.toml):
```toml
[budget]
# Monthly spending limit in USD (required to enable budget enforcement)
monthly_limit = 100.00

# Percentage threshold for soft limit warning (default: 80)
soft_limit_percent = 80

# Action when hard limit (100%) is reached:
# - "local-only": Route only to local agents (block cloud)
# - "queue": Queue requests requiring cloud agents (future: task queue)
# - "reject": Return 429 error for requests requiring cloud agents
hard_limit_action = "local-only"

# Billing cycle start day (1-31, default: 1)
# Budget resets on this day each month at 00:00 UTC
billing_cycle_start_day = 1
```

**Hardcoded** (not configurable):
- Pricing tables (updated with Nexus releases)
- Reconciliation loop interval (60 seconds)
- Heuristic multiplier for unknown models (1.15x)
- Token count tier labels ("exact", "approximation", "estimated")

**Rationale**:
- **Minimal configuration surface**: Operators only configure business logic (limits, actions), not implementation details
- **Sane defaults**: If `[budget]` section is omitted, budget enforcement is disabled (BudgetStatus::Normal for all requests)
- **Validation at startup**: monthly_limit >= 0.0, soft_limit_percent in 0-100, hard_limit_action is valid enum

**Alternatives Considered**:
1. **Per-provider budgets**: Rejected as over-engineered for v0.3 (single monthly limit is sufficient)
2. **Per-model budgets**: Rejected for same reason
3. **Configurable pricing tables**: Rejected to avoid operator maintenance burden

---

### Q5: Prometheus Metrics Design

**Question**: What metrics are needed for budget observability and alerting?

**Decision**: Add 6 budget-specific metrics to existing Prometheus exporter.

**Metrics Schema**:

| Metric Name | Type | Labels | Description |
|-------------|------|--------|-------------|
| `nexus_budget_current_spending_usd` | Gauge | none | Current monthly spending (resets on billing cycle) |
| `nexus_budget_limit_usd` | Gauge | none | Configured monthly limit |
| `nexus_budget_percent_used` | Gauge | none | Percentage of budget consumed (0-100+) |
| `nexus_budget_requests_blocked_total` | Counter | `reason` | Requests blocked by budget (hard limit) |
| `nexus_budget_soft_limit_activations_total` | Counter | none | Times soft limit (80%) was triggered |
| `nexus_budget_hard_limit_activations_total` | Counter | none | Times hard limit (100%) was triggered |
| `nexus_cost_estimate_usd` | Histogram | `provider`, `model`, `tier` | Per-request cost estimates |

**Prometheus Alerts** (examples for Grafana):
```yaml
# Alert when 70% of budget consumed (advance warning)
- alert: BudgetNearing70Percent
  expr: nexus_budget_percent_used > 70
  for: 5m
  annotations:
    summary: "Budget utilization at {{ $value }}%"

# Alert when soft limit reached
- alert: BudgetSoftLimitReached
  expr: rate(nexus_budget_soft_limit_activations_total[5m]) > 0
  annotations:
    summary: "Budget soft limit reached, preferring local agents"

# Alert when hard limit reached (critical)
- alert: BudgetHardLimitReached
  expr: rate(nexus_budget_hard_limit_activations_total[1m]) > 0
  annotations:
    summary: "Budget hard limit reached, cloud requests blocked"
```

**Rationale**:
- **Leverage existing metrics infrastructure**: Nexus already has Prometheus exporter (`GET /metrics`)
- **Minimal storage overhead**: 6 metrics × 8 bytes = 48 bytes in-memory
- **Histogram for cost distribution**: Enables p50/p95/p99 cost analysis per model

**Alternatives Considered**:
1. **StatsD push model**: Rejected (Nexus has no external push dependencies)
2. **Custom JSON endpoint**: Rejected (Prometheus is industry standard for observability)
3. **Per-backend cost metrics**: Deferred to v0.4 (adds 10+ labels, increases cardinality)

---

## Technology Choices

### Budget State Management
**Choice**: `Arc<AtomicU64>` for current spending counter  
**Justification**: Lock-free, sub-microsecond updates, Rust standard library (no new dependencies)

### Background Tasks
**Choice**: `tokio::spawn()` for reconciliation loop  
**Justification**: Already using Tokio runtime, no new scheduler needed

### Configuration Parsing
**Choice**: `serde` + `toml` (existing dependencies)  
**Justification**: Consistent with existing nexus.toml configuration

### Metrics Export
**Choice**: `metrics` crate with Prometheus exporter (existing)  
**Justification**: Already integrated, no new dependencies

---

## Implementation Phases

### Phase 1: Basic Budget Tracking (P1)
- Add `BudgetConfig` to `NexusConfig`
- Implement cost estimation in `BudgetReconciler::reconcile()`
- Add Prometheus metrics for spending/limits/percentage
- Test: Verify cost calculation accuracy with mock backends

### Phase 2: Soft Limit Enforcement (P2)
- Implement BudgetStatus logic (Normal/SoftLimit/HardLimit)
- Modify routing to prefer local agents when BudgetStatus::SoftLimit
- Add warning logs and soft_limit_activations counter
- Test: Verify local-first routing at 80% budget

### Phase 3: Hard Limit Enforcement (P2)
- Implement hard_limit_action: local-only, queue, reject
- Filter cloud backends when BudgetStatus::HardLimit + action=local-only
- Return 429 errors when action=reject
- Add hard_limit_activations counter
- Test: Verify cloud blocking at 100% budget

### Phase 4: Billing Cycle Reset (P2)
- Implement background reconciliation loop
- Detect first-of-month UTC and reset spending counter
- Log budget reset events
- Test: Mock time advancement, verify reset behavior

### Phase 5: v0.4 Exact Tokenization (P3 - future)
- Add tiktoken-rs and tokenizers dependencies
- Implement per-provider tokenizers in agent implementations
- Update TokenCount enum with "exact" tier
- Verify accuracy against provider-reported token counts

---

## Open Questions

### Q6: How to handle queued requests (hard_limit_action="queue")?
**Status**: Deferred to implementation phase  
**Options**:
1. In-memory VecDeque with max queue size (simple, no persistence)
2. Reject immediately with 429 + Retry-After header (simpler, let clients retry)
3. Background task to process queue when budget resets (complex state management)

**Recommendation**: Start with option 2 (reject with Retry-After), add queuing in v0.4 if demand exists.

### Q7: Should we track actual costs from provider invoices?
**Status**: Deferred to v0.4  
**Options**:
1. Poll OpenAI/Anthropic usage APIs daily (requires API keys, adds complexity)
2. Manual reconciliation by operators (upload invoice CSVs)
3. Trust estimates only (simpler, 10-20% variance acceptable)

**Recommendation**: Option 3 for v0.3, revisit in v0.4 based on user feedback.

### Q8: What happens when billing_cycle_start_day > days in current month?
**Example**: billing_cycle_start_day=31, current month is February (28/29 days)  
**Recommendation**: Reset on last day of month if configured day doesn't exist. Log warning.

---

## Risks & Mitigations

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Heuristic tokenization inaccuracy | Budget overruns by 10-30% | High | Apply 1.15x multiplier, document variance in quickstart.md |
| Race condition on spending counter | Multiple requests exceed hard limit | Medium | Document acceptable overage in spec, add monitoring alert |
| Pricing tables become stale | Incorrect cost estimates | Medium | Monthly review process, version pricing in logs |
| Billing cycle misconfiguration | Budget never resets | Low | Validation at startup, log reset events prominently |
| Background loop crashes | Budget never resets | Low | Use tokio::spawn with error logging, restart on panic |

---

## Success Metrics

1. **Cost Estimation Accuracy**: Heuristic within 30% of actual provider-reported tokens (measured in v0.4 when exact tokenizers added)
2. **Routing Overhead**: Budget check adds < 0.2ms to request latency (p95)
3. **Memory Overhead**: < 1KB per backend for pricing data, < 100 bytes for budget state
4. **Soft Limit Effectiveness**: ≥90% of requests routed to local agents when at 80% budget
5. **Hard Limit Enforcement**: 0% cloud requests when at 100% budget with local-only action

---

## References

- [OpenAI Pricing](https://openai.com/pricing) (snapshot: Jan 2025)
- [Anthropic Pricing](https://www.anthropic.com/pricing) (snapshot: Jan 2025)
- [tiktoken-rs GitHub](https://github.com/zurawiki/tiktoken-rs) (for v0.4)
- [HuggingFace tokenizers](https://github.com/huggingface/tokenizers) (for v0.4)
- Nexus Constitution: Principle VII (Local-First), Principle IX (Explicit Contracts)
