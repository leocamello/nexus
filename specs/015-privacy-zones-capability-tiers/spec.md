# Feature Specification: Privacy Zones & Capability Tiers

**Feature Branch**: `015-privacy-zones-capability-tiers`  
**Created**: 2025-01-24  
**Status**: Draft  
**Input**: User description: "Structural enforcement of privacy boundaries and quality levels. Privacy is a backend property configured by the admin, NOT a request header that clients can forget. Capability tiers prevent silent quality downgrades during failover."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Privacy-Conscious Local Deployment (Priority: P1)

A developer deploys Nexus in an airgapped corporate environment with sensitive data. They configure local backends as "restricted" to ensure conversation history never leaves their infrastructure, even during failover scenarios. When local backends are unavailable, the system returns actionable 503 errors instead of silently routing to cloud providers.

**Why this priority**: Core privacy guarantee - violating privacy boundaries destroys trust and may violate legal/compliance requirements. This is the primary value proposition of the privacy zones feature.

**Independent Test**: Can be fully tested by configuring a restricted backend, forcing it offline, and verifying that requests return 503 with privacy context rather than routing to open/cloud backends. Delivers immediate privacy guarantee without other features.

**Acceptance Scenarios**:

1. **Given** a backend configured with `zone = "restricted"` and a request for a model served by that backend, **When** the request arrives, **Then** the system routes to the restricted backend and sets `X-Nexus-Privacy-Zone: restricted` response header
2. **Given** a restricted backend is offline and an open backend is available, **When** a request for the restricted backend's model arrives, **Then** the system returns 503 with `Retry-After` header and privacy zone context in the error response
3. **Given** multiple backends with different privacy zones, **When** a request arrives without privacy constraints, **Then** the system respects backend zone configuration and never cross-routes between zones
4. **Given** a request being served by a restricted backend, **When** the response is returned, **Then** the response includes `X-Nexus-Privacy-Zone: restricted` header to confirm privacy compliance

---

### User Story 2 - Quality-Aware Failover Control (Priority: P2)

A developer specifies model requirements in their application and wants explicit control over failover behavior. By default (strict mode), the system only routes to the exact requested model. When the developer opts into flexible mode via `X-Nexus-Flexible: true` header, the system can route to tier-equivalent alternatives but never silently downgrades to lower capability tiers.

**Why this priority**: Prevents silent quality degradation that could cause application failures or degraded user experience. While less critical than privacy violations, unexpected quality downgrades can break applications relying on specific model capabilities.

**Independent Test**: Can be tested by configuring backends with different capability tiers (1-5), making requests with and without the `X-Nexus-Flexible` header, and verifying routing behavior matches expectations. Delivers quality guarantees without privacy zones.

**Acceptance Scenarios**:

1. **Given** a backend configured with `tier = 3` and a request without `X-Nexus-Flexible` header, **When** the primary backend is unavailable, **Then** the system returns 503 with tier context rather than routing to a different tier backend
2. **Given** a backend with `tier = 3` is unavailable and another backend with `tier = 4` is available, **When** a request with `X-Nexus-Flexible: true` arrives, **Then** the system routes to the tier 4 backend (higher tier acceptable)
3. **Given** a backend with `tier = 3` is unavailable and only tier 2 backends are available, **When** a request with `X-Nexus-Flexible: true` arrives, **Then** the system returns 503 (lower tier not acceptable even in flexible mode)
4. **Given** a request with `X-Nexus-Strict: true` header, **When** routing occurs, **Then** the system enforces exact model matching regardless of tier configuration

---

### User Story 3 - Transparent Backend Configuration (Priority: P1)

An administrator configures backend privacy zones and capability tiers in the TOML configuration file. These settings are backend properties (not request-time decisions) that define structural enforcement boundaries. The configuration is clear, explicit, and cannot be accidentally bypassed by client headers.

