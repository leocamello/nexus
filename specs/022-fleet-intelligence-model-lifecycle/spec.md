# Feature Specification: Fleet Intelligence and Model Lifecycle Management

**Feature Branch**: `022-fleet-intelligence-model-lifecycle`  
**Created**: 2025-01-19  
**Status**: Draft  
**Input**: User description: "Fleet Intelligence and Model Lifecycle (RFC-001 Phase 3) - Ship Model Lifecycle Management (F20) and Pre-warming & Fleet Intelligence (F19). Enable Nexus to control model loading/unloading across the fleet and predict demand for proactive model placement."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Manual Model Placement Control (Priority: P1)

As a platform operator, I need to explicitly load a specific model onto a backend agent to prepare capacity for anticipated workload, so I can ensure availability before demand spikes occur.

**Why this priority**: Foundational capability that enables manual control over model placement. Without this, no lifecycle management is possible. Provides immediate value for operators managing capacity during known events (product launches, scheduled load tests, maintenance windows).

**Independent Test**: Can be fully tested by triggering a model load via API on an idle backend with sufficient VRAM, verifying the model becomes available for routing, and delivers successful inference requests. This works standalone without any intelligence or pre-warming.

**Acceptance Scenarios**:

1. **Given** an Ollama backend with 8GB free VRAM and no models loaded, **When** operator sends POST /v1/models/load with model="llama3-8b" and backend_id="ollama-gpu-01", **Then** system initiates model pull, returns 202 Accepted with loading status, and backend enters HealthStatus::Loading state
2. **Given** a backend in HealthStatus::Loading state pulling llama3-8b at 45% completion, **When** router receives an inference request for llama3-8b, **Then** request is queued (not routed to loading backend) until load completes
3. **Given** a backend that successfully completed model load, **When** operator queries backend status, **Then** backend shows HealthStatus::Healthy with llama3-8b in loaded_models list
4. **Given** a backend with insufficient VRAM (model requires 8GB, only 4GB free), **When** operator attempts to load model, **Then** system returns 400 Bad Request with specific VRAM requirement details

---

### User Story 2 - Model Migration Across Backends (Priority: P2)

As a platform operator, I need to migrate a model from one backend to another (unload from backend A, load on backend B) to rebalance capacity or perform maintenance, without dropping active requests.

**Why this priority**: Builds on P1 by adding coordination between load/unload operations. Essential for operational flexibility (moving workloads during maintenance, optimizing GPU utilization). Requires P1's load capability to function.

**Independent Test**: Can be tested by loading a model on backend A, initiating migration to backend B, verifying backend A continues serving existing requests while backend B loads, and confirming traffic shifts only after backend B is healthy.

**Acceptance Scenarios**:

1. **Given** llama3-8b is loaded and serving traffic on backend A, **When** operator initiates migration to backend B via API, **Then** system starts loading model on backend B while keeping backend A active
2. **Given** backend A is serving active requests during migration, **When** new requests arrive for llama3-8b, **Then** they route to backend A until backend B completes loading
3. **Given** backend B successfully completed model load, **When** operator confirms migration completion, **Then** backend A unloads model (keepalive=0) and all new traffic routes to backend B
4. **Given** migration fails on backend B (insufficient VRAM discovered mid-pull), **When** system detects failure, **Then** backend A remains active, backend B is marked unhealthy, and operator receives detailed failure notification

---

### User Story 3 - Graceful Model Unloading (Priority: P3)

As a platform operator, I need to explicitly unload a model from a backend to free VRAM for other models, ensuring no active requests are disrupted.

**Why this priority**: Completes the lifecycle control toolkit by enabling capacity reclamation. Less critical than load/migration since operators can work around this by restarting backends. Can be implemented after P1/P2 deliver core value.

**Independent Test**: Can be tested by unloading an idle model (no active requests), verifying VRAM is released and reported via resource_usage(), and confirming new requests for that model are rejected or routed elsewhere.

**Acceptance Scenarios**:

1. **Given** llama3-8b is loaded on backend A with no active requests in last 5 minutes, **When** operator sends DELETE /v1/models/llama3-8b?backend_id=ollama-gpu-01, **Then** system unloads model and VRAM is released
2. **Given** a model has 3 active inference requests in progress, **When** operator attempts to unload that model, **Then** system returns 409 Conflict with active request count and refuses unload
3. **Given** model is successfully unloaded, **When** resource_usage() is queried, **Then** vram_used_bytes decreases by model's footprint and model is removed from loaded_models list
4. **Given** unload completes successfully, **When** new inference request arrives for that model, **Then** request is either rejected (503) or queued if auto-load is configured

