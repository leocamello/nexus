# Feature Specification: Inference Budget Management

**Feature Branch**: `016-inference-budget-mgmt`  
**Created**: 2025-01-24  
**Status**: Draft  
**Input**: User description: "Cost-aware routing with graceful degradation. Includes a tokenizer registry for audit-grade token counting across different providers."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Cost Control with Soft Limits (Priority: P1)

As a platform administrator, I need to set a monthly inference budget that gradually shifts traffic to cost-efficient options as spending approaches limits, so that I can control costs without disrupting service availability.

**Why this priority**: Core value proposition - enables cost control without service disruption. This is the foundation of the entire feature and delivers immediate business value.

**Independent Test**: Can be fully tested by configuring a monthly budget, sending inference requests until 80% threshold is reached, and observing routing shift from cloud-preferred to local-preferred behavior. Delivers immediate cost control value.

**Acceptance Scenarios**:

1. **Given** a monthly budget of $100 configured at 0% utilization, **When** inference requests accumulate $50 in costs, **Then** routing continues with normal cloud-overflow behavior and budget status shows "Normal"
2. **Given** budget utilization reaches the soft limit threshold (default 75%, configurable via soft_limit_percent), **When** new inference requests arrive, **Then** system shifts to local-preferred routing and emits warning metrics
3. **Given** budget utilization is at 85%, **When** user checks /v1/stats endpoint, **Then** budget status shows "SoftLimit" with current spending and remaining budget
4. **Given** soft limit is active, **When** local agents are unavailable, **Then** cloud agents are still used to maintain service availability

---

### User Story 2 - Precise Cost Tracking (Priority: P2)

As a platform administrator, I need accurate per-request cost estimates using provider-specific token counting, so that budget enforcement is audit-grade and I can trust spending projections.

**Why this priority**: Ensures accuracy of the budget system. Without accurate counting, the soft/hard limits trigger at wrong times, reducing trust in the feature.

**Independent Test**: Can be tested by sending identical requests to different providers (OpenAI, Anthropic, local) and verifying cost estimates use appropriate tokenizers. Delivers accurate billing visibility.

**Acceptance Scenarios**:

1. **Given** an OpenAI request with 1000 tokens, **When** cost is estimated, **Then** system uses tiktoken o200k_base encoder and records "exact" tier in metrics
2. **Given** an Anthropic Claude request, **When** cost is estimated, **Then** system uses cl100k_base approximation and records "exact" tier
3. **Given** a request to an unknown model, **When** cost is estimated, **Then** system applies 1.15x conservative multiplier and flags as "estimated" in metrics
4. **Given** multiple requests processed, **When** viewing Prometheus metrics, **Then** nexus_token_count_tier counter shows breakdown of exact vs estimated counts

---

### User Story 3 - Hard Limit Protection (Priority: P3)

As a platform administrator, I need configurable actions when monthly budget is exhausted, so that I can choose between local-only routing, queueing, or rejection based on my availability requirements.

**Why this priority**: Provides final safety net for budget control. Less critical than P1/P2 since soft limits already provide significant protection, but important for strict budget enforcement.

**Independent Test**: Can be tested by exhausting monthly budget (100% utilization) and verifying configured hard_limit_action is enforced. Delivers budget ceiling enforcement.

**Acceptance Scenarios**:

1. **Given** budget at 100% with hard_limit_action="block_cloud", **When** inference request arrives with only cloud options available, **Then** request is rejected with budget exceeded error including actionable context
2. **Given** budget at 100% with hard_limit_action="block_all", **When** inference request arrives, **Then** request is rejected with 503 status and budget context in error response
3. **Given** budget at 100% with hard_limit_action="warn", **When** inference request arrives, **Then** request is routed normally but warning is logged and metrics updated
4. **Given** hard limit is active, **When** billing cycle resets (first day of month, UTC), **Then** budget counter resets to 0 and routing returns to normal

---

### User Story 4 - Budget Visibility and Monitoring (Priority: P4)

As a platform operator, I need real-time budget status visibility through metrics and response headers, so that I can monitor spending trends and take proactive action before limits impact service.

**Why this priority**: Operational visibility is important but doesn't affect core budget enforcement. Can be added after budget control is proven working.

**Independent Test**: Can be tested by generating various load patterns and verifying metrics, dashboard, and response headers accurately reflect budget state. Delivers operational transparency.

**Acceptance Scenarios**:

1. **Given** active inference requests, **When** Prometheus is scraped, **Then** metrics include nexus_budget_spending_usd, nexus_budget_utilization_percent, and nexus_budget_status gauges
2. **Given** budget utilization above 80%, **When** inference response is returned, **Then** X-Nexus-Budget-Status header indicates "SoftLimit" or "HardLimit"
3. **Given** multiple requests at various costs, **When** viewing Prometheus, **Then** nexus_cost_per_request_usd histogram shows distribution of request costs
4. **Given** budget status changes from Normal to SoftLimit, **When** background reconciliation runs, **Then** metrics update within 60 seconds and dashboard reflects new status

---

### Edge Cases

- What happens when budget is exhausted mid-request? System completes in-flight requests but rejects new ones based on hard_limit_action.
- How does system handle clock skew across distributed nodes? Budget reconciliation uses centralized state with eventual consistency; brief over-spending (< reconciliation_interval) is acceptable.
- What happens when pricing data is unavailable for a provider? Falls back to 1.15x conservative multiplier with "estimated" flag in metrics.
- How does system handle budget reset during active requests? In-flight requests count against old billing cycle; new requests start fresh counter.
- What happens when local agents fail during hard limit local-only mode? Requests fail with 503 Service Unavailable - budget enforcement takes precedence over availability in this mode.
- How does system handle concurrent requests racing to exhaust budget? Last-write-wins with atomic counter updates; slight over-spending possible but bounded by reconciliation interval.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST estimate per-request cost using provider-specific tokenizers before routing decisions
- **FR-002**: System MUST track cumulative monthly spending against configured budget limit
- **FR-003**: System MUST transition routing behavior at soft limit threshold (configurable via soft_limit_percent, default 75% utilization)
- **FR-004**: System MUST enforce hard limit action when budget is exhausted (100% utilization)
- **FR-005**: System MUST use exact tokenizers for OpenAI (o200k_base/cl100k_base via tiktoken-rs) and Anthropic (cl100k_base approximation)
- **FR-006**: System MUST apply 1.15x conservative multiplier for unknown models and flag as "estimated" in metrics
- **FR-007**: System MUST reset budget counter on first day of each month (UTC). Day-of-month configuration deferred to v2.
- **FR-008**: System MUST expose budget status through /v1/stats endpoint including current spending, limit, utilization percentage, and status
- **FR-009**: System MUST run background reconciliation loop at configurable interval (default 60 seconds) to sync spending state
- **FR-010**: System MUST support three hard limit actions: "warn" (log warning, allow all requests), "block_cloud" (exclude cloud agents, keep local), "block_all" (reject all requests with 503)
- **FR-011**: System MUST include X-Nexus-Budget-Status response header when utilization exceeds soft limit
- **FR-012**: System MUST record token count tier (exact vs estimated) in metrics for audit trail
- **FR-013**: System MUST complete in-flight requests even when budget is exhausted mid-execution
- **FR-014**: System SHOULD persist spending state to survive service restarts (DEFERRED to v2 — v1 uses in-memory DashMap with acceptable data loss on restart)

### Key Entities

- **Budget State**: Current month spending total (USD), configured monthly limit (USD), billing cycle reset day, last reconciliation timestamp, current status (Normal/SoftLimit/HardLimit)
- **Cost Estimate**: Per-request estimate including input token count, estimated output tokens, total cost in USD, tokenizer tier (exact/estimated), provider name
- **Tokenizer Registry**: Mapping of model identifiers to tokenizer implementations (tiktoken o200k_base, cl100k_base, SentencePiece for Llama, fallback heuristic)
- **Pricing Table**: Per-provider, per-model pricing for input tokens and output tokens in USD (refreshed from external source or hardcoded with update cadence)
- **Budget Event**: Audit log entry recording timestamp, event type (soft_limit_reached, hard_limit_reached, budget_reset, reconciliation), spending amount, utilization percentage

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: System accurately estimates request costs within 5% variance for providers with exact tokenizers (OpenAI, Anthropic)
- **SC-002**: Soft limit routing shift reduces cloud spending by at least 40% in the 80-100% utilization range
- **SC-003**: Budget status transitions (Normal → SoftLimit → HardLimit) occur within one reconciliation interval (60 seconds) of threshold crossing
- **SC-004**: Budget metrics (spending, utilization, status) are exposed via Prometheus and queryable with <1% sampling error
- **SC-005**: Zero in-flight requests are terminated when hard limit is reached (graceful degradation only)
- **SC-006**: Budget counter resets successfully on billing cycle day without manual intervention
- **SC-007**: System maintains sub-200ms latency overhead for cost estimation and budget checks on P95
- **SC-008**: Unknown model cost estimates use conservative multiplier resulting in budget exhaustion prediction that is early (safe) rather than late (overspend)
- **SC-009**: Budget state persists across service restarts with zero spending data loss (DEFERRED to v2 — v1 accepts data loss on restart)
- **SC-010**: 100% of requests include accurate cost metadata in response headers when budget status is not Normal

