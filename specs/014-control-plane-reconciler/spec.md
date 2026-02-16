# Feature Specification: Control Plane — Reconciler Pipeline (RFC-001 Phase 2)

**Feature Branch**: `014-control-plane-reconciler`  
**Created**: 2025-01-09  
**Status**: Draft  
**Input**: User description: "Replace the imperative Router::select_backend() god-function with a pipeline of independent Reconcilers that annotate shared routing state. This enables Privacy Zones (F13) and Budget Management (F14) without O(n²) feature interaction."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Basic Pipeline Execution (Priority: P1)

As a Nexus operator, I need the routing system to make decisions through a clean pipeline of independent reconcilers instead of a monolithic function, so that I can understand, test, and debug routing behavior more easily.

**Why this priority**: This is the foundational infrastructure that enables all other reconcilers. Without this, no other privacy, budget, or quality features can be implemented in a maintainable way.

**Independent Test**: Can be fully tested by routing a single request through the pipeline with minimal reconcilers (RequestAnalyzer + SchedulerReconciler) and verifying the routing decision matches the existing Router::select_backend() behavior. Delivers immediate value by making the codebase more maintainable.

**Acceptance Scenarios**:

1. **Given** a configured agent pool with 3 healthy agents, **When** a request arrives for "gpt-4", **Then** the pipeline produces a Route decision with agent_id, model, reason, and cost_estimate
2. **Given** all agents are at capacity, **When** a request arrives, **Then** the pipeline produces a Queue decision with estimated_wait_ms and fallback_agent
3. **Given** no agents support the requested model, **When** a request arrives, **Then** the pipeline produces a Reject decision with detailed rejection_reasons from each reconciler

---

### User Story 2 - Privacy Zone Enforcement (Priority: P2)

As a compliance officer, I need to ensure that requests with privacy constraints never route to cloud agents, so that sensitive data remains within our controlled infrastructure.

**Why this priority**: Critical for regulatory compliance (GDPR, HIPAA) but can be implemented after basic pipeline exists. Represents a key value proposition for enterprise customers.

**Independent Test**: Can be tested by configuring a TrafficPolicy with privacy="restricted" for a model pattern, sending requests for that model, and verifying cloud agents are excluded from candidate set with appropriate RejectionReason entries.

**Acceptance Scenarios**:

1. **Given** a TrafficPolicy with model_pattern="gpt-4-*" and privacy="restricted", **When** a request arrives for "gpt-4-turbo", **Then** all cloud agents (privacy_zone=cloud) are excluded with RejectionReason explaining privacy constraint
2. **Given** a TrafficPolicy with privacy="restricted" and only cloud agents available, **When** a request arrives, **Then** the pipeline produces a Reject decision with clear explanation that no privacy-compliant agents exist
3. **Given** a TrafficPolicy with privacy="unrestricted", **When** a request arrives, **Then** cloud agents remain in candidate pool

---

### User Story 3 - Budget Management (Priority: P2)

As a cost controller, I need the system to enforce spending limits and prefer cost-effective agents when approaching limits, so that I don't exceed our monthly AI model budget.

**Why this priority**: Prevents unexpected cloud bills and enables predictable cost management. Can be implemented after basic pipeline exists and delivers immediate ROI by preventing cost overruns.

**Independent Test**: Can be tested by configuring a budget limit, simulating requests that approach the soft/hard limits, and verifying that BudgetStatus changes trigger appropriate agent filtering behavior (prefer local at soft limit, block cloud at hard limit).

**Acceptance Scenarios**:

1. **Given** monthly spending is below soft limit (75%), **When** a request arrives, **Then** BudgetStatus is Normal and all agents remain candidates
2. **Given** monthly spending exceeds soft limit, **When** a request arrives, **Then** BudgetStatus is SoftLimit and local agents receive higher priority scores
3. **Given** monthly spending exceeds hard limit, **When** a request arrives, **Then** BudgetStatus is HardLimit and cloud agents are excluded with RejectionReason explaining budget constraint
4. **Given** hard limit reached with no local agents available, **When** a request arrives, **Then** pipeline produces Reject decision with actionable guidance on increasing budget

---

### User Story 4 - Capability Tier Enforcement (Priority: P3)

As an API user, I need explicit control over quality-cost tradeoffs so that I can prevent silent downgrades to lower-tier models when my application requires specific capabilities.

**Why this priority**: Improves user experience by making quality expectations explicit, but can be implemented after privacy and budget features. Requires X-Nexus-Strict/X-Nexus-Flexible headers to be useful.

**Independent Test**: Can be tested by setting a TrafficPolicy with min_tier=3 for a model pattern, sending requests with X-Nexus-Strict header, and verifying agents below tier 3 are excluded with appropriate RejectionReason.

**Acceptance Scenarios**:

1. **Given** a TrafficPolicy with min_tier=3 for "gpt-4", **When** a request arrives with X-Nexus-Strict header, **Then** agents with capability_tier < 3 are excluded
2. **Given** X-Nexus-Flexible header, **When** no tier-3 agents available, **Then** pipeline falls back to tier-2 agents with warning in response
3. **Given** min_tier constraint with no qualifying agents, **When** request arrives, **Then** Reject decision includes RejectionReason for each agent explaining tier mismatch

---

### User Story 5 - Actionable Error Responses (Priority: P3)

As an API consumer, I need detailed rejection reasons when routing fails, so that I can take corrective action (adjust budget, change privacy settings, retry later) instead of guessing why my request failed.

**Why this priority**: Significantly improves developer experience but depends on all reconcilers being implemented. Can be added incrementally as reconcilers gain rejection tracking.

**Independent Test**: Can be tested by triggering various rejection scenarios (privacy constraint, budget exceeded, no capable agents) and verifying the 503 response contains structured rejection_reasons with agent_id, reconciler name, reason, and suggested_action.

**Acceptance Scenarios**:

1. **Given** privacy constraint excludes all agents, **When** request fails, **Then** 503 response includes rejection_reasons showing which agents were excluded and why
2. **Given** budget hard limit reached, **When** request fails, **Then** 503 response suggests increasing budget or retrying with privacy="restricted"
3. **Given** mixed rejection reasons (some privacy, some budget, some tier), **When** request fails, **Then** 503 response groups reasons by type and suggests most actionable fix

---

### Edge Cases

- What happens when a reconciler mutates RoutingIntent in a way that contradicts a previous reconciler's decision? (Reconcilers must be order-independent; only add constraints, never remove)
- How does system handle a reconciler that takes > 1ms to execute? (Pipeline should timeout individual reconcilers and log performance warnings)
- What happens when BudgetReconciliationLoop aggregates spending while a request is in-flight? (Budget checks use snapshot at request start; reconciliation is eventually consistent)
- How does system behave when TrafficPolicy glob patterns overlap? (Most specific pattern wins; document precedence rules in config)
- What happens if alias resolution creates a 3-level chain A→B→C→D? (RequestAnalyzer enforces max 3-level chaining, rejects deeper chains)
- How does PrivacyReconciler handle agents with unknown privacy_zone? (Treats unknown as "cloud" for safety; requires explicit "local" or "private" annotation)

## Requirements *(mandatory)*

### Functional Requirements

#### Core Pipeline Infrastructure

- **FR-001**: System MUST define a Reconciler trait with methods: name() returning reconciler identifier, and reconcile() accepting mutable RoutingIntent and returning Result
- **FR-002**: System MUST define RoutingIntent struct containing: request_id, requested_model, resolved_model, requirements (RequestRequirements), privacy_constraint (Option<PrivacyZone>), budget_status (BudgetStatus enum), min_capability_tier (Option<u8>), cost_estimate (CostEstimate), candidate_agents (Vec<AgentId>), excluded_agents (Vec<AgentId>), rejection_reasons (Vec<RejectionReason>)
- **FR-003**: System MUST define RoutingDecision enum with variants: Route { agent_id, model, reason, cost_estimate }, Queue { reason, estimated_wait_ms, fallback_agent }, Reject { rejection_reasons }
- **FR-004**: System MUST define RejectionReason struct containing: agent_id, reconciler name, reason description, suggested_action
- **FR-005**: System MUST execute reconcilers in fixed order: RequestAnalyzer → PrivacyReconciler → BudgetReconciler → TierReconciler → QualityReconciler → SchedulerReconciler
- **FR-006**: System MUST ensure Router::select_backend() method signature remains unchanged to maintain backward compatibility with existing tests

#### RequestAnalyzer (Phase 2 Foundation)

- **FR-007**: RequestAnalyzer MUST resolve model aliases with maximum 3-level chaining (A→B→C allowed, A→B→C→D rejected)
- **FR-008**: RequestAnalyzer MUST extract RequestRequirements from request containing: vision capability, tools capability, JSON mode, estimated token count
- **FR-009**: RequestAnalyzer MUST complete alias resolution and requirement extraction in under 0.5ms per request
- **FR-010**: RequestAnalyzer MUST populate initial RoutingIntent with requested_model, resolved_model, requirements, and full candidate_agents list from agent pool

#### PrivacyReconciler (F13)

- **FR-011**: PrivacyReconciler MUST load TrafficPolicy rules from [routing.policies.*] TOML sections at startup, matching requests by model pattern using glob syntax
- **FR-012**: PrivacyReconciler MUST read privacy_zone from each AgentSchedulingProfile via agent.profile() method
- **FR-013**: PrivacyReconciler MUST exclude agents where privacy_zone="cloud" when TrafficPolicy specifies privacy="restricted"
- **FR-014**: PrivacyReconciler MUST log RejectionReason for each excluded agent containing: agent_id, "PrivacyReconciler" as reconciler name, reason explaining privacy constraint, suggested_action recommending local agents or relaxing constraint
- **FR-015**: PrivacyReconciler MUST treat agents with unknown/missing privacy_zone as "cloud" for safety (require explicit "local" or "private" annotation)