---

### User Story 4 - Fleet Intelligence and Pre-warming Recommendations (Priority: P4)

As a platform operator, I want the system to analyze request patterns (time of day, model popularity trends) and recommend which models to pre-load on which backends, so I can proactively place models before demand materializes.

**Why this priority**: Delivers predictive optimization value but depends on P1 load capability and benefits from P2 migration for rebalancing. Should be implemented after manual controls are proven stable. Suggestion-first approach means it's advisory, not critical path.

**Independent Test**: Can be tested by simulating request history patterns (e.g., increased llama3-8b requests every weekday 9am-11am), running FleetReconciler analysis, and verifying it produces recommendations like "Pre-load llama3-8b on backend B at 8:45am" without actually executing loads.

**Acceptance Scenarios**:

1. **Given** historical data shows llama3-8b requests spike every weekday between 9am-11am, **When** FleetReconciler runs at 8:30am on a weekday, **Then** it recommends pre-loading llama3-8b on backends with available VRAM capacity
2. **Given** a recommendation to pre-load model X, **When** operator reviews recommendations via GET /v1/fleet/recommendations, **Then** response includes model_id, target_backend_ids, confidence score (based on pattern strength), and VRAM requirements
3. **Given** backend A has 8GB VRAM with 7.5GB used (93% utilization), **When** FleetReconciler considers pre-loading 2GB model, **Then** recommendation is not generated because headroom threshold (default 20%) would be violated
4. **Given** model is actively serving requests (hot model), **When** FleetReconciler considers rebalancing, **Then** it never recommends unloading that model, even if predictions suggest different placement
5. **Given** FleetReconciler produces 3 recommendations, **When** recommendations are logged, **Then** logs include model_id, target backends, reasoning (pattern detected), and required actions (load operations)

---

### Edge Cases

- **What happens when a model load is initiated but the backend becomes unreachable mid-pull?** System should detect timeout/health check failure, mark backend as Unhealthy, and allow retry or rollback to prevent indefinite Loading state.

- **How does the system handle concurrent load requests for the same model on the same backend?** Second request should be rejected (409 Conflict) with message indicating load already in progress, returning existing operation ID for status tracking.

- **What if resource_usage() reports VRAM capacity incorrectly (stale data)?** Load operations should fail fast if actual VRAM is insufficient, updating resource metrics and returning 507 Insufficient Storage with corrected capacity info.

- **How does migration handle the case where both backends A and B have the model loaded?** System treats this as successful migration completion state, unloads from source backend A, confirms backend B is primary, and updates routing tables.

- **What happens to queued requests if all backends enter Loading state simultaneously?** Requests remain queued up to configured timeout (e.g., 30s), then return 503 with Retry-After header indicating estimated completion time.

- **How does FleetReconciler handle model demand predictions when historical data is sparse (new model)?** Recommendations require minimum sample size (e.g., 7 days of data, 100+ requests). New models generate low-confidence suggestions or none until threshold met.

- **What if operator manually unloads a model that FleetReconciler recommended pre-loading?** Manual operations always take precedence. FleetReconciler observes the unload, removes recommendation, and re-evaluates next cycle without re-recommending immediately.

## Requirements *(mandatory)*

### Functional Requirements

#### Model Lifecycle Control (F20)

- **FR-001**: System MUST provide POST /v1/models/load API endpoint accepting model_id and backend_id to trigger model load on specific backend
- **FR-002**: System MUST provide DELETE /v1/models/{model_id} API endpoint accepting backend_id query parameter to trigger model unload from specific backend
- **FR-003**: System MUST expose HealthStatus::Loading { model_id, percent, eta_ms } state during model pull operations to track progress
- **FR-004**: System MUST prevent routing inference requests to backends in HealthStatus::Loading state until model load completes
- **FR-005**: System MUST implement LifecycleReconciler component that coordinates load/unload operations and integrates with SchedulerReconciler for queueing decisions
- **FR-006**: System MUST call OllamaAgent.load_model() which executes POST /api/pull to Ollama backend
- **FR-007**: System MUST call OllamaAgent.unload_model() which sends keepalive=0 or DELETE request to unload model
- **FR-008**: System MUST query OllamaAgent.resource_usage() via GET /api/ps to retrieve VRAM usage (vram_used_bytes, vram_total_bytes) before load operations
- **FR-009**: System MUST validate sufficient VRAM capacity exists before initiating model load, returning 400 Bad Request if insufficient
- **FR-010**: System MUST support model migration operations (coordinated unload from backend A + load on backend B) ensuring no request drops during transition
- **FR-011**: System MUST refuse unload operations if model has active inference requests in progress, returning 409 Conflict
- **FR-012**: System MUST update BackendStatus to reflect model additions/removals in loaded_models list after lifecycle operations complete

