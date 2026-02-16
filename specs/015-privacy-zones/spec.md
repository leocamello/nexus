# Feature Specification: Privacy Zones & Capability Tiers

**Feature Branch**: `015-privacy-zones`  
**Created**: 2025-02-16  
**Status**: Draft  
**Input**: User description: "Structural enforcement of privacy boundaries and quality levels. Privacy is a backend property configured by the admin, NOT a request header that clients can forget. Capability tiers prevent silent quality downgrades during failover."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Enforce Privacy Boundaries at Backend Level (Priority: P1)

An enterprise administrator configures certain AI backends (local GPU servers) as "restricted" privacy zones, ensuring sensitive customer data never leaves the organization's infrastructure. When a developer makes API requests for customer data analysis, the system automatically routes to restricted backends only, blocking any overflow to cloud providers even during peak load.

**Why this priority**: Core security requirement - prevents accidental data leaks to cloud providers. Without this, the entire privacy enforcement model fails. This is the foundational capability that all other privacy features depend on.

**Independent Test**: Can be fully tested by configuring a backend as "restricted", sending requests that would normally overflow to cloud, and verifying the request either succeeds on local backend or fails with 503 (never silently routes to cloud). Delivers immediate value by guaranteeing data locality.

**Acceptance Scenarios**:

1. **Given** a backend is configured with `zone = "restricted"` and cloud backends are configured as `zone = "open"`, **When** a request arrives during normal load, **Then** the request is routed only to the restricted backend
2. **Given** a restricted backend is at capacity, **When** a new request arrives, **Then** the system returns 503 with Retry-After header instead of routing to cloud backends
3. **Given** a backend is configured as `zone = "open"`, **When** overflow occurs from any zone, **Then** the backend can receive requests from restricted or open zones
4. **Given** multiple restricted backends are available, **When** a request needs routing, **Then** the system uses backend affinity (sticky routing) to maintain conversation context on the same backend

---

### User Story 2 - Prevent Quality Downgrades During Failover (Priority: P1)

A developer building a code generation tool explicitly requests a high-capability model (e.g., reasoning=9, coding=9). During peak hours when that model is unavailable, the system should never silently downgrade to a lower-tier model that produces inferior results. Instead, it either waits for the requested tier or returns a clear error so the developer can make an informed decision.

**Why this priority**: Core quality guarantee - prevents silent degradation that could break client applications. Critical for production reliability where developers need predictable model behavior. Equally foundational as privacy enforcement.

**Independent Test**: Can be fully tested by configuring backends with different capability scores, requesting a high-tier model, making the high-tier backend unavailable, and verifying the system either finds an equivalent tier or returns 503 (never silently downgrades). Delivers immediate value by ensuring predictable quality.

**Acceptance Scenarios**:

1. **Given** a backend declares `capability_tier = {reasoning=9, coding=9}` and another declares `capability_tier = {reasoning=6, coding=7}`, **When** a request targets the high-tier backend and it's unavailable, **Then** the system returns 503 instead of routing to the lower-tier backend (default strict mode)
2. **Given** backends with different capability tiers, **When** overflow occurs, **Then** the system only routes to same-tier-or-higher backends
3. **Given** a backend declares multiple capability scores, **When** the TierReconciler evaluates a request, **Then** all declared capabilities must meet or exceed the minimum tier requirements from TrafficPolicy
4. **Given** a TrafficPolicy specifies minimum capability requirements, **When** no backend meets those requirements, **Then** the system returns 503 with tier context explaining why routing failed

---

### User Story 3 - Client Control Over Routing Flexibility (Priority: P2)

A developer working on a chatbot application wants to allow tier-equivalent model alternatives during failover to maintain availability, while another developer working on a compliance tool requires the exact requested model with no substitutions. Both developers use request headers to express their preferences, and the system respects those choices.

**Why this priority**: Important for production flexibility and developer experience, but depends on P1 privacy and tier enforcement being in place. Enables developers to optimize for either strict quality or high availability based on their use case.