**Why this priority**: Critical foundation - privacy and tier enforcement must be structurally configured, not request-dependent. This is a P1 because without proper configuration support, the other scenarios cannot function.

**Independent Test**: Can be tested by writing TOML config with `zone` and `tier` fields for backends, starting Nexus, and verifying the configuration is correctly parsed and applied. Configuration-only test that requires no request routing.

**Acceptance Scenarios**:

1. **Given** a backend configuration with `zone = "restricted"` in TOML, **When** Nexus starts, **Then** the backend's `AgentProfile` contains `privacy_zone = PrivacyZone::Restricted`
2. **Given** a backend configuration with `tier = 4` in TOML, **When** Nexus starts, **Then** the backend's `AgentProfile` contains `capability_tier = 4`
3. **Given** a backend configuration without explicit `zone` field, **When** Nexus starts, **Then** the backend defaults to `privacy_zone = PrivacyZone::Open`
4. **Given** a backend configuration without explicit `tier` field, **When** Nexus starts, **Then** the backend defaults to `capability_tier = 1`

---

### User Story 4 - Actionable Error Responses (Priority: P3)

When a request cannot be fulfilled due to privacy or tier constraints, the developer receives a 503 response with clear context explaining why the request failed (required privacy zone, required tier) and when to retry. This enables debugging and appropriate error handling in client applications.

**Why this priority**: Important for developer experience and debugging, but the system can function without detailed error messages. A P3 because basic rejection already works from P1/P2, this adds clarity.

**Independent Test**: Can be tested by triggering privacy/tier rejections and inspecting the 503 response body and headers for required context fields. Delivers better debuggability independently of other features.

**Acceptance Scenarios**:

1. **Given** a request that fails privacy zone enforcement, **When** the 503 response is generated, **Then** the response includes `privacy_zone_required` field indicating the required privacy zone
2. **Given** a request that fails tier enforcement, **When** the 503 response is generated, **Then** the response includes `required_tier` field indicating the minimum capability tier needed
3. **Given** any 503 rejection from privacy or tier enforcement, **When** the response is generated, **Then** the response includes a `Retry-After` header with appropriate retry timing
4. **Given** a 503 rejection response, **When** examined by a developer, **Then** the error message clearly explains the privacy or tier constraint that caused rejection

---

### User Story 5 - Zero-Config Backward Compatibility (Priority: P2)

An existing Nexus deployment without privacy policies or tier configuration continues to work unchanged. The default behavior (no policies = no filtering) ensures that privacy and tier enforcement are opt-in features that don't break existing deployments.

**Why this priority**: Essential for smooth adoption and backward compatibility, but less critical than core functionality. A P2 because it affects existing users but doesn't block new feature usage.

**Independent Test**: Can be tested by running Nexus with legacy configuration (no zone/tier fields) and verifying all requests route normally without enforcement. Demonstrates backward compatibility as a standalone property.

**Acceptance Scenarios**:

1. **Given** a configuration without any `zone` or `tier` fields in backend configs, **When** Nexus processes requests, **Then** routing behaves identically to pre-feature behavior (all backends treated as open, tier 1)
2. **Given** no TrafficPolicies defined in configuration, **When** requests arrive, **Then** privacy and tier reconcilers pass all candidates through without filtering
3. **Given** a mixed configuration with some backends having `zone` fields and others not, **When** routing occurs, **Then** backends without explicit zones are treated as "open" and participate in normal routing

---

### Edge Cases

