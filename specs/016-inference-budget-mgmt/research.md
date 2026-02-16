# Research: F14 Inference Budget Management

**Date**: 2025-01-24  
**Feature**: Inference Budget Management  
**Branch**: `016-inference-budget-mgmt`

## Overview

This document records research findings for implementing audit-grade token counting, billing cycle management, and budget monitoring metrics. All decisions are informed by the existing Control Plane PR infrastructure (BudgetReconciler, PricingTable) and Constitution principles (Precise Measurement, Explicit Contracts).

---

## R1: Tokenizer Selection Strategy

### Context
FR-005 requires exact tokenizers for OpenAI/Anthropic and FR-006 requires 1.15x conservative multiplier for unknown models. Need to determine which tokenizers to use for each provider.

### Decision
Implement three-tier tokenization strategy:

1. **Tier 1 - Exact (OpenAI)**:
   - Use `tiktoken-rs` with `o200k_base` encoding (GPT-4 Turbo, GPT-4o)
   - Use `tiktoken-rs` with `cl100k_base` encoding (GPT-3.5, GPT-4 base)
   - Already integrated in OpenAI agent (src/agent/openai.rs line 5, line 256)
   - Accuracy: Within 0-2 token variance from OpenAI's API

2. **Tier 2 - Approximation (Anthropic, Local)**:
   - **Anthropic Claude**: Use `tiktoken-rs` with `cl100k_base` (GPT-3.5 tokenizer) as approximation
     - Rationale: Claude uses similar BPE vocabulary to GPT-3.5
     - Variance: ~3-7% higher than actual (conservative)
   - **Local Models (Llama, Mistral)**: Use `tokenizers` crate with SentencePiece
     - Rationale: Most local models use SentencePiece tokenization
     - Variance: ~2-5% depending on model family

3. **Tier 3 - Heuristic Fallback (Unknown)**:
   - Use existing character-based heuristic: `text.len() / 4` with 1.15x multiplier
   - Already implemented in OpenAI agent (src/agent/openai.rs line 260)
   - Ensures conservative estimate (safe overspend protection)

### Rationale
- **Avoids complexity**: Reuses tiktoken-rs already in Cargo.toml
- **Constitution alignment**: Principle X (Precise Measurement) - "per-backend tokenizer registry for audit-grade token counting"
- **Graceful degradation**: Falls back gracefully for unknown models instead of failing

### Alternatives Considered
- **Use provider APIs for token counting**: REJECTED - adds network latency and external dependency (violates Constitution Principle VII - Local-First)
- **Single universal tokenizer**: REJECTED - inaccurate for multi-provider environment (violates FR-005)
- **No token counting, estimate by characters only**: REJECTED - fails FR-005 requirement for exact counting

### Implementation Notes
```rust
// Trait for tokenizer abstraction
trait Tokenizer: Send + Sync {
    fn count_tokens(&self, text: &str) -> Result<u32, TokenizerError>;
    fn tier(&self) -> TokenCountTier; // Exact, Approximation, Heuristic
}

// Registry maps model prefix -> Tokenizer implementation
struct TokenizerRegistry {
    matchers: Vec<(GlobMatcher, Arc<dyn Tokenizer>)>,
    fallback: Arc<dyn Tokenizer>,
}
```

### Testing Strategy
- Unit tests: Verify each tokenizer against known text samples
- Property tests: Ensure tier 3 is always >= tier 2 >= tier 1 (conservative ordering)
- Integration tests: Compare against provider API counts (manual validation, not automated)

---

## R2: Billing Cycle Reset Strategy

### Context
FR-007 requires configurable billing cycle day each month. Need to determine how to detect month rollover and reset spending counter without losing data or causing service disruption.

### Decision
Use **passive rollover detection** in reconciliation loop:

```rust
impl BudgetMetrics {
    fn current_month_key() -> String {
        chrono::Utc::now().format("%Y-%m").to_string()
    }
}

// In reconciliation loop (every 60 seconds)
fn reconcile_spending(&self) {
    let current_month = BudgetMetrics::current_month_key();
    
    self.budget_state.entry(GLOBAL_BUDGET_KEY.to_string())
        .and_modify(|metrics| {
            if metrics.month_key != current_month {
                tracing::info!(
                    old_month = %metrics.month_key,
                    new_month = %current_month,
                    final_spending = metrics.current_month_spending,
                    "Budget month rollover, resetting spending"
                );
                metrics.current_month_spending = 0.0;
                metrics.month_key = current_month.clone();
            }
            metrics.last_reconciliation_time = now;
        });
}
```

**This is already implemented in the Control Plane PR** (src/routing/reconciler/budget.rs lines 338-367). No changes needed - just wire it into serve.rs.

### Rationale
- **Simple**: No cron jobs, no timezone complexity
- **Zero-config**: Uses UTC month boundaries (aligns with cloud provider billing)
- **Graceful**: In-flight requests complete against old month, new requests start fresh
- **Testable**: Can mock time in tests to verify rollover behavior