**Independent Test**: Can be fully tested by sending requests with `X-Nexus-Strict: true` vs `X-Nexus-Flexible: true` headers when the requested backend is unavailable, and verifying strict mode returns 503 while flexible mode allows tier-equivalent alternatives. Delivers value by giving developers control over availability vs. consistency tradeoffs.

**Acceptance Scenarios**:

1. **Given** a request includes `X-Nexus-Strict: true` header, **When** the requested model is unavailable, **Then** the system returns 503 instead of routing to any alternative
2. **Given** a request includes `X-Nexus-Flexible: true` header, **When** the requested model is unavailable, **Then** the system routes to a tier-equivalent backend if available
3. **Given** no routing preference header is provided, **When** the requested model is unavailable, **Then** the system defaults to strict mode (never surprise the developer)
4. **Given** a flexible routing request, **When** only lower-tier alternatives exist, **Then** the system still returns 503 (flexible allows lateral substitution, not downgrades)

---

### User Story 4 - Cross-Zone Overflow with Context Protection (Priority: P2)

During maintenance windows or capacity constraints, some overflow from restricted zones to open zones is necessary, but only for fresh conversations (no history). A developer starts a new conversation that overflows to cloud, but when they continue that conversation, the system either maintains cloud routing (with fresh context each time) or blocks history forwarding to prevent sensitive data leakage.

**Why this priority**: Enables operational flexibility while maintaining privacy guarantees. Important for real-world deployments but requires P1 privacy enforcement as foundation.

**Independent Test**: Can be fully tested by forcing overflow from restricted to open zone with a new conversation (succeeds), then attempting to forward conversation history in subsequent requests (blocked with clear error). Delivers value by balancing availability with privacy.

**Acceptance Scenarios**:

1. **Given** a restricted backend is unavailable and an open backend is available, **When** a new request arrives with no conversation history, **Then** the system allows overflow to the open zone
2. **Given** a conversation started in a restricted zone, **When** overflow to open zone occurs, **Then** the system blocks forwarding of conversation history to the open zone
3. **Given** a conversation started in an open zone, **When** subsequent requests arrive, **Then** the system maintains backend affinity within the open zone
4. **Given** cross-zone overflow is configured to block entirely, **When** restricted backend is unavailable, **Then** the system returns 503 instead of allowing any overflow

---

### User Story 5 - Actionable Error Responses for Debugging (Priority: P3)

A developer receives a 503 error when their request cannot be routed due to privacy or tier constraints. The error response includes structured context (privacy zone mismatch, tier unavailable, etc.) and a Retry-After header, enabling them to understand why routing failed and when to retry.

**Why this priority**: Enhances developer experience and debugging but doesn't affect core functionality. Can be added after P1/P2 features are working.

**Independent Test**: Can be fully tested by triggering various rejection scenarios (privacy mismatch, tier unavailable) and verifying the 503 response includes appropriate RejectionReason context and Retry-After headers. Delivers value by reducing troubleshooting time.

**Acceptance Scenarios**:

1. **Given** a request cannot be routed due to privacy constraints, **When** the system returns 503, **Then** the response includes RejectionReason indicating privacy zone mismatch
2. **Given** a request cannot be routed due to tier unavailability, **When** the system returns 503, **Then** the response includes RejectionReason indicating which capability requirements were not met
3. **Given** all matching backends are at capacity, **When** the system returns 503, **Then** the response includes a Retry-After header suggesting when capacity may be available
4. **Given** a routing failure occurs, **When** the error is logged, **Then** PrivacyReconciler or TierReconciler logs actionable context for administrators

---

### Edge Cases