#### Fleet Intelligence & Pre-warming (F19)

- **FR-013**: System MUST implement FleetReconciler component that analyzes historical request patterns to identify model demand trends
- **FR-014**: System MUST track model request frequency over time, including time-of-day patterns and model popularity metrics
- **FR-015**: System MUST generate pre-warming recommendations identifying which models should be loaded on which backends based on predicted demand
- **FR-016**: System MUST expose GET /v1/fleet/recommendations API endpoint returning pre-warming suggestions with model_id, target_backend_ids, confidence score, and reasoning
- **FR-017**: System MUST log pre-warming recommendations with sufficient detail (model_id, backends, pattern detected, VRAM requirements) for operator review
- **FR-018**: System MUST query resource_usage() to determine VRAM headroom before generating pre-warming recommendations
- **FR-019**: System MUST respect configurable VRAM headroom budget (e.g., 20% minimum free capacity) and never recommend loads that would violate this threshold
- **FR-020**: System MUST never recommend unloading or migrating hot models (actively serving requests) to free capacity for predictions
- **FR-021**: System MUST prioritize pre-warming recommendations only for idle capacity, never disrupting active model serving
- **FR-022**: System MUST implement suggestion-first approach where recommendations are advisory and require operator/policy approval to execute
- **FR-023**: System MUST allow operators to configure minimum sample size thresholds (e.g., days of history, request count) before generating predictions
- **FR-024**: System MUST handle sparse historical data gracefully by generating low-confidence recommendations or none until thresholds are met

#### Integration & Performance

- **FR-025**: System MUST maintain routing decision latency under 1ms (per constitution latency budget) even with lifecycle state checks
- **FR-026**: System MUST keep memory overhead per backend under 10KB for lifecycle state tracking (per constitution memory constraints)
- **FR-027**: System MUST expose lifecycle operation status via OpenAI-compatible X-Nexus-* response headers only (maintaining API compatibility)
- **FR-028**: System MUST return 503 Service Unavailable with actionable context (Retry-After header, eta_ms) when all backends are in Loading state
- **FR-029**: System MUST integrate LifecycleReconciler into existing Reconciler pipeline maintaining stateless design principles
- **FR-030**: System MUST update OllamaAgent capabilities flags to model_lifecycle: true and resource_monitoring: true once implementations are active

### Key Entities

- **LifecycleOperation**: Represents a model load/unload/migration operation with fields: operation_id, operation_type (Load/Unload/Migrate), model_id, source_backend_id, target_backend_id (for migration), status (Pending/InProgress/Completed/Failed), progress_percent, eta_ms, initiated_at, completed_at, error_details

- **LoadingState**: Captures backend state during model pulls with fields: model_id, percent_complete, estimated_completion_ms, started_at, backend_id. Used to populate HealthStatus::Loading

- **ResourceSnapshot**: Point-in-time VRAM usage data with fields: backend_id, vram_used_bytes, vram_total_bytes, vram_free_bytes, loaded_models (list), pending_requests, timestamp. Retrieved via resource_usage() calls

- **PrewarmingRecommendation**: Fleet intelligence output with fields: recommendation_id, model_id, target_backend_ids, confidence_score (0.0-1.0), reasoning (pattern description), vram_required_bytes, generated_at, expires_at, status (Pending/Approved/Rejected/Executed)

- **RequestPattern**: Historical request analysis with fields: model_id, time_window (hourly/daily/weekly), request_count, avg_latency_ms, peak_hour, trend_direction (Increasing/Stable/Decreasing). Used by FleetReconciler for predictions

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Operators can successfully trigger model load operations via API and verify completion within model-specific pull times (e.g., 8B model loads in under 2 minutes on typical GPU)

- **SC-002**: System prevents routing of inference requests to backends in Loading state 100% of the time, with requests either queued or routed to alternative healthy backends

- **SC-003**: Model migration operations complete without dropping any active requests (0% request failure rate during migration window)

- **SC-004**: Unload operations correctly refuse when active requests exist (100% detection rate for in-flight requests preventing premature unload)

