# Implementation Plan: Fleet Intelligence and Model Lifecycle Management

**Branch**: `022-fleet-intelligence-model-lifecycle` | **Date**: 2025-01-19 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/022-fleet-intelligence-model-lifecycle/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

This phase implements RFC-001 Phase 3, delivering Model Lifecycle Management (F20) and Fleet Intelligence with Pre-warming (F19). The system will enable explicit control over model loading/unloading operations across the backend fleet and provide predictive recommendations for proactive model placement based on demand patterns.

**Primary Requirements**:
- Manual model placement control via POST /v1/models/load and DELETE /v1/models/{id} APIs
- Model migration support (coordinated load/unload) without dropping active requests
- HealthStatus::Loading state integration to prevent routing to backends during model pulls
- FleetReconciler for analyzing request patterns and generating pre-warming recommendations
- LifecycleReconciler for coordinating load/unload operations and integrating with scheduler
- OllamaAgent implementations for load_model() (POST /api/pull), unload_model() (keepalive=0), and resource_usage() (GET /api/ps)
- VRAM capacity validation before load operations with configurable headroom budgets
- Suggestion-first approach where recommendations are advisory, requiring operator approval

**Technical Approach**:
- Implement lifecycle operations as Ollama API wrappers with progress tracking
- Add LifecycleReconciler to existing reconciler pipeline for state validation during routing
- Create FleetReconciler as background task analyzing request history for pattern detection
- Extend BackendStatus to track lifecycle operation state and loaded models
- Add configuration support for VRAM headroom thresholds, sample sizes, and operation timeouts
- Maintain <1ms routing latency and <10KB per-backend memory overhead per constitution

## Technical Context

**Language/Version**: Rust 1.87 (stable toolchain, edition 2021)  
**Primary Dependencies**: 
- Axum 0.7 (HTTP framework for API endpoints)
- Tokio 1.x (async runtime with full features)
- reqwest 0.12 (HTTP client for Ollama API calls with connection pooling)
- DashMap 6.x (concurrent HashMap for registry state)
- serde/serde_json 1.x (serialization for API contracts)
- chrono 0.4 (timestamp handling for request patterns)

**Storage**: In-memory only (DashMap for BackendRegistry, no persistence required per constitution)
- Request history for pattern analysis: In-memory storage using DashMap with hourly aggregation buckets (720 buckets × 30 day sliding window, capped at ~2MB)
- Lifecycle operation state: In-memory tracking within BackendStatus
- VRAM metrics: Real-time queries via Ollama /api/ps (no caching beyond health check interval)

**Testing**: 
- Unit tests: `cargo test` with `#[cfg(test)]` modules
- Integration tests: `tests/` directory with wiremock 0.6 for mocking Ollama APIs
- Property-based tests: proptest 1.x for FleetReconciler prediction validation
- Test-Driven Development: All tests written and verified to FAIL before implementation (per constitution)

**Target Platform**: Linux/macOS/Windows servers (cross-platform, no GUI)

**Project Type**: Single Rust workspace (existing nexus-orchestrator binary + library)

**Performance Goals**: 
- Routing decision latency: <1ms (P95) including lifecycle state checks (FR-025, constitution requirement)
- Resource usage query: <100ms for Ollama /api/ps calls (SC-010)
- Model load operation: 30s-5min depending on model size (8B model target: <2min per SC-001)
- Fleet analysis cycle: <5s to generate recommendations (SC-005)

**Constraints**: 
- Memory overhead: <10KB per backend for lifecycle tracking (FR-026, constitution limit)
- Total memory increase: <50MB baseline (constitution limit)
- OpenAI API compatibility: Must not break /v1/chat/completions or /v1/models contracts
- Stateless design: No session affinity, all state operational (constitution principle VIII)
- Zero-config philosophy: Lifecycle features work with sensible defaults, configuration optional

**Scale/Scope**: 
- Target fleet size: 10-100 backends (home lab to small enterprise)
- Request volume: 10K req/min for pattern analysis (14M records/day per risk analysis)
- Historical data retention: 30 days rolling window with hourly aggregation
- Concurrent lifecycle operations: 1 per backend (sequential loads, reject concurrent on same backend)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Simplicity Gate ✅

- [x] **Using ≤3 main modules for initial implementation?** YES
  - `src/agent/ollama.rs` (load_model/unload_model/resource_usage implementations)
  - `src/routing/reconciler/lifecycle.rs` (LifecycleReconciler for state checks)
  - `src/routing/reconciler/fleet.rs` (FleetReconciler for pattern analysis)
  - `src/api/lifecycle.rs` (API endpoints for load/unload/recommendations)
  