- What happens when all restricted backends are offline and no open backends are available? System returns 503 with privacy context and retry timing rather than failing silently.
- How does system handle a request when all backends in the required tier are offline? System returns 503 with tier context, never downgrades silently even in flexible mode.
- What happens if a backend is marked as restricted but the TrafficPolicy doesn't specify privacy constraints? Backend's zone property is still enforced by PrivacyReconciler to prevent accidental cross-zone routing.
- How does system handle conflicting headers (both `X-Nexus-Strict` and `X-Nexus-Flexible`)? System defaults to strict mode (safer default, no surprises).
- What happens during a backend health check failure mid-request? If the restricted backend fails after routing decision but before completion, system returns 503 rather than attempting cross-zone failover.
- How does the system handle invalid tier values (< 1 or > 5) in configuration? System rejects configuration at startup with clear validation error.
- What happens if a request arrives with a privacy zone header? System ignores client-provided privacy headers - privacy is a backend property only, not controllable by clients.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST enforce privacy zone boundaries as backend configuration properties, not request-time headers
- **FR-002**: System MUST support two privacy zones: "restricted" (local-only) and "open" (can receive overflow)
- **FR-003**: System MUST prevent cross-zone failover - restricted backends never overflow to open backends
- **FR-004**: System MUST return 503 with `Retry-After` header when restricted backend is unavailable instead of cross-zone failover
- **FR-005**: System MUST support capability tiers 1-5 as backend configuration properties
- **FR-006**: System MUST enforce tier requirements during routing - only route to same-tier-or-higher backends
- **FR-007**: System MUST parse `X-Nexus-Strict: true` header to enforce exact model matching (default behavior)
- **FR-008**: System MUST parse `X-Nexus-Flexible: true` header to allow tier-equivalent alternatives
- **FR-009**: System MUST default to strict enforcement mode when neither header is present
- **FR-010**: System MUST include `X-Nexus-Privacy-Zone` header in all responses indicating the zone of the serving backend
- **FR-011**: System MUST wire PrivacyReconciler into Router's reconciler pipeline
- **FR-012**: System MUST wire TierReconciler into Router's reconciler pipeline
- **FR-013**: System MUST parse `zone` field from backend TOML configuration (values: "restricted" | "open", default: "open")
- **FR-014**: System MUST parse `tier` field from backend TOML configuration (values: 1-5, default: 1)
- **FR-015**: System MUST include privacy zone context in 503 error responses when privacy enforcement rejects request
- **FR-016**: System MUST include tier context in 503 error responses when tier enforcement rejects request
- **FR-017**: System MUST set tier enforcement mode on RoutingIntent based on request headers
- **FR-018**: System MUST validate tier values (1-5) in configuration at startup
- **FR-019**: System MUST validate zone values ("restricted" | "open") in configuration at startup
- **FR-020**: System MUST maintain zero-config backward compatibility - no policies means no filtering
- **FR-021**: System MUST treat backends without explicit zone configuration as "open"
- **FR-022**: System MUST treat backends without explicit tier configuration as tier 1
- **FR-023**: System MUST complete privacy and tier reconciliation within routing performance budget (< 1ms total)
- **FR-024**: System MUST NOT allow client-provided privacy headers to override backend zone configuration
- **FR-025**: System MUST maintain reconciler pipeline order: Privacy → Budget → Tier → Quality → Scheduler

### Key Entities

