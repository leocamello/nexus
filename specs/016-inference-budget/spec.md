# Feature Specification: Inference Budget Management

**Feature Branch**: `016-inference-budget`  
**Created**: 2025-01-22  
**Status**: Draft  
**Input**: User description: "Cost-aware routing with graceful degradation. Includes a tokenizer registry for audit-grade token counting across different providers."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Basic Budget Tracking and Cost Visibility (Priority: P1)

As a system operator, I need to track inference costs in real-time so that I can monitor spending against allocated budgets and receive alerts before exceeding limits.

**Why this priority**: Core functionality that enables cost awareness. Without accurate cost tracking, all other budget features are meaningless. This delivers immediate value by providing spending visibility.

**Independent Test**: Can be fully tested by sending inference requests to different providers (OpenAI, Anthropic, Llama) and verifying that costs are accurately calculated and tracked in Prometheus metrics. Delivers value by showing real-time spending dashboards.

**Acceptance Scenarios**:

1. **Given** a configured monthly budget of $100.00, **When** an inference request is routed to OpenAI with 500 input tokens, **Then** the system calculates the cost using tiktoken-rs (o200k_base/cl100k_base tokenizer), records the cost estimate in metrics, and updates the current spending total.

2. **Given** ongoing inference requests, **When** I query Prometheus metrics, **Then** I see accurate per-request cost estimates broken down by provider, model, token counts (input/output), and cumulative monthly spending.

3. **Given** a request to an unknown model without a registered tokenizer, **When** the system estimates cost, **Then** it applies a 1.15x conservative multiplier to the token count and flags the estimate as "estimated" (not "exact") in metrics.

---

### User Story 2 - Graceful Degradation at Soft Limit (Priority: P2)

As a system operator, I need the system to automatically prefer cost-efficient local agents when approaching budget limits so that I can continue serving requests while minimizing cloud costs.

**Why this priority**: Implements intelligent cost-aware routing without disrupting service. This is the primary value proposition of graceful degradation - maintaining availability while controlling costs.

**Independent Test**: Can be fully tested by setting a low monthly budget, consuming 80% of it, then sending new requests and verifying that local agents are strongly preferred over cloud agents. Delivers value by extending budget runway without service interruption.

**Acceptance Scenarios**:

1. **Given** current spending at 0-79% of monthly budget, **When** a new inference request arrives, **Then** the system routes using normal local-first logic (local preferred, cloud for overflow).

2. **Given** current spending reaches 80% of monthly budget (soft limit), **When** a new inference request arrives, **Then** the BudgetReconciler sets BudgetStatus to "SoftLimit" on the RoutingIntent, the system strongly prefers local agents, and emits a warning log stating "Budget soft limit reached: preferring local agents".

3. **Given** current spending between 80-99% of monthly budget, **When** a request must use cloud due to no available local agents, **Then** the system still allows cloud routing but logs a warning about the budget status.

---

### User Story 3 - Hard Limit Enforcement with Configurable Actions (Priority: P2)

As a system operator, I need the system to enforce hard budget limits with configurable behavior so that I can prevent runaway costs while choosing the appropriate fallback strategy for my use case.

**Why this priority**: Provides the safety net for budget enforcement. This prevents unexpected cost overruns while giving operators control over the tradeoff between cost and availability.

**Independent Test**: Can be fully tested by exhausting a monthly budget (100% spending), then sending new requests and verifying that the configured hard_limit_action is applied (local-only, queue, or reject). Delivers value by guaranteeing cost control.

**Acceptance Scenarios**:

1. **Given** current spending reaches 100% of monthly budget and hard_limit_action is "local-only", **When** a new inference request arrives, **Then** the BudgetReconciler sets BudgetStatus to "HardLimit", the system routes ONLY to local agents, and excludes all cloud agents from routing consideration.

2. **Given** current spending at 100% of monthly budget and hard_limit_action is "queue", **When** a new request requires cloud agents, **Then** the system queues the request and logs "Budget hard limit reached: request queued pending budget reset".

3. **Given** current spending at 100% of monthly budget and hard_limit_action is "reject", **When** a new request requires cloud agents, **Then** the system returns an error response stating "Budget limit exceeded, request rejected" and logs the rejection.

4. **Given** a hard limit has been reached, **When** the monthly billing cycle resets (first day of new month), **Then** the BudgetReconciliationLoop resets the spending counter to $0.00, sets BudgetStatus back to "Normal", and logs "Monthly budget reset: $100.00 available".

---

### User Story 4 - Per-Provider Tokenizer Accuracy (Priority: P3)