- [x] **No speculative "might need" features?** YES
  - Only implementing P1-P4 user stories from spec
  - Automated policy execution explicitly deferred (out of scope)
  - Multi-model optimization deferred (out of scope)
  - Cross-cloud caching deferred (out of scope)

- [x] **No premature optimization?** YES
  - Starting with in-memory request tracking (complexity deferred to research phase)
  - Simple pattern detection (time-of-day + popularity, no ML)
  - Direct Ollama API calls without caching layers
  - VRAM estimates with 10% buffer, not complex prediction models

- [x] **Start with simplest approach that could work?** YES
  - Manual load/unload before intelligence (P1→P2→P3→P4 priority order)
  - Suggestion-first recommendations (no auto-execution)
  - Hourly aggregation for request patterns (not streaming analytics)

### Anti-Abstraction Gate ✅

- [x] **Using Axum/Tokio/reqwest directly (no wrapper layers)?** YES
  - API endpoints use Axum handlers directly
  - Ollama calls use reqwest with existing HTTP client
  - No new HTTP abstractions or adapter patterns

- [x] **Single representation for each data type?** YES
  - LifecycleOperation: tracks operation state
  - ResourceUsage: already defined in types.rs (reuse)
  - HealthStatus::Loading: already defined (reuse)
  - PrewarmingRecommendation: new type for fleet intelligence output

- [x] **No "framework on top of framework" patterns?** YES
  - LifecycleReconciler implements existing Reconciler trait
  - FleetReconciler is standalone background task (no new pipeline abstraction)
  - Integrates with existing ReconcilerPipeline without modification

- [x] **Abstractions justified by actual (not theoretical) needs?** YES
  - Reconciler trait already exists (Phase 2.5)
  - ResourceUsage struct already defined (stub since Phase 1)
  - HealthStatus::Loading already defined (stub since Phase 2)
  - New abstractions limited to domain entities from spec (LifecycleOperation, PrewarmingRecommendation)

### Integration-First Gate ✅

- [x] **API contracts defined before implementation?** YES
  - Phase 1 generates OpenAPI spec for POST /v1/models/load, DELETE /v1/models/{id}, GET /v1/fleet/recommendations
  - Request/response schemas in contracts/ directory
  - Error response formats specified (400, 409, 503, 507)

- [x] **Integration tests planned with real/mock backends?** YES
  - wiremock for mocking Ollama /api/pull, /api/ps, keepalive=0 responses
  - Test scenarios for P1-P4 acceptance criteria
  - Health check integration with Loading state
  - End-to-end migration flow (unload + load coordination)

- [x] **End-to-end flow testable?** YES
  - P1: Load model → verify Loading state → confirm Healthy with model in loaded_models
  - P2: Migration → verify coordination → confirm traffic shift
  - P3: Unload model → verify VRAM release → confirm model removed
  - P4: Pattern analysis → verify recommendations → check VRAM constraints

### Performance Gate ✅

- [x] **Routing decision target: <1ms?** YES
  - LifecycleReconciler only checks in-memory HealthStatus (no I/O)
  - Leverages existing health check infrastructure (30s polling interval)
  - No additional backend queries in hot path

- [x] **Total overhead target: <5ms?** YES
  - Lifecycle state check adds ~0.1ms (memory read)
  - No streaming or blocking operations in reconciler
  - Resource usage queries happen out-of-band (health checker background task)

- [x] **Memory baseline target: <50MB?** YES
  - Per-backend tracking: ~2KB (LifecycleOperation state)
  - Request pattern aggregates: ~5MB for 30-day hourly buckets (10K req/min rate)
  - FleetReconciler state: ~1MB (recommendation cache)
  - Total estimated increase: ~8MB (well under 50MB constitution limit)

**GATE STATUS: ✅ ALL GATES PASSED** - Proceed to Phase 0 research.

## Project Structure

### Documentation (this feature)