- **SC-005**: Fleet intelligence generates pre-warming recommendations within 5 seconds of demand pattern detection with confidence scores correlating to actual demand materialization

- **SC-006**: Pre-warming recommendations respect VRAM headroom budget 100% of the time (never recommend operations exceeding configured capacity threshold)

- **SC-007**: System never recommends unloading hot models for predictive placements (0% false evictions of actively serving models)

- **SC-008**: Routing decision latency remains under 1ms (P95) even with lifecycle state validation checks integrated

- **SC-009**: Memory baseline increases by less than 50MB total and per-backend overhead stays under 10KB for lifecycle tracking

- **SC-010**: Resource usage queries (resource_usage() calls) complete within 100ms to support real-time capacity planning decisions

- **SC-011**: All lifecycle operations emit sufficient diagnostic data (operation IDs, timestamps, progress) enabling operators to troubleshoot failures within 2 minutes

- **SC-012**: System correctly handles edge cases (concurrent loads, mid-pull failures, insufficient VRAM) with actionable error responses 100% of the time

## Assumptions

- **Phase Dependencies**: RFC-001 Phase 2.5 (Request Queueing) is complete and functional, providing queue infrastructure for handling requests to Loading backends

- **Ollama API Availability**: Target Ollama backends support /api/pull for model loading, keepalive=0 or DELETE for unloading, and /api/ps for resource monitoring

- **VRAM Reporting Accuracy**: Ollama /api/ps endpoint provides accurate real-time VRAM usage metrics with sub-second staleness

- **Historical Data Storage**: System has persistent storage mechanism for tracking request history (time-series data) to support pattern analysis. Implementation details deferred to planning phase

- **Operator Approval Workflow**: Initial implementation assumes manual operator review of pre-warming recommendations. Automated policy-based execution is future enhancement

- **Model Pull Times**: Typical model load times range from 30 seconds (small models) to 5 minutes (large models) depending on network bandwidth and model size. These estimates inform timeout configurations

- **Concurrent Load Limit**: System assumes backends handle one model load operation at a time. Concurrent pulls to same backend are rejected

- **VRAM Headroom Default**: Default VRAM headroom threshold is 20% (80% max utilization) unless operator configures otherwise via Nexus configuration

- **Minimum Sample Size**: Fleet intelligence requires minimum 7 days of historical data and 100+ requests per model before generating predictions. Below thresholds, recommendations are low-confidence or suppressed

- **Request Pattern Stability**: Demand patterns are assumed to exhibit weekly periodicity (e.g., weekday business hours). Irregular/chaotic patterns receive low confidence scores

- **Stateless Reconciler**: FleetReconciler and LifecycleReconciler integrate into existing reconciler pipeline maintaining stateless architecture (no session affinity required)

- **Error Recovery**: Failed lifecycle operations (timeouts, VRAM exhaustion) are detected via health checks and marked as backend Unhealthy, triggering automatic circuit breaker behavior

## Out of Scope

- **Automated Policy Execution**: This phase delivers recommendation APIs and logs only. Automated policy-based approval and execution of pre-warming operations is deferred to future phases

- **Cross-Cloud Model Caching**: Pre-loading models from shared registry or CDN-style distribution. Assumes models are pulled directly from Ollama backend sources per standard /api/pull behavior

- **Model Versioning and Rollback**: Managing multiple versions of same model or rolling back to previous versions during migration failures. Each model is treated as single entity (e.g., llama3-8b)

- **Fine-grained Cost Tracking**: Detailed cost allocation for VRAM usage, model pull bandwidth, or inference costs per model/tenant. Basic resource usage reporting only

- **Multi-Model Co-location Optimization**: Intelligent packing of multiple models on same backend to maximize GPU utilization. This phase focuses on single-model load/unload operations

- **Real-time Migration (Zero-Downtime)**: Seamless model migration where both source and target serve simultaneously during transition. This phase ensures no request drops but allows brief queueing

- **Predictive Autoscaling of Backends**: Automatically provisioning or deprovisioning backend agents based on predicted demand. Assumes fixed backend fleet capacity

- **Model Warm-up / First-Request Latency**: Optimizing initial inference latency after model load (some models have cold-start overhead). Focuses on load completion only

- **Custom Resource Metrics**: Extended resource monitoring beyond VRAM (GPU utilization %, memory bandwidth, PCIe throughput). Scope limited to vram_used_bytes and vram_total_bytes

- **Recommendation Expiration and Auto-Refresh**: Automatic invalidation of stale recommendations or continuous re-evaluation loops. Recommendations are generated on-demand via API calls