#### BudgetReconciler (F14)

- **FR-016**: BudgetReconciler MUST load budget configuration containing: monthly_limit (USD), soft_limit_percent (default 75%), hard_limit_action (enum: warn, block_cloud, block_all)
- **FR-017**: BudgetReconciler MUST call agent.count_tokens() to estimate request cost in USD based on: input tokens, estimated output tokens, model pricing
- **FR-018**: BudgetReconciler MUST populate CostEstimate in RoutingIntent containing: input_tokens, estimated_output_tokens, cost_usd, token_count_tier
- **FR-019**: BudgetReconciler MUST set BudgetStatus to Normal when spending < soft_limit, SoftLimit when soft_limit ≤ spending < hard_limit, HardLimit when spending ≥ hard_limit
- **FR-020**: BudgetReconciler MUST prefer local agents (increase priority score) when BudgetStatus is SoftLimit
- **FR-021**: BudgetReconciler MUST exclude cloud agents when BudgetStatus is HardLimit and hard_limit_action is block_cloud or block_all
- **FR-022**: BudgetReconciler MUST run a background BudgetReconciliationLoop that aggregates spending every 60 seconds from agent telemetry
- **FR-023**: BudgetReconciler MUST use snapshot of budget state at request start time (eventual consistency with reconciliation loop)

#### TierReconciler (F13)

- **FR-024**: TierReconciler MUST read min_capability_tier from TrafficPolicy matched by model pattern glob
- **FR-025**: TierReconciler MUST read capability_tier from each AgentSchedulingProfile
- **FR-026**: TierReconciler MUST exclude agents where capability_tier < min_capability_tier
- **FR-027**: TierReconciler MUST respect X-Nexus-Strict header to enforce tier requirements strictly (reject if tier not met)
- **FR-028**: TierReconciler MUST respect X-Nexus-Flexible header to allow fallback to lower tiers with warning when X-Nexus-Strict not present

#### SchedulerReconciler

- **FR-029**: SchedulerReconciler MUST score remaining candidate agents using formula: priority × (1 - load_factor) × (1 / latency_ema_ms) × quality_score
- **FR-030**: SchedulerReconciler MUST incorporate quality_score based on: error_rate_1h (default 0.0), avg_ttft_ms (default 0), success_rate_24h (default 1.0)
- **FR-031**: SchedulerReconciler MUST return Queue decision when highest-scoring agent has HealthStatus::Loading, including estimated_wait_ms and fallback_agent
- **FR-032**: SchedulerReconciler MUST return Reject decision when no candidate agents remain, aggregating rejection_reasons from all reconcilers
- **FR-033**: SchedulerReconciler MUST return Route decision with selected agent_id, resolved model, routing reason, and cost_estimate when candidates exist

#### Integration & Configuration

- **FR-034**: System MUST load TrafficPolicy definitions from optional [routing.policies.*] TOML sections, defaulting to zero constraints when sections absent
- **FR-035**: TrafficPolicy MUST support fields: model_pattern (glob), privacy (enum: unrestricted, restricted), max_cost_per_request (USD), min_tier (u8), fallback_allowed (bool)
- **FR-036**: System MUST make pipeline total overhead (all reconcilers combined) remain under 1ms for requests with warm cache
- **FR-037**: Router::select_backend() MUST delegate to pipeline while maintaining existing method signature and return types
- **FR-038**: System MUST ensure all existing Router tests pass without modification after pipeline integration

### Key Entities