```text
specs/022-fleet-intelligence-model-lifecycle/
├── spec.md              # Feature specification (user stories, requirements, entities)
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output: unknowns resolved, design decisions documented
├── data-model.md        # Phase 1 output: entities, state machines, relationships
├── quickstart.md        # Phase 1 output: integration guide for lifecycle operations
├── contracts/           # Phase 1 output: OpenAPI/JSON schemas for new endpoints
│   ├── load-model.yaml          # POST /v1/models/load request/response
│   ├── unload-model.yaml        # DELETE /v1/models/{id} request/response
│   └── fleet-recommendations.yaml  # GET /v1/fleet/recommendations response
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
src/
├── agent/
│   ├── mod.rs                    # InferenceAgent trait (load_model/unload_model/resource_usage stubs exist)
│   ├── types.rs                  # HealthStatus::Loading, ResourceUsage (already defined)
│   └── ollama.rs                 # [MODIFIED] Implement lifecycle methods for OllamaAgent
│
├── api/
│   ├── mod.rs                    # API router registration
│   ├── lifecycle.rs              # [NEW] Lifecycle API endpoints (load/unload/recommendations)
│   └── types.rs                  # [MODIFIED] Add lifecycle request/response types
│
├── routing/reconciler/
│   ├── mod.rs                    # [MODIFIED] Add lifecycle & fleet to pipeline
│   ├── lifecycle.rs              # [NEW] LifecycleReconciler (validate backend state)
│   └── fleet.rs                  # [NEW] FleetReconciler (pattern analysis, recommendations)
│
├── registry/
│   ├── backend.rs                # [MODIFIED] Extend BackendStatus with lifecycle operation tracking
│   └── mod.rs                    # [MODIFIED] Add lifecycle operation state queries
│
├── config/
│   └── mod.rs                    # [MODIFIED] Add lifecycle config (vram_headroom, timeouts, sample thresholds)
│
└── health/
    └── mod.rs                    # [MODIFIED] Integrate HealthStatus::Loading into health checker

tests/
├── contract/
│   └── lifecycle_api.rs          # [NEW] Contract tests for load/unload/recommendations endpoints
├── integration/
│   ├── ollama_lifecycle.rs       # [NEW] Integration tests for OllamaAgent lifecycle methods
│   ├── lifecycle_reconciler.rs   # [NEW] LifecycleReconciler tests with Loading state scenarios
│   └── fleet_intelligence.rs     # [NEW] FleetReconciler tests with simulated request patterns
└── unit/
    └── resource_usage.rs         # [NEW] Unit tests for VRAM validation logic
```

**Structure Decision**: 

This feature extends the existing single Rust workspace (nexus-orchestrator) without adding new projects or abstractions. We leverage:

1. **Existing InferenceAgent trait** in `src/agent/mod.rs` - lifecycle methods are stubbed as Unsupported, we activate them in OllamaAgent
2. **Existing Reconciler pipeline** in `src/routing/reconciler/` - we add two new reconcilers (lifecycle, fleet) to the chain
3. **Existing BackendRegistry** in `src/registry/` - we extend BackendStatus to track operation state
4. **Existing API framework** in `src/api/` - we add one new module (lifecycle.rs) for the three endpoints
5. **Existing config system** - we add lifecycle-specific settings to the existing TOML structure

No new crates, no workspace restructuring, no abstraction layers. This aligns with Constitution Principle II (Single Binary) and Anti-Abstraction Gate requirements. All tests follow existing conventions (`tests/contract/`, `tests/integration/`, unit tests in `mod tests` blocks).

## Complexity Tracking

> **No violations - section intentionally empty**

All Constitution Gates passed without justification required. The feature integrates cleanly into existing architecture:

- Uses ≤3 new modules (lifecycle.rs, fleet.rs in reconciler, lifecycle.rs in api)
- Extends existing traits and structs (InferenceAgent, BackendStatus, ReconcilerPipeline)
- No new abstraction layers or frameworks
- Maintains <1ms routing latency and <10KB per-backend memory targets

If complexity violations emerge during implementation, document them here with justification.

---

## Phase 0: Research & Design Decisions

**Objective**: Resolve all NEEDS CLARIFICATION items from Technical Context and document technology choices with rationale.

### Research Tasks

1. **Request History Storage Approach** (Technical Context line 42)
   - **Unknown**: How to store and query time-series request patterns for FleetReconciler
   - **Options to evaluate**:
     - In-memory circular buffer with hourly aggregates (simplest, ephemeral)
     - DashMap with timestamp-keyed buckets (bounded memory, no external deps)
     - Metrics crate with histogram/counter exports (reuse existing observability)
     - External time-series DB (InfluxDB, Prometheus remote write) - adds complexity
   - **Research questions**:
     - What query patterns does FleetReconciler need? (time-of-day, model popularity, 7-day sliding window)
     - Can we aggregate to hourly buckets to cap memory? (14M req/day → 24 buckets/day × 30 days = 720 buckets)
     - Do we need persistence across restarts? (spec implies no - advisory recommendations only)
   - **Decision criteria**: Zero external dependencies > Memory bounded < 50MB > Query latency < 5s