## Dependencies

- **Phase 2.5 Completion**: Request Queueing infrastructure (RoutingDecision::Queue, queue timeout handling) must be operational to handle requests during Loading states

- **Ollama Agent Extensions**: OllamaAgent implementation must activate load_model(), unload_model(), and resource_usage() methods replacing current Unsupported stubs

- **Health Status Infrastructure**: HealthStatus::Loading variant must be integrated into routing decision logic to block traffic to loading backends

- **Reconciler Pipeline**: Existing reconciler chain (RequestAnalyzer → Privacy → Budget → Tier → Quality → Scheduler) must support lifecycle state queries without breaking latency budget

- **Backend Registry**: BackendStatus and loaded_models tracking must be enhanced to persist lifecycle state changes and support concurrent reads

- **Request History Storage**: Time-series database or equivalent storage layer for tracking request patterns. Implementation approach TBD in planning phase

- **Configuration System**: Nexus configuration must support new settings: vram_headroom_percent, min_sample_days, min_request_count, lifecycle_timeout_ms

- **API Framework**: REST API handler infrastructure for new endpoints (/v1/models/load, DELETE /v1/models/{id}, /v1/fleet/recommendations) with OpenAI compatibility constraints

## Risks

- **Ollama API Gaps**: If Ollama /api/ps doesn't provide accurate VRAM metrics or /api/pull lacks progress reporting, lifecycle operations will be blind. **Mitigation**: Early prototype against real Ollama instance to validate API capabilities

- **Race Conditions During Migration**: Concurrent requests might target source backend during unload or hit target backend before load completes. **Mitigation**: Atomic state transitions with HealthStatus guards in routing logic

- **VRAM Estimation Errors**: Model VRAM requirements may vary by quantization or context size, causing load failures despite pre-checks. **Mitigation**: Add 10% buffer to VRAM estimates, implement fast-fail with detailed error messages

- **Historical Data Volume**: Request pattern tracking for entire fleet could generate significant storage overhead (10K req/min = 14M records/day). **Mitigation**: Aggregate to hourly buckets, retain only 30 days sliding window

- **Prediction Accuracy**: Demand predictions may be wrong, causing unnecessary pre-warming that wastes VRAM or misses actual spikes. **Mitigation**: Start with high-confidence patterns only (>0.8 score), monitor recommendation hit rate, iterate

- **Latency Budget Violation**: Adding lifecycle state checks to routing decision could breach 1ms latency budget. **Mitigation**: Use in-memory state cache, avoid I/O in hot path, profile extensively

- **Operator Overload**: Too many recommendations could create alert fatigue. **Mitigation**: Limit to top 5 recommendations per cycle, require minimum confidence threshold, provide batch approve/reject

- **Backend Hangs During Load**: Model pull could hang indefinitely if network stalls or Ollama process freezes. **Mitigation**: Implement operation timeout (default 5 min), health check failures trigger auto-abort

- **Request Queue Overflow**: If all backends enter Loading simultaneously, queue could overflow causing 503 failures. **Mitigation**: Set queue depth limits, implement backpressure with Retry-After headers

## Notes

- **Integration with RFC-001 Roadmap**: This phase activates NII methods defined since Phase 1, completing the model lifecycle story before Phase 4 (Advanced Routing) and Phase 5 (Telemetry)

- **Incremental Delivery**: User stories are prioritized to enable staged rollout: P1 (manual load) → P2 (migration) → P3 (unload) → P4 (intelligence). Each delivers standalone value

- **Ollama-Specific**: While NII trait is backend-agnostic, this phase focuses on Ollama implementation. Future backends (vLLM, TGI) would implement same trait methods

- **Suggestion-First Philosophy**: FleetReconciler deliberately does not auto-execute recommendations. Operators retain control, system provides insights. Aligns with Nexus constitution's transparency principle

- **Performance Non-Negotiable**: Routing latency and memory constraints from constitution are hard requirements. Lifecycle features must not compromise core proxy performance

- **Testing Strategy**: Each user story includes independent test approach. P1-P3 require real Ollama backends for integration tests. P4 can use simulated request history for pattern validation

- **Observability**: All lifecycle operations must emit structured logs with operation IDs for tracing. Critical for debugging migration failures or understanding why recommendations were generated

- **Future Evolution**: This phase lays groundwork for policy-based automation (Phase 6?), cost optimization, and multi-cloud model distribution, but keeps current scope tightly bounded