As a system operator, I need audit-grade token counting for each provider so that cost estimates are accurate and I can trust budget enforcement decisions.

**Why this priority**: Ensures accuracy of the entire budget system. While lower priority than core routing, this is essential for production reliability and financial accuracy. Inaccurate tokenization undermines trust in the system.

**Independent Test**: Can be fully tested by sending identical prompts to different providers (OpenAI with o200k_base, Anthropic with cl100k_base approximation, Llama with SentencePiece) and verifying that token counts match provider-reported values within acceptable margins. Delivers value by ensuring financial accuracy.

**Acceptance Scenarios**:

1. **Given** a request to an OpenAI GPT-4 model, **When** the system counts tokens, **Then** it uses the o200k_base tokenizer via tiktoken-rs and reports the token count tier as "exact".

2. **Given** a request to an Anthropic Claude model, **When** the system counts tokens, **Then** it uses the cl100k_base approximation via tiktoken-rs and reports the token count tier as "exact" (or "approximation" if documented as such).

3. **Given** a request to a Llama model, **When** the system counts tokens, **Then** it uses the SentencePiece tokenizer via the tokenizers crate and reports the token count tier as "exact".

4. **Given** a request to an unknown model not in the tokenizer registry, **When** the system counts tokens, **Then** it uses a heuristic default (character count / 4 or similar) with 1.15x conservative multiplier and reports the token count tier as "estimated".

5. **Given** any request with token counting, **When** metrics are recorded, **Then** the CostEstimate struct includes: input_tokens (int), estimated_output_tokens (int), cost_usd (float), and token_count_tier (enum: "exact", "approximation", "estimated").

---

### Edge Cases

- **What happens when monthly budget is set to $0.00?** System should treat this as hard limit immediately (100% of $0 is $0), applying hard_limit_action to all requests.

- **What happens when a request is in-flight and budget crosses from Normal to SoftLimit or HardLimit?** In-flight requests continue processing (pre-authorized), but the updated BudgetStatus applies to new requests queued after the status change.

- **What happens when provider-reported costs differ from estimated costs?** The BudgetReconciliationLoop (running every 60s) should reconcile actual costs from provider APIs when available and adjust the spending total accordingly, logging discrepancies.

- **What happens when the tokenizer registry is updated mid-month?** New token counts apply to future requests only. Historical cost estimates are not retroactively adjusted unless explicitly triggered by an operator via a reconciliation command.

- **What happens when the system clock is wrong or billing cycle date is misconfigured?** Budget reset logic should be based on a configurable billing cycle start date (default: 1st of month UTC). If misconfigured, budget reset will be delayed/early but will not cause data corruption.

- **What happens when concurrent requests race to consume the last dollar of budget?** Multiple requests may be authorized before BudgetReconciliationLoop updates the spending total, potentially exceeding budget by up to [number of concurrent requests] × [average request cost]. This is acceptable overage documented in system limits.

- **What happens when estimated output tokens significantly differ from actual output tokens?** The cost estimate is based on typical output token counts for similar requests. Actual costs are reconciled by BudgetReconciliationLoop when provider invoices are available. Significant deviations should trigger a warning for manual review.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST count tokens for each inference request using provider-specific tokenizers: OpenAI models use o200k_base or cl100k_base via tiktoken-rs, Anthropic models use cl100k_base approximation via tiktoken-rs, Llama models use SentencePiece via tokenizers crate.

- **FR-002**: System MUST apply a 1.15x conservative multiplier to token counts for unknown models not in the tokenizer registry, and flag such estimates with token_count_tier = "estimated" in metrics.

- **FR-003**: System MUST calculate per-request cost estimates including: input_tokens, estimated_output_tokens, cost_usd, and token_count_tier (exact/approximation/estimated).

- **FR-004**: System MUST implement a BudgetReconciler component in the Control Plane that evaluates current spending against configured monthly_limit before each routing decision.

- **FR-005**: System MUST set BudgetStatus (Normal/SoftLimit/HardLimit) on the RoutingIntent based on current spending percentage: Normal (0-79%), SoftLimit (80-99%), HardLimit (100%+).

- **FR-006**: System MUST adjust routing behavior based on BudgetStatus: Normal routing uses local-first with cloud overflow, SoftLimit strongly prefers local agents, HardLimit applies configured hard_limit_action.

- **FR-007**: System MUST support three hard_limit_action options: "local-only" (exclude cloud agents), "queue" (queue requests requiring cloud), "reject" (return error for requests requiring cloud).