2. **Ollama API Lifecycle Contract Validation**
   - **Unknown**: Do Ollama /api/pull, /api/ps, keepalive=0 APIs behave as spec assumes?
   - **Validation needed**:
     - Does /api/pull support progress tracking (percent, eta_ms)? Or only binary complete/incomplete?
     - Does /api/ps accurately report VRAM usage in real-time? Format of response?
     - Does keepalive=0 immediately unload models? Or graceful drain with timeout?
     - Can we detect concurrent load attempts and reject with 409?
   - **Approach**: Prototype against real Ollama 0.1.29+ instance, document actual API behavior
   - **Risk mitigation**: If progress tracking unavailable, fall back to polling /api/ps for model appearance

3. **Pattern Detection Algorithm Design**
   - **Unknown**: What algorithm balances simplicity vs. accuracy for demand prediction?
   - **Requirements** (from spec FR-013 to FR-024):
     - Detect time-of-day patterns (hourly, daily, weekly periodicity)
     - Track model popularity trends (request frequency over 7-30 day windows)
     - Generate confidence scores (0.0-1.0) based on pattern strength
     - Minimum sample size: 7 days history, 100+ requests per model
   - **Options**:
     - Simple moving average + threshold detection (e.g., if avg(9am requests) > 2× avg(rest of day) → spike detected)
     - Time-series decomposition (trend + seasonality, requires stats library)
     - Percentile-based anomaly detection (P95 request rate as baseline)
   - **Decision criteria**: No external ML libraries > Explainable recommendations > 5s computation time

4. **VRAM Estimation Strategy**
   - **Unknown**: How to estimate model VRAM requirements before load operation?
   - **Challenges**:
     - Model size varies by quantization (4-bit, 8-bit, fp16)
     - Context window affects KV cache size
     - Ollama doesn't expose VRAM requirements via API
   - **Options**:
     - Hardcoded lookup table (llama3-8b: 8GB, llama3-70b: 70GB) - brittle
     - Parse model name for parameter count (8b, 70b suffix) + assume 1 byte/param
     - Query /api/show {model} for model metadata (if available)
     - Add 10% buffer to estimates (per risk analysis in spec)
   - **Decision criteria**: Works for 80% of Ollama models > Fails safely (rejects load vs crashes)

5. **Lifecycle Operation Timeout Strategy**
   - **Unknown**: How to detect and recover from hung model load operations?
   - **Scenarios**:
     - Network stall during model pull (Ollama backend unresponsive)
     - Ollama process freeze mid-load
     - Disk I/O bottleneck causing indefinite loading
   - **Options**:
     - Fixed timeout per operation (e.g., 5 minutes for any model)
     - Model-size-based timeout (30s for <5GB, 5min for >50GB)
     - Health check failure triggers auto-abort (reuse existing health checker)
   - **Decision criteria**: Simple to configure > Prevents zombie Loading state > Allows operator override

### Output Artifact

Generate `research.md` with structure:

```markdown
# Research: Fleet Intelligence and Model Lifecycle

## Decision: Request History Storage
**Chosen**: [approach]
**Rationale**: [why]
**Alternatives Considered**: [what else, why rejected]
**Implementation Notes**: [key constraints, memory estimates]

## Decision: Ollama API Behavior
**Validated**: [/api/pull, /api/ps, keepalive=0 contracts]
**Progress Tracking**: [yes/no, format]
**VRAM Reporting**: [format, accuracy, staleness]
**Edge Cases**: [concurrent loads, failures, timeouts]

## Decision: Pattern Detection Algorithm
**Chosen**: [algorithm]
**Rationale**: [simplicity vs accuracy tradeoff]
**Confidence Score Calculation**: [formula]
**Sample Size Thresholds**: [7 days, 100 requests - justified]

## Decision: VRAM Estimation Strategy
**Chosen**: [approach]
**Rationale**: [coverage, failure mode]
**Buffer Strategy**: [10% safety margin]
**Fallback Behavior**: [what happens on estimation failure]

## Decision: Lifecycle Operation Timeouts
**Chosen**: [timeout strategy]
**Rationale**: [detection vs recovery tradeoff]
**Configuration**: [TOML settings, defaults]
**Abort Mechanism**: [how to cancel hung operations]
```