- What happens when a backend's privacy zone configuration changes while requests are in flight? (System should complete in-flight requests with old configuration, apply new configuration to subsequent requests)
- How does the system handle backends that partially meet tier requirements? (Must meet ALL declared capability minimums from TrafficPolicy; partial match = rejection)
- What happens when a restricted conversation has been running for hours and the backend fails mid-conversation? (Return 503 with Retry-After; client must retry when backend recovers; never fail over to cloud with history)
- How does backend affinity work across service restarts? (Affinity is per-request cycle based on AgentProfile; no persistent session state per Principle VIII)
- What happens when TrafficPolicy specifies conflicting privacy and tier requirements? (Privacy takes precedence; route to restricted backends first, then apply tier filtering within that zone)
- How does the system handle requests when NO backends match privacy+tier requirements? (Return 503 immediately with clear explanation; never degrade privacy or tier guarantees)
- What happens during graceful backend shutdown? (Backend stops accepting new requests but completes in-flight requests; reconcilers exclude backend from routing pool immediately)

## Requirements *(mandatory)*

### Functional Requirements

#### Privacy Zone Enforcement

- **FR-001**: System MUST enforce privacy zones as backend properties configured in AgentProfile, not as request headers
- **FR-002**: System MUST support two privacy zone types: "restricted" (local-only) and "open" (can receive overflow)
- **FR-003**: PrivacyReconciler MUST run in the Reconciler Pipeline before TierReconciler, BudgetReconciler, and QualityReconciler
- **FR-004**: PrivacyReconciler MUST read zone configuration from AgentProfile for each backend
- **FR-005**: PrivacyReconciler MUST exclude backends with mismatched privacy zones from the routing pool
- **FR-006**: System MUST never route restricted-zone traffic to open-zone backends during overflow, except for fresh conversations (no history)
- **FR-007**: System MUST support backend affinity (sticky routing) to maintain conversation context within restricted zones
- **FR-008**: System MUST block forwarding of conversation history when cross-zone overflow occurs
- **FR-009**: System MUST allow administrators to configure cross-zone overflow as either "fresh-only" or "block-entirely" via TrafficPolicy

#### Capability Tier Enforcement

- **FR-010**: System MUST enforce capability tiers as backend properties declared in AgentSchedulingProfile
- **FR-011**: System MUST support capability scores for: reasoning, coding, context_window, vision, and tools
- **FR-012**: TierReconciler MUST run in the Reconciler Pipeline after PrivacyReconciler but before SchedulerReconciler
- **FR-013**: TierReconciler MUST read capability_tier configuration from AgentSchedulingProfile for each backend
- **FR-014**: TierReconciler MUST enforce minimum tier requirements specified in TrafficPolicy for matching routes
- **FR-015**: System MUST only allow overflow to backends with same-tier-or-higher capability scores (never downgrade)
- **FR-016**: System MUST support client routing preferences via X-Nexus-Strict header (default: true, only exact model)
- **FR-017**: System MUST support client routing preferences via X-Nexus-Flexible header (allows tier-equivalent alternatives)
- **FR-018**: System MUST default to strict routing mode when no client preference header is provided

#### Error Handling & Observability

- **FR-019**: System MUST return 503 status code when no backends match privacy and tier requirements
- **FR-020**: System MUST include Retry-After header in 503 responses indicating when to retry
- **FR-021**: PrivacyReconciler MUST log RejectionReason when excluding backends due to zone mismatch
- **FR-022**: TierReconciler MUST log RejectionReason when excluding backends due to tier mismatch
- **FR-023**: 503 responses MUST include structured context (privacy zone, tier requirements, available alternatives) for debugging
- **FR-024**: System MUST expose metrics for privacy zone rejections and tier rejections for monitoring

#### Configuration & Integration

- **FR-025**: TrafficPolicies MUST support optional privacy zone requirements specified as TOML sections: `[routing.policies."pattern"] = { privacy = "restricted" }`
- **FR-026**: TrafficPolicies MUST support optional minimum capability requirements specified in TOML: `{ min_reasoning = 8, min_coding = 7 }`
- **FR-027**: System MUST apply TrafficPolicy rules per-request based on route pattern matching
- **FR-028**: System MUST support multiple TrafficPolicies with different privacy and tier requirements for different route patterns