- **FR-008**: System MUST implement a BudgetReconciliationLoop background task that updates current spending totals every 60 seconds.

- **FR-009**: System MUST reset monthly spending to $0.00 at the start of each billing cycle based on a configurable billing cycle start date (default: 1st of month UTC).

- **FR-010**: System MUST expose budget metrics via Prometheus including: current spending (gauge), budget limit (gauge), budget percentage used (gauge), requests blocked by budget (counter), soft limit activations (counter), hard limit activations (counter).

- **FR-011**: System MUST emit warning logs when soft limit (80%) is reached: "Budget soft limit reached: preferring local agents".

- **FR-012**: System MUST emit error logs when hard limit (100%) is reached with the action taken: "Budget hard limit reached: [action description]".

- **FR-013**: System MUST load budget configuration from nexus.toml including: monthly_limit (float, USD), soft_limit_percent (integer, default 80), hard_limit_action (enum: local-only/queue/reject).

- **FR-014**: System MUST validate budget configuration at startup: monthly_limit must be >= 0.00, soft_limit_percent must be 0-100, hard_limit_action must be a valid enum value.

- **FR-015**: System MUST call agent.count_tokens() from the NII trait for each inference request before routing.

- **FR-016**: System MUST support tiered token counting: exact for registered tokenizers, heuristic default for unknown models.

- **FR-017**: System MUST track cost estimates per provider and model in metrics to enable granular cost analysis.

### Key Entities *(include if feature involves data)*

- **BudgetStatus**: Represents the current state of budget consumption (Normal, SoftLimit, HardLimit). Attached to RoutingIntent to inform routing decisions.

- **CostEstimate**: Represents the estimated cost of an inference request. Contains: input_tokens (integer), estimated_output_tokens (integer), cost_usd (float), token_count_tier (enum: exact, approximation, estimated), provider (string), model (string), timestamp.

- **BudgetConfig**: Represents the loaded budget configuration. Contains: monthly_limit (float, USD), soft_limit_percent (integer), hard_limit_action (enum), billing_cycle_start_day (integer, 1-31).

- **TokenizerRegistry**: Represents the mapping of model identifiers to tokenizer implementations. Contains: model_pattern (regex or string match), tokenizer_type (enum: tiktoken_o200k, tiktoken_cl100k, sentencepiece), tokenizer_crate (tiktoken-rs or tokenizers), token_count_tier (exact/approximation).

- **BudgetMetrics**: Represents Prometheus metrics for budget tracking. Contains: current_spending_usd (gauge), monthly_limit_usd (gauge), budget_percent_used (gauge), requests_blocked_by_budget_total (counter), soft_limit_activations_total (counter), hard_limit_activations_total (counter), cost_by_provider_model (histogram).

- **RoutingIntent**: Extended to include BudgetStatus field. Routing algorithms read this status to adjust agent selection based on budget constraints.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Operators can view real-time budget utilization in Grafana dashboards showing current spending, budget limit, percentage used, and spending trend over the billing cycle.

- **SC-002**: Token counts for OpenAI models match OpenAI's reported token counts within 1% accuracy (measured by comparing system estimates with actual provider-reported tokens from API responses).

- **SC-003**: When soft limit (80%) is reached, at least 90% of new inference requests are routed to local agents (measured by comparing local vs cloud routing decisions during soft limit periods).

- **SC-004**: When hard limit (100%) is reached with hard_limit_action="local-only", zero requests are routed to cloud agents (measured by counting cloud routing decisions during hard limit periods).

- **SC-005**: Budget resets occur automatically on the first day of each month (or configured billing cycle start date) without manual intervention, with downtime or service disruption under 1 second.

- **SC-006**: Unknown model token counts are flagged as "estimated" in 100% of cases where the model is not in the tokenizer registry (measured by checking token_count_tier field in metrics).

- **SC-007**: Budget reconciliation updates spending totals at least once per minute (60-second intervals), with maximum staleness of 65 seconds (measured by timestamp differences in budget metrics).

- **SC-008**: Cost estimates include all required fields (input_tokens, estimated_output_tokens, cost_usd, token_count_tier) for 100% of inference requests (measured by metric completeness checks).

- **SC-009**: System prevents runaway costs by blocking or queueing requests when hard limit is reached, with zero unauthorized cloud requests during hard limit enforcement periods.

- **SC-010**: Operators can identify spending anomalies within 2 minutes of occurrence using Prometheus alerts based on budget metrics (e.g., rapid spending increase, unexpected soft/hard limit activation).