---

## Phase 1: Data Model & Contracts

**Objective**: Define entities, relationships, state machines, API contracts, and integration guide.

**Prerequisites**: `research.md` complete (all NEEDS CLARIFICATION resolved)

### 1. Data Model (`data-model.md`)

Extract entities from spec Key Entities section and extend with implementation details:

#### Core Entities

**LifecycleOperation** (from spec lines 140-141)
```rust
struct LifecycleOperation {
    operation_id: Uuid,              // Unique identifier for tracking
    operation_type: OperationType,   // Load | Unload | Migrate
    model_id: String,                // Model being operated on
    source_backend_id: Option<Uuid>, // For migration/unload
    target_backend_id: Uuid,         // For load/migration
    status: OperationStatus,         // Pending | InProgress | Completed | Failed
    progress_percent: u8,            // 0-100 for tracking
    eta_ms: Option<u64>,             // Estimated completion time
    initiated_at: DateTime<Utc>,     // Start timestamp
    completed_at: Option<DateTime<Utc>>, // End timestamp
    error_details: Option<String>,   // Failure reason if status=Failed
}

enum OperationType {
    Load,    // POST /v1/models/load
    Unload,  // DELETE /v1/models/{id}
    Migrate, // Coordinated unload + load
}

enum OperationStatus {
    Pending,     // Queued, not started
    InProgress,  // Currently executing
    Completed,   // Successfully finished
    Failed,      // Error occurred (see error_details)
}
```

**LoadingState** (from spec lines 143-144)
```rust
struct LoadingState {
    model_id: String,
    percent_complete: u8,
    estimated_completion_ms: Option<u64>,
    started_at: DateTime<Utc>,
    backend_id: Uuid,
}

// Maps to HealthStatus::Loading variant (already defined in types.rs)
// HealthStatus::Loading { model_id, percent, eta_ms }
```

**ResourceSnapshot** (from spec lines 146-147)
```rust
struct ResourceSnapshot {
    backend_id: Uuid,
    vram_used_bytes: u64,
    vram_total_bytes: u64,
    vram_free_bytes: u64,         // Computed: total - used
    loaded_models: Vec<String>,   // From /api/ps response
    pending_requests: u32,        // From backend queue depth
    timestamp: DateTime<Utc>,
}

// Extends existing ResourceUsage struct in types.rs
// ResourceUsage { vram_used_bytes, vram_total_bytes, pending_requests, avg_latency_ms, loaded_models }
```

**PrewarmingRecommendation** (from spec lines 149-150)
```rust
struct PrewarmingRecommendation {
    recommendation_id: Uuid,
    model_id: String,
    target_backend_ids: Vec<Uuid>,
    confidence_score: f32,         // 0.0-1.0 based on pattern strength
    reasoning: String,             // Human-readable explanation
    vram_required_bytes: u64,      // Estimated model VRAM footprint
    generated_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,     // Recommendations are time-bounded
    status: RecommendationStatus,
}

enum RecommendationStatus {
    Pending,   // Generated, awaiting operator review
    Approved,  // Operator accepted (future: policy engine)
    Rejected,  // Operator declined
    Executed,  // Load operation initiated
}
```

**RequestPattern** (from spec lines 152-153)
```rust
struct RequestPattern {
    model_id: String,
    time_window: TimeWindow,       // Hourly | Daily | Weekly
    request_count: u64,
    avg_latency_ms: u32,
    peak_hour: u8,                 // 0-23 for hourly patterns
    trend_direction: TrendDirection,
}

enum TimeWindow {
    Hourly,  // Last 24 hours
    Daily,   // Last 7 days
    Weekly,  // Last 4 weeks
}

enum TrendDirection {
    Increasing,  // Request rate growing
    Stable,      // Consistent load
    Decreasing,  // Declining usage
}
```

#### State Machines

**LifecycleOperation State Transitions**:
```
Pending → InProgress → Completed
              ↓
            Failed

Rules:
- Pending: Operation queued, VRAM validation passed
- InProgress: Ollama API call initiated (/api/pull or keepalive=0)
- Completed: Backend health check confirms model loaded/unloaded
- Failed: Timeout, VRAM exhaustion, network error, or backend unhealthy
- No retries: Failed operations remain Failed, operator must retry manually
```