- **PrivacyZone**: Enumeration representing backend privacy boundaries (Restricted: local-only, never receive cross-zone traffic | Open: can receive overflow from any zone)
- **CapabilityTier**: Integer 1-5 representing backend quality/capability level (higher tier = more capable, can substitute for lower tiers in flexible mode)
- **BackendConfig**: Configuration structure containing `zone` (PrivacyZone, default: open) and `tier` (u8, default: 1) fields parsed from TOML
- **AgentProfile**: Runtime backend metadata containing `privacy_zone` and `capability_tier` fields used by reconcilers
- **RoutingIntent**: Request-scoped routing parameters containing `privacy_constraint`, `min_capability_tier`, and `tier_enforcement_mode` fields
- **TierEnforcementMode**: Enumeration controlling failover behavior (Strict: exact model only | Flexible: allow tier-equivalent alternatives)
- **TrafficPolicy**: Optional TOML configuration section defining routing policies with `model_pattern` globs and constraints
- **ActionableErrorContext**: Error context structure containing `required_tier` and `privacy_zone_required` fields for 503 responses

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Privacy zone enforcement prevents 100% of cross-zone routing attempts (restricted → open always returns 503, never routes)
- **SC-002**: Capability tier enforcement prevents 100% of silent quality downgrades (lower tier never substitutes for higher tier)
- **SC-003**: Privacy and tier reconciliation combined adds < 1ms to routing latency (measured P95)
- **SC-004**: All 503 responses from privacy/tier enforcement include actionable context (zone or tier information present in response)
- **SC-005**: Backend configuration with zone and tier fields parses successfully on Nexus startup (validation errors prevent startup with clear messages)
- **SC-006**: All responses include `X-Nexus-Privacy-Zone` header indicating serving backend's zone (100% coverage)
- **SC-007**: Default behavior (no explicit headers) enforces strict mode (exact model matching)
- **SC-008**: Flexible mode (`X-Nexus-Flexible: true`) allows higher-tier substitution while blocking lower-tier substitution
- **SC-009**: Existing deployments without zone/tier configuration continue functioning without modification (zero-config backward compatibility)
- **SC-010**: All existing reconciler pipeline tests pass without modification after wiring privacy and tier reconcilers into Router

## Assumptions *(optional)*

- Backend operators understand their privacy requirements and correctly configure zone values
- Capability tier values 1-5 provide sufficient granularity for quality differentiation
- Request headers (`X-Nexus-Strict`, `X-Nexus-Flexible`) are mutually exclusive (strict takes precedence if both present)
- The reconciler pipeline order (Privacy → Budget → Tier → Quality → Scheduler) is optimal for performance and correctness
- Backend health checks detect failures fast enough to trigger 503 responses before cross-zone failover attempts
- PrivacyReconciler and TierReconciler implementations from PR #157 are correct and complete
- TrafficPolicy glob matching for `model_pattern` supports standard glob syntax (*, ?, [])
- Backend capabilities remain stable during runtime (tier changes require restart)

## Out of Scope *(optional)*

- Dynamic tier adjustment based on observed backend performance (tier is static configuration)
- Per-user or per-request privacy preferences (privacy is backend-level only)
- Privacy zone hierarchy beyond two levels (no "semi-restricted" zone)
- Automatic tier discovery or inference (must be explicitly configured)
- Cross-zone request history migration or sanitization (no data flows between zones)
- Privacy zone auditing or compliance reporting (only enforcement at routing layer)
- Backend-specific tier overrides per model (tier applies to all models served by backend)
- Request-time tier overrides (tier enforcement cannot be disabled per request)
- Privacy zone propagation to upstream APIs (headers reflect only Nexus behavior)
- Fallback chain tier validation (tier enforcement at routing time, not chain definition time)

## Dependencies *(optional)*

- **Control Plane Reconciler Pipeline** (PR #157): PrivacyReconciler and TierReconciler implementations already exist and are tested
- **Backend Registry** (F1): Backend configuration parsing and AgentProfile creation
- **Health Checker** (F2): Backend availability detection to trigger 503 responses
- **Router** (F6): Pipeline construction and reconciler wiring
- **API Error Handling** (existing): ActionableErrorContext structure and 503 response formatting
- **Config System** (F3): TOML parsing for backend zone and tier fields
- **Request Headers** (existing): Header parsing infrastructure for X-Nexus-* headers

## Related Features *(optional)*

- **F6 - Intelligent Router**: Privacy and tier reconcilers integrate into Router's pipeline
- **F8 - Fallback Chains**: Cross-zone failover prevention affects fallback chain traversal
- **F9 - Request Metrics**: Privacy zone and tier should be included in routing metrics/logs
- **F14 - Control Plane Reconciler** (PR #157): Provides PrivacyReconciler and TierReconciler implementations
- **F13 - Cloud Backend Support**: Privacy zones distinguish local vs cloud backends