### Key Entities

- **Privacy Zone**: Configuration property of a backend (AgentProfile) indicating whether it can receive overflow traffic. Values: "restricted" (local-only), "open" (accepts overflow from any zone). Enforced by PrivacyReconciler.

- **Capability Tier**: Multi-dimensional quality scores declared by a backend (AgentSchedulingProfile) indicating its capabilities across reasoning, coding, context_window, vision, and tools. Used by TierReconciler to prevent quality downgrades.

- **TrafficPolicy**: Optional TOML configuration section defining privacy and tier requirements for specific route patterns. Reconcilers use these policies to filter backend pools before scheduling.

- **AgentProfile**: Backend metadata containing privacy zone configuration (`zone = "restricted"` or `zone = "open"`). Read by PrivacyReconciler to enforce data locality.

- **AgentSchedulingProfile**: Backend metadata containing capability tier declarations (`capability_tier = { reasoning = 9, coding = 8, ... }`). Read by TierReconciler to enforce quality guarantees.

- **RejectionReason**: Structured logging context emitted by Reconcilers explaining why a backend was excluded from routing (e.g., "privacy_zone_mismatch", "tier_insufficient_reasoning"). Used for actionable 503 responses and observability.

- **Reconciler Pipeline**: Ordered execution of PrivacyReconciler → BudgetReconciler → TierReconciler → QualityReconciler → SchedulerReconciler. Privacy and tier enforcement happen early in the pipeline to reduce unnecessary processing.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of requests to restricted backends remain within configured privacy zone (zero cloud overflow with conversation history)
- **SC-002**: 100% of capability tier enforcement prevents silent quality downgrades (zero requests routed to lower-tier backends without explicit client consent)
- **SC-003**: When requested tier is unavailable, system responds with 503 within 100ms (no long timeouts waiting for unavailable backends)
- **SC-004**: 503 error responses include actionable context (privacy zone, tier requirements) enabling developers to resolve issues within one retry cycle
- **SC-005**: Backend affinity (sticky routing) maintains conversation context within restricted zones across 95% of multi-turn conversations (only breaks on backend failure)
- **SC-006**: Cross-zone overflow for fresh conversations succeeds 99% of the time when open-zone backends have capacity
- **SC-007**: Administrators can configure and deploy new privacy zones or tier requirements without service restart (configuration hot-reloads)
- **SC-008**: System processes privacy and tier reconciliation within 10ms per request (minimal overhead in the reconciler pipeline)

## Assumptions *(mandatory)*

1. **Privacy zones are configured by administrators in backend TOML files** (AgentProfile), not dynamically adjusted by the system
2. **Capability tier scores are self-reported by backends** during registration; no automatic capability detection or benchmarking
3. **TrafficPolicies are optional**; if no policy is defined for a route, privacy and tier enforcement uses backend defaults only
4. **Backend affinity (sticky routing) is best-effort**; backend failures may break affinity and cause 503 errors
5. **Fresh conversations are defined as requests with no conversation history** (empty context); the system does not track conversation lifetime
6. **Cross-zone overflow defaults to "block-entirely"** unless explicitly configured as "fresh-only" in TrafficPolicy
7. **Retry-After headers suggest conservative wait times** (e.g., 30-60 seconds) based on typical backend recovery times, not real-time capacity prediction
8. **Client routing headers (X-Nexus-Strict, X-Nexus-Flexible) apply only to tier enforcement**, not privacy zone enforcement (privacy is never flexible)
9. **Metrics for privacy/tier rejections are exposed via standard observability endpoints** (Prometheus, OpenTelemetry); no custom monitoring UI required
10. **Configuration hot-reload is supported for TrafficPolicies and backend profiles**; changes take effect within one configuration refresh cycle (typically 5-30 seconds)

## Dependencies *(mandatory)*

### Internal Dependencies