**Backend HealthStatus with Loading**:
```
Healthy → Loading → Healthy
  ↓         ↓         ↓
Unhealthy ← ← ← ← Unhealthy

Rules:
- Healthy → Loading: load_model() called
- Loading → Healthy: Model pull completes, /api/ps shows model loaded
- Loading → Unhealthy: Timeout (5min), health check failure, or /api/pull error
- Loading backends MUST NOT receive inference requests (LifecycleReconciler blocks)
- Draining state (existing) is separate from Loading (both block new requests)
```

#### Relationships

- **Backend 1:N LifecycleOperation**: Each backend can have multiple operations (historical), but only 1 InProgress at a time
- **Backend 1:1 LoadingState**: Backends in HealthStatus::Loading have exactly one LoadingState
- **Backend 1:1 ResourceSnapshot**: Each backend has one current snapshot (updated every health check)
- **Model 1:N PrewarmingRecommendation**: A model can have multiple recommendations targeting different backends
- **Model 1:N RequestPattern**: Each model has patterns for hourly, daily, weekly windows

### 2. API Contracts (`contracts/`)

Generate OpenAPI 3.0 YAML specs for new endpoints:

#### `contracts/load-model.yaml`
```yaml
paths:
  /v1/models/load:
    post:
      summary: Load a model onto a specific backend
      operationId: loadModel
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              required: [model_id, backend_id]
              properties:
                model_id:
                  type: string
                  description: Model identifier (e.g., "llama3-8b")
                  example: "llama3-8b"
                backend_id:
                  type: string
                  format: uuid
                  description: Target backend UUID
      responses:
        '202':
          description: Load operation accepted and initiated
          content:
            application/json:
              schema:
                type: object
                properties:
                  operation_id:
                    type: string
                    format: uuid
                  status:
                    type: string
                    enum: [InProgress]
                  progress_percent:
                    type: integer
                    minimum: 0
                    maximum: 100
                  eta_ms:
                    type: integer
                    nullable: true
        '400':
          description: Insufficient VRAM or invalid request
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Error'
        '409':
          description: Backend already loading a model
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Error'
        '507':
          description: Insufficient storage (VRAM capacity)
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Error'
```

#### `contracts/unload-model.yaml`
```yaml
paths:
  /v1/models/{model_id}:
    delete:
      summary: Unload a model from a specific backend
      operationId: unloadModel
      parameters:
        - name: model_id
          in: path
          required: true
          schema:
            type: string
        - name: backend_id
          in: query
          required: true
          schema:
            type: string
            format: uuid
      responses:
        '200':
          description: Model unloaded successfully
          content:
            application/json:
              schema:
                type: object
                properties:
                  operation_id:
                    type: string
                    format: uuid
                  status:
                    type: string
                    enum: [Completed]
                  vram_freed_bytes:
                    type: integer
                    format: int64
        '409':
          description: Model has active requests, cannot unload
          content:
            application/json:
              schema:
                type: object
                properties:
                  error:
                    type: object
                    properties:
                      message:
                        type: string
                      active_requests:
                        type: integer
```

#### `contracts/fleet-recommendations.yaml`
```yaml
paths:
  /v1/fleet/recommendations:
    get:
      summary: Get fleet intelligence pre-warming recommendations
      operationId: getRecommendations
      parameters:
        - name: min_confidence
          in: query
          schema:
            type: number
            format: float
            minimum: 0.0
            maximum: 1.0
            default: 0.8
      responses:
        '200':
          description: List of pre-warming recommendations
          content:
            application/json:
              schema:
                type: object
                properties:
                  recommendations:
                    type: array
                    items:
                      type: object
                      properties:
                        recommendation_id:
                          type: string
                          format: uuid
                        model_id:
                          type: string
                        target_backend_ids:
                          type: array
                          items:
                            type: string
                            format: uuid
                        confidence_score:
                          type: number
                          format: float
                        reasoning:
                          type: string
                        vram_required_bytes:
                          type: integer
                          format: int64
                        generated_at:
                          type: string
                          format: date-time
```

### 3. Integration Guide (`quickstart.md`)

Create operator-focused quickstart:

```markdown
# Fleet Intelligence and Model Lifecycle - Quickstart

## Prerequisites
- Nexus v0.5+ running with Ollama backend
- Ollama 0.1.29+ with at least one GPU backend
- Sufficient VRAM capacity (check with `GET /v1/fleet/recommendations`)

## Manual Model Placement (P1)

### Load a model
curl -X POST http://localhost:8000/v1/models/load \
  -H "Content-Type: application/json" \
  -d '{
    "model_id": "llama3-8b",
    "backend_id": "550e8400-e29b-41d4-a716-446655440000"
  }'

Response (202 Accepted):
{
  "operation_id": "123e4567-e89b-12d3-a456-426614174000",
  "status": "InProgress",
  "progress_percent": 0,
  "eta_ms": 120000
}

### Check load progress
curl http://localhost:8000/v1/models

(Look for backend with HealthStatus::Loading in X-Nexus-Backend-Status header)

### Verify model is loaded
curl http://localhost:8000/v1/models
# Should see llama3-8b in response with target backend

## Model Migration (P2)

# 1. Load model on target backend (backend B)
curl -X POST http://localhost:8000/v1/models/load \
  -d '{"model_id": "llama3-8b", "backend_id": "<backend-B-uuid>"}'

# 2. Wait for load to complete (poll /v1/models)

# 3. Unload from source backend (backend A)
curl -X DELETE "http://localhost:8000/v1/models/llama3-8b?backend_id=<backend-A-uuid>"

(Traffic automatically shifts to backend B)

## Model Unloading (P3)

curl -X DELETE "http://localhost:8000/v1/models/llama3-8b?backend_id=<backend-uuid>"

Error cases:
- 409 Conflict: Model has active requests (wait and retry)
- 404 Not Found: Model not loaded on specified backend

## Fleet Intelligence (P4)

### Get pre-warming recommendations
curl "http://localhost:8000/v1/fleet/recommendations?min_confidence=0.8"

Response:
{
  "recommendations": [
    {
      "recommendation_id": "...",
      "model_id": "llama3-8b",
      "target_backend_ids": ["backend-uuid-1", "backend-uuid-2"],
      "confidence_score": 0.92,
      "reasoning": "llama3-8b requests spike every weekday 9am-11am (pattern detected over 14 days)",
      "vram_required_bytes": 8589934592,
      "generated_at": "2025-01-19T08:30:00Z"
    }
  ]
}

### Act on recommendations
(Manual approval for now, automated policies in future phase)

curl -X POST http://localhost:8000/v1/models/load \
  -d '{"model_id": "llama3-8b", "backend_id": "<recommended-backend-uuid>"}'
```

### 4. Agent Context Update

Run agent context script:
```bash
.specify/scripts/bash/update-agent-context.sh copilot
```

Add new technology/patterns to `.github/copilot-instructions.md` between markers:
- Lifecycle operation state tracking (DashMap with LifecycleOperation)
- Request pattern aggregation (hourly buckets, 30-day retention)
- VRAM validation with headroom budgets (80% max utilization default)
- Reconciler pipeline extension (LifecycleReconciler for state checks)

---

## Phase 2: Task Generation (NOT part of /speckit.plan)

**Objective**: This phase is handled by `/speckit.tasks` command, which generates `tasks.md`.

The planning command (`/speckit.plan`) stops here. Tasks are generated separately after plan review.

**Expected task breakdown preview** (for context only):

1. **Foundation Tasks** (dependencies for all features)
   - T001: Extend LifecycleConfig in `src/config/mod.rs` with VRAM thresholds, timeouts, sample sizes
   - T002: Add LifecycleOperation and PrewarmingRecommendation types to `src/agent/types.rs`
   - T003: Extend BackendStatus in `src/registry/backend.rs` to track lifecycle operation state

2. **OllamaAgent Implementation** (P1: Manual Load)
   - T004: Implement OllamaAgent.load_model() calling POST /api/pull with progress tracking
   - T005: Implement OllamaAgent.resource_usage() calling GET /api/ps for VRAM metrics
   - T006: Add VRAM validation logic before load operations (check headroom budget)
   - T007: Integration tests for load_model with wiremock Ollama responses

3. **LifecycleReconciler** (P1: Routing Integration)
   - T008: Create LifecycleReconciler implementing Reconciler trait
   - T009: Block routing to backends in HealthStatus::Loading state
   - T010: Integrate LifecycleReconciler into ReconcilerPipeline (after Scheduler)
   - T011: Unit tests for Loading state blocking with candidate exclusion

4. **Lifecycle API Endpoints** (P1: Operator Interface)
   - T012: Create `src/api/lifecycle.rs` with POST /v1/models/load handler
   - T013: Implement DELETE /v1/models/{id} handler with active request check
   - T014: Add OpenAPI error responses (400, 409, 507) with actionable context
   - T015: Contract tests for load/unload endpoints with wiremock