## Scope

### In Scope

- Enhancing existing BudgetReconciler with complete tokenizer registry support
- Adding Prometheus metrics for budget monitoring (spending, utilization, status, cost distribution)
- Implementing per-request cost estimation with provider-specific tokenizers
- Configurable hard limit actions (local-only, queue, reject)
- Budget status visibility in /v1/stats endpoint and response headers
- Background reconciliation loop with configurable interval
- Monthly billing cycle reset (fixed to first day of month, UTC; day-of-month configuration deferred to v2)
- In-memory budget state (persistence deferred to v2)
- Audit-grade token counting with tier tracking (exact vs estimated)

### Out of Scope

- Real-time dynamic pricing updates from provider APIs (use hardcoded pricing table with manual updates)
- Multi-tenant budget isolation (single global budget for initial release)
- Budget alerting via external channels (email, Slack, PagerDuty) - metrics-only
- Historical spending analytics and trend analysis (rely on Prometheus/Grafana for this)
- Budget forecasting and predictive alerts (future enhancement)
- Per-user or per-team budget allocation (future enhancement)
- Cost optimization recommendations based on usage patterns (future enhancement)
- Integration with cloud billing APIs for reconciliation (manual price updates only)

## Assumptions

- **Pricing Stability**: Provider pricing changes infrequently enough that hardcoded pricing table with manual updates is acceptable for initial release
- **Single Billing Cycle**: All usage follows a single monthly billing cycle reset day (not per-user or per-tenant cycles)
- **Tokenizer Availability**: Required tokenizer libraries (tiktoken-rs, tokenizers crate) are available and maintained for target providers
- **Reconciliation Accuracy**: 60-second reconciliation interval provides sufficient accuracy for budget enforcement (slight overspend acceptable)
- **Centralized State**: Budget state is managed in a single centralized store (not distributed consensus) with acceptable eventual consistency
- **Request Completion**: Average request duration is under 60 seconds, so in-flight overspend during hard limit is bounded
- **Conservative Estimation**: 1.15x multiplier for unknown models provides sufficient safety margin to avoid significant overspend
- **Operator Monitoring**: Platform operators actively monitor Prometheus metrics and respond to budget alerts (no automated external alerting)

## Dependencies

- **Existing Infrastructure**: BudgetReconciler, BudgetConfig, BudgetStatus enum, CostEstimate struct, PricingTable, BudgetReconciliationLoop (all already implemented in Control Plane PR)
- **External Crates**: tiktoken-rs (OpenAI/Anthropic tokenizers), tokenizers crate (Llama SentencePiece, behind feature flag)
- **Metrics System**: Prometheus metrics exporter must be available to expose budget gauges, histograms, and counters
- **Persistent Storage**: Deferred to v2. v1 uses in-memory DashMap (spending resets on restart).
- **Routing System**: Integration with RoutingIntent and pipeline (RequestAnalyzer → PrivacyReconciler → BudgetReconciler → TierReconciler → Scheduler)
- **Configuration**: TOML configuration system for budget settings (monthly_limit, soft_limit_percent, hard_limit_action, reconciliation_interval_secs, billing_cycle_day)

## Open Questions

None - all requirements are fully specified based on existing architecture and constitution alignment.

## Related Work

- **RFC-001**: Defines BudgetReconciler position in Control Plane pipeline and tiered token counting approach
- **Control Plane PR**: Implements foundation including BudgetReconciler (823 lines, 18 tests), BudgetConfig, BudgetStatus, CostEstimate, reconciliation loop
- **Constitution Principle X (Precise Measurement)**: Drives requirement for per-backend tokenizers instead of generic estimates
- **Constitution Principle IX (Explicit Contracts)**: Drives graceful degradation approach (never hard-cut production)
- **Constitution Principle V (Intelligent Routing)**: Establishes cost as a routing factor alongside privacy and capability tiers