- **RFC-001 (Control Plane Architecture)**: This feature implements PrivacyReconciler and TierReconciler as specified in the Reconciler Pipeline design
- **AgentProfile schema**: Must include `zone` field for privacy zone configuration
- **AgentSchedulingProfile schema**: Must include `capability_tier` field for multi-dimensional capability scores
- **TrafficPolicy TOML parsing**: Configuration loader must support optional `privacy` and capability tier fields in routing policies
- **Reconciler Pipeline**: Privacy and Tier reconcilers must integrate into the existing pipeline execution order
- **Error response formatting**: 503 responses must support structured RejectionReason context for observability

### External Dependencies

- **Backend configuration**: Administrators must manually configure privacy zones and capability tiers in backend TOML files; no auto-discovery
- **Observability infrastructure**: Metrics for privacy/tier rejections require Prometheus or OpenTelemetry collectors
- **Load balancer / routing layer**: Must support Retry-After headers and 503 status codes for proper client retry behavior

### Integration Points

- **Reconciler Pipeline**: PrivacyReconciler and TierReconciler hook into the existing pipeline (Privacy → Budget → Tier → Quality → Scheduler)
- **Configuration loader**: Must read and parse AgentProfile, AgentSchedulingProfile, and TrafficPolicy TOML files
- **Error handler**: Must format 503 responses with RejectionReason context and Retry-After headers
- **Metrics exporter**: Must expose privacy_zone_rejections_total and tier_rejections_total counters

## Out of Scope *(mandatory)*

1. **Automatic capability tier detection or benchmarking**: Backends self-report capability scores; the system does not validate or measure them
2. **Dynamic privacy zone adjustment based on load**: Privacy zones are static configuration; no runtime zone changes
3. **Client-controlled privacy zones via request headers**: Privacy is a backend property only; clients cannot request specific zones
4. **Persistent session tracking for backend affinity**: Affinity is per-request cycle based on conversation context; no session state (per Principle VIII)
5. **Quality-of-Service (QoS) tiering beyond capability scores**: This feature enforces declared capabilities only; advanced QoS (priority queues, SLA enforcement) is separate
6. **Cross-zone conversation history migration**: When overflow occurs, history is blocked entirely; no automatic history sanitization or partial forwarding
7. **Multi-region privacy zones**: This feature handles single-deployment privacy boundaries; multi-region deployments with separate control planes are out of scope
8. **Client authentication or authorization integration**: Privacy zones enforce data locality, not access control; authentication is handled by a separate system
9. **Cost-based routing decisions**: Tier enforcement is quality-focused; cost optimization (budget-aware routing) is handled by BudgetReconciler
10. **Fallback to degraded modes**: The system never silently downgrades quality or compromises privacy; strict failure (503) is preferred over degraded success

## Risks *(optional)*

### Technical Risks

- **Risk**: Backend affinity may cause load imbalance if many conversations stick to one restricted backend
  - **Mitigation**: Use consistent hashing for affinity to distribute load; monitor backend utilization metrics

- **Risk**: Strict tier enforcement may reduce availability if high-tier backends are frequently unavailable
  - **Mitigation**: Encourage deployments to run multiple replicas of high-tier backends; provide clear 503 errors for capacity planning

- **Risk**: Configuration errors (typos in privacy zone or tier declarations) may cause unexpected routing failures
  - **Mitigation**: Validate TOML configuration on load; provide clear error messages for invalid values

### Operational Risks

- **Risk**: Administrators may misconfigure privacy zones, accidentally allowing sensitive data to reach cloud backends
  - **Mitigation**: Default to "restricted" privacy zone if not explicitly configured; require explicit "open" declaration

- **Risk**: Developers may not understand why requests fail with 503 due to tier constraints
  - **Mitigation**: Include detailed RejectionReason in error responses; document common scenarios in runbooks

### Compliance Risks

- **Risk**: Cross-zone overflow with "fresh-only" mode may still leak metadata (request patterns, timing) to cloud backends
  - **Mitigation**: Document that metadata is not protected; recommend "block-entirely" mode for strict compliance requirements