5. **Health Check Integration** (P1: Progress Tracking)
   - T016: Modify health checker to map Ollama /api/ps to HealthStatus::Loading
   - T017: Detect load completion and transition Loading → Healthy
   - T018: Detect timeout/failure and transition Loading → Unhealthy
   - T019: Integration tests for health state transitions

6. **Model Unloading** (P3: VRAM Reclamation)
   - T020: Implement OllamaAgent.unload_model() calling keepalive=0
   - T021: Add active request detection (query metrics for pending_requests > 0)
   - T022: Return 409 Conflict with active_requests count if unload blocked
   - T023: Integration tests for graceful unload and rejection scenarios

7. **Request Pattern Tracking** (P4: Data Collection)
   - T024: Implement request history storage (approach from research.md decision)
   - T025: Add instrumentation to routing pipeline to record model requests with timestamps
   - T026: Create hourly aggregation background task (buckets per model per hour)
   - T027: Unit tests for memory-bounded history retention (30-day rolling window)

8. **FleetReconciler** (P4: Pattern Analysis)
   - T028: Create FleetReconciler with pattern detection algorithm (from research.md)
   - T029: Implement time-of-day spike detection (weekday 9am-11am pattern example)
   - T030: Implement model popularity trend analysis (7-30 day windows)
   - T031: Calculate confidence scores based on pattern strength and sample size
   - T032: Property-based tests for pattern detection with proptest

9. **Fleet Intelligence API** (P4: Recommendations)
   - T033: Add GET /v1/fleet/recommendations endpoint returning PrewarmingRecommendation list
   - T034: Filter recommendations by min_confidence query parameter (default 0.8)
   - T035: Validate VRAM headroom before including backends in recommendations
   - T036: Never recommend unloading hot models (active request check)
   - T037: Contract tests for recommendation response format

10. **Configuration & Documentation** (Cross-cutting)
    - T038: Add [lifecycle] section to nexus.example.toml with defaults
    - T039: Update docs/FEATURES.md with lifecycle management section
    - T040: Update README.md with lifecycle API examples
    - T041: Add tracing instrumentation for all lifecycle operations (operation_id, model_id, backend_id)

11. **Performance Validation** (Constitution Requirements)
    - T042: Benchmark routing latency with LifecycleReconciler in pipeline (target <1ms)
    - T043: Measure memory overhead per backend (target <10KB)
    - T044: Profile FleetReconciler analysis cycle (target <5s)
    - T045: Verify total memory increase <50MB under load

12. **Migration Support** (P2: Coordinated Operations)
    - T046: Add migration coordination logic (unload + load sequencing)
    - T047: Ensure source backend stays active during target backend load
    - T048: Verify routing shift only after target backend transitions to Healthy
    - T049: Integration tests for migration without request drops

**Total Estimated Tasks**: 49 tasks across 12 categories

Each task will have:
- Acceptance criteria from spec success criteria (SC-001 to SC-012)
- Test requirements (TDD: tests written and failing before implementation)
- Dependencies on prior tasks
- Estimated complexity (S/M/L)

---

## Final Checklist

- [x] Summary extracted from spec with technical approach
- [x] Technical Context filled (all NEEDS CLARIFICATION identified for Phase 0)
- [x] Constitution Check completed (all gates passed, no violations)
- [x] Project Structure documented (files to create/modify)
- [x] Complexity Tracking section (empty - no violations)
- [x] Phase 0 research tasks defined with decision criteria
- [x] Phase 1 data model entities documented from spec
- [x] Phase 1 API contracts outlined (OpenAPI schemas)
- [x] Phase 1 integration guide (quickstart.md structure)
- [x] Phase 2 preview provided (task generation is separate `/speckit.tasks` command)

**Next Steps**:
1. Run `/speckit.plan` to execute Phase 0 (generate research.md)
2. Review research.md, approve design decisions
3. `/speckit.plan` continues to Phase 1 (generate data-model.md, contracts/, quickstart.md)
4. Review Phase 1 artifacts, re-check Constitution Gates
5. Run `/speckit.tasks` to generate tasks.md (Phase 2, separate command)
6. Begin implementation with `/speckit.implement` (Phase 3, separate command)

**Branch**: `022-fleet-intelligence-model-lifecycle` (already created by setup-plan.sh)
**Spec**: `/home/lhnascimento/Projects/nexus/specs/022-fleet-intelligence-model-lifecycle/spec.md`
**Plan**: `/home/lhnascimento/Projects/nexus/specs/022-fleet-intelligence-model-lifecycle/plan.md` (this file)