### Alternatives Considered
- **Configurable billing_cycle_day (e.g., 1st, 15th of month)**: REJECTED for v1
  - Adds complexity with timezone handling and day-of-month edge cases
  - Spec lists "billing cycle day" in FR-007 but does not require it for P1/P2/P3 acceptance
  - Can add in future if multi-tenant billing requires it
- **Active cron-style scheduler**: REJECTED - adds dependency, violates simplicity
- **Persist billing history**: REJECTED for v1 - Constitution Principle VIII (Stateless by Design)

### Implementation Notes
BudgetConfig already has `reconciliation_interval_secs: u64` (default 60). Month rollover is detected within one reconciliation interval (acceptable per spec Assumption: "60-second reconciliation interval provides sufficient accuracy").

**Action**: Wire BudgetReconciliationLoop in cli/serve.rs at startup.

---

## R3: Prometheus Metrics Design

### Context
FR-009 requires budget status exposure and SC-004 requires <1% sampling error for Prometheus metrics. Need to design metric schema that supports monitoring, alerting, and dashboard visualization.

### Decision
Add the following metrics to existing `src/metrics/mod.rs`:

#### Gauges (Current State)
```prometheus
# Budget spending and utilization
nexus_budget_spending_usd{billing_month="2024-01"} 45.23
nexus_budget_utilization_percent{billing_month="2024-01"} 45.23
nexus_budget_limit_usd 100.0

# Budget status (0=Normal, 1=SoftLimit, 2=HardLimit)
nexus_budget_status{billing_month="2024-01"} 0
```

#### Histograms (Distribution)
```prometheus
# Per-request cost distribution
nexus_cost_per_request_usd_bucket{le="0.001"} 120
nexus_cost_per_request_usd_bucket{le="0.01"} 450
nexus_cost_per_request_usd_bucket{le="0.1"} 890
nexus_cost_per_request_usd_bucket{le="1.0"} 920
nexus_cost_per_request_usd_bucket{le="+Inf"} 1000

# Token counting latency (should be <50ms p95)
nexus_token_count_duration_seconds_bucket{tier="exact",le="0.01"} 450
nexus_token_count_duration_seconds_bucket{tier="exact",le="0.05"} 890
nexus_token_count_duration_seconds_bucket{tier="exact",le="0.1"} 950
```

#### Counters (Cumulative Events)
```prometheus
# Token counting by tier (for audit trail per FR-012)
nexus_token_count_tier_total{tier="exact",model="gpt-4-turbo"} 12500
nexus_token_count_tier_total{tier="approximation",model="claude-3-opus"} 3400
nexus_token_count_tier_total{tier="heuristic",model="unknown-model"} 120

# Budget events
nexus_budget_events_total{event_type="soft_limit_reached"} 3
nexus_budget_events_total{event_type="hard_limit_reached"} 1
nexus_budget_events_total{event_type="month_rollover"} 1
```

### Rationale
- **Prometheus-native**: Uses standard gauge/histogram/counter types
- **Grafana-ready**: Billing month label enables multi-month dashboards
- **Alert-friendly**: `nexus_budget_status` gauge enables simple alerts (> 0 = warning, > 1 = critical)
- **Audit-grade**: Token tier counter provides evidence for budget accuracy

### Alternatives Considered
- **Separate time series per status**: REJECTED - single gauge with enum value is cleaner
- **Push metrics to external system**: REJECTED - violates Constitution Principle VII (no external dependencies)

### Implementation Notes
Metrics are recorded in two places:
1. **BudgetReconciler::reconcile()**: Update gauges after budget status calculation
2. **BudgetReconciliationLoop::reconcile_spending()**: Update gauges and emit events on transitions
3. **Token counting**: Record histogram + counter in tokenizer implementations

**Existing infrastructure**: `src/metrics/mod.rs` already has `setup_metrics()` with custom histogram buckets (lines 158-198). Just add new metric registrations.

---

## R4: Response Header Strategy

### Context
FR-011 requires X-Nexus-Budget-Status response header when utilization exceeds soft limit. Need to determine header format and integration point.

### Decision
Add budget headers to existing X-Nexus-* header family:

```http
HTTP/1.1 200 OK
X-Nexus-Backend-Type: cloud
X-Nexus-Route-Reason: capacity-overflow
X-Nexus-Privacy-Zone: open
X-Nexus-Cost-Estimated: 0.0245
X-Nexus-Budget-Status: SoftLimit
X-Nexus-Budget-Utilization: 82.3
X-Nexus-Budget-Remaining: 17.75
```

**Only include budget headers when `budget_status != Normal`** (reduces header overhead for common case).

### Rationale
- **Constitution Principle III**: "Nexus-Transparent outputs: routing metadata in X-Nexus-* response headers (never in JSON body)"
- **Backward compatible**: Existing clients ignore unknown headers
- **Actionable**: Clients can detect SoftLimit and adjust behavior (e.g., queue non-urgent requests)