- **Reconciler**: A trait representing an independent stage in the routing pipeline that reads and annotates RoutingIntent. Each reconciler implements: name() for identification, reconcile(&mut RoutingIntent) for logic execution
- **RoutingIntent**: Shared state object passed through pipeline containing request metadata, extracted requirements, constraints from policies, budget status, agent candidates, excluded agents, and rejection reasons. Reconcilers only add constraints; never remove
- **RoutingDecision**: Final output of pipeline representing one of three outcomes: Route (successful routing to agent), Queue (agent busy, wait required), Reject (no viable agents, detailed reasons provided)
- **RejectionReason**: Detailed explanation for why an agent was excluded, containing: agent identifier, reconciler that excluded it, human-readable reason, suggested corrective action
- **AgentSchedulingProfile**: Metadata about an agent required for routing decisions, containing: agent_id, privacy_zone, capability_tier, current_load, latency_ema_ms, available_models, resource_usage, budget_remaining, error_rate_1h, avg_ttft_ms, success_rate_24h
- **TrafficPolicy**: Configuration rule matching requests by model pattern (glob) and specifying constraints: privacy requirements, cost limits, minimum capability tier, fallback behavior
- **BudgetStatus**: Enumeration representing current budget state: Normal (below soft limit), SoftLimit (prefer local agents), HardLimit (enforce restrictions)
- **CostEstimate**: Calculated cost for a request containing: input token count, estimated output tokens, cost in USD, token count tier for billing

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Pipeline replaces Router::select_backend() god-function with 6 independent reconcilers, each testable in isolation with mocked AgentSchedulingProfiles
- **SC-002**: All existing Router::select_backend() integration tests pass without modification after pipeline integration
- **SC-003**: Pipeline total execution time (RequestAnalyzer through SchedulerReconciler) completes in under 1ms for 95% of requests under normal load
- **SC-004**: RequestAnalyzer completes alias resolution and requirement extraction in under 0.5ms per request
- **SC-005**: PrivacyReconciler correctly excludes 100% of cloud agents when TrafficPolicy specifies privacy="restricted"
- **SC-006**: BudgetReconciler tracks spending and transitions BudgetStatus correctly when crossing soft limit (75% default) and hard limit (100%)
- **SC-007**: When routing fails, Reject decisions contain actionable rejection_reasons from all reconcilers that excluded agents
- **SC-008**: Adding a new reconciler requires zero changes to existing reconcilers (O(1) feature interaction instead of O(n²))
- **SC-009**: TrafficPolicies load from optional [routing.policies.*] TOML sections, with system functioning correctly when sections are absent (zero required config)
- **SC-010**: BudgetReconciliationLoop aggregates spending from agent telemetry every 60 seconds with eventual consistency guarantees

## Assumptions

- **A-001**: RFC-001 Phase 1 (NII Extraction) is complete, providing RequestRequirements struct and extraction logic that RequestAnalyzer can reuse
- **A-002**: Agent implementations already provide agent.profile() method returning AgentSchedulingProfile, or this will be added as part of integration
- **A-003**: Agent implementations provide agent.count_tokens() method for cost estimation, or this will be added as part of BudgetReconciler integration
- **A-004**: Existing Router::select_backend() scoring logic can be extracted and moved into SchedulerReconciler without behavior changes
- **A-005**: TOML configuration format supports [routing.policies.*] sections with glob pattern matching syntax
- **A-006**: Existing agent telemetry system exposes spending data that BudgetReconciliationLoop can aggregate
- **A-007**: Default AgentSchedulingProfile values are reasonable: error_rate_1h=0.0, avg_ttft_ms=0, success_rate_24h=1.0 for agents without telemetry history
- **A-008**: Glob pattern matching for model_pattern in TrafficPolicy uses standard glob syntax (* for any chars, ? for single char)

## Dependencies

- **RFC-001 Phase 1**: NII Extraction must be complete to provide RequestRequirements struct and extraction logic
- **Agent Profile API**: Agents must expose profile() method returning privacy_zone, capability_tier, and performance metrics, or this must be implemented
- **Telemetry System**: Budget aggregation requires agent telemetry to track actual spending; if not available, BudgetReconciler will estimate only

## Out of Scope

- **Dashboard changes**: Visualization of reconciler decisions and rejection reasons in dashboard UI (future enhancement)
- **Metrics changes**: Detailed per-reconciler telemetry and latency breakdowns (future enhancement)
- **CLI changes**: Command-line tools for testing individual reconcilers or inspecting TrafficPolicies (future enhancement)
- **QualityReconciler implementation**: Mentioned in pipeline order but detailed requirements deferred to separate spec
- **Dynamic policy updates**: Hot-reloading of TrafficPolicies without restart (future enhancement)
- **Multi-region routing**: Cross-datacenter routing decisions (separate feature)
- **Cost prediction ML**: Machine learning models for output token estimation (BudgetReconciler uses simple heuristics)
- **Reconciler plugin system**: External/custom reconciler loading (future extensibility feature)

## Notes

This specification focuses on the architectural transformation from monolithic routing to pipeline-based routing. The pipeline pattern enables Privacy Zones (F13) and Budget Management (F14) to be implemented as independent reconcilers without complex interdependencies. Each reconciler only adds constraints to RoutingIntent; they never remove constraints from prior reconcilers, ensuring order-independence and composability.

The design prioritizes backward compatibility: Router::select_backend() remains the external interface, all existing tests pass, and zero configuration changes are required (TrafficPolicies are optional enhancements).

Performance budget is critical: 1ms total pipeline overhead ensures the architectural refactoring doesn't impact user-facing latency. RequestAnalyzer's 0.5ms budget reflects that alias resolution and requirement extraction must be extremely fast since they run on every request.