### Alternatives Considered
- **Include budget headers on all responses**: REJECTED - unnecessary overhead when budget is Normal
- **Add budget status to JSON body**: REJECTED - violates Constitution (OpenAI compatibility)
- **Use standard HTTP 429 for soft limit**: REJECTED - 429 implies rejection, but soft limit still serves requests

### Implementation Notes
Add header injection in `src/api/completions.rs` after routing decision, reading from `RoutingIntent.budget_status` and `RoutingIntent.cost_estimate`.

---

## R5: Persistent Budget State

### Context
FR-014 requires budget state to persist across service restarts. Need to determine persistence strategy that balances Constitution Principle VIII (Stateless by Design) with practical operational needs.

### Decision
**Phase 1 (P1/P2/P3)**: In-memory only (DashMap) - no persistence
- Rationale: Constitution explicitly says "All state is in-memory (no database)"
- Trade-off: Service restart resets budget counter (acceptable for initial release)
- Mitigation: Operators can query Prometheus for historical spending before restart

**Phase 2 (Future P4)**: Optional file-based persistence
- Store `BudgetMetrics` to JSON file on shutdown
- Load on startup if file exists
- Still no database dependency

### Rationale
- **Constitution compliance**: Principle VIII says "Operational state only: backend health, metrics, load — never user data"
- **Simplicity**: Avoids introducing database dependency for v1
- **Pragmatic**: Monthly budget resets mean losing a few hours of spending data is recoverable

### Alternatives Considered
- **SQLite for persistence**: REJECTED - adds dependency, complicates deployment
- **Redis/external state store**: REJECTED - violates Constitution Principle VII (no external dependencies)
- **Prometheus as source of truth**: REJECTED - Prometheus is for monitoring, not operational state

### Implementation Notes
For v1, document in operational guide: "Service restarts reset budget counter. Check Prometheus metrics before restart to record final spending."

Future enhancement: Add `--budget-state-file` CLI flag to enable optional persistence.

---

## R6: Billing Cycle Day Configuration

### Context
FR-007 mentions "configurable billing cycle day" but user context indicates "configurable billing cycle reset day". Need to clarify if this is day-of-month (1-31) or month-based rollover.

### Decision
**Phase 1**: Use month-based rollover only (YYYY-MM comparison)
- Default: First day of month (UTC) triggers reset
- Configuration: Not exposed in v1 (simplicity)

**Rationale**:
- Spec FR-007 says "configurable day-of-month" but no acceptance criteria test it
- User context emphasizes "configurable billing cycle" not "day"
- Cloud providers (OpenAI, Anthropic) use monthly billing aligned to account creation, not specific day
- Day-of-month adds timezone complexity and edge cases (Feb 29, months with 30 days)

### Alternatives Considered
- **Add `billing_cycle_day: u8` to BudgetConfig**: DEFERRED to future PR
  - Would require timezone handling (which timezone? user's? server's?)
  - Edge cases: day=31 in February, day=29 in non-leap years
  - Complexity not justified for single-tenant v1

### Implementation Notes
If multi-tenant billing is added (out of scope per spec), revisit this decision. For now, month rollover is sufficient for global budget tracking.

---

## Summary Table

| Topic | Decision | Key Trade-off | Constitution Principle |
|-------|----------|---------------|------------------------|
| **Tokenizers** | 3-tier strategy (exact/approx/heuristic) | Complexity vs accuracy | X (Precise Measurement) |
| **Billing Cycle** | Passive month rollover in reconciliation loop | Simplicity vs day-of-month control | I (Zero Configuration) |
| **Metrics** | Prometheus gauges + histograms + counters | Standard types vs custom aggregations | VII (Local-First, no external deps) |
| **Response Headers** | X-Nexus-Budget-* when status != Normal | Header overhead vs transparency | III (OpenAI-Compatible, Nexus-Transparent) |
| **Persistence** | In-memory only for v1 | Restart risk vs simplicity | VIII (Stateless by Design) |
| **Cycle Day Config** | Month-only for v1 (defer day-of-month) | Simplicity vs fine-grained control | I (Zero Configuration) |

---

## Open Questions Resolved

1. **Q**: Should token counting block the routing decision?  
   **A**: NO - Token counting happens in RequestAnalyzer phase (before pipeline), preserving <1ms routing budget.

2. **Q**: How to handle tokenizer failures (tiktoken panics, model not found)?  
   **A**: Graceful fallback to tier 3 heuristic with warning log. Never fail request due to tokenization error.

3. **Q**: Should budget metrics be per-model or global?  
   **A**: Global for v1 (single budget). Per-model breakdown available via Prometheus labels on `nexus_token_count_tier_total`.

4. **Q**: What happens if spending exceeds limit between reconciliation intervals?  
   **A**: Acceptable per spec Assumption: "slight overspend (< reconciliation_interval) is acceptable". Default 60s interval bounds overspend to ~$1-2 at typical request rates.

---

**Next Steps**: Proceed to Phase 1 (Design & Contracts) with these research findings as foundation.
