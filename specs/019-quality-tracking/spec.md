# Feature Specification: Quality Tracking & Backend Profiling

**Feature Branch**: `019-quality-tracking`  
**Created**: 2025-01-24  
**Status**: Draft  
**Input**: User description: "Build performance profiles for each model+backend combination using rolling window statistics. Profiles feed into the router scoring algorithm."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Automatic Quality-Based Routing (Priority: P1)

As a Nexus user making API requests, I want the system to automatically route my requests away from degraded backends without any manual intervention, so that I experience consistent performance and reliability.

**Why this priority**: Core value proposition - automatic reliability without user action. This is the minimum viable feature that delivers immediate value.

**Independent Test**: Can be fully tested by simulating backend failures and verifying requests are automatically routed to healthy backends. Delivers immediate value by improving request success rates.

**Acceptance Scenarios**:

1. **Given** multiple backends are available for a model, **When** one backend starts returning errors at 15% rate over the past hour, **Then** the router automatically deprioritizes that backend for new requests
2. **Given** a backend has been excluded due to high error rate, **When** its error rate drops below the threshold, **Then** the backend is automatically re-included in routing decisions
3. **Given** a backend has no request history, **When** routing decisions are made, **Then** the backend is included with neutral quality score (safe default)

---

### User Story 2 - Performance-Aware Request Distribution (Priority: P2)

As a Nexus user, I want my requests to be routed to backends with lower time-to-first-token (TTFT), so that I receive faster responses and better user experience.

**Why this priority**: Enhances user experience beyond basic reliability. Requires P1 infrastructure but adds optimization layer.

**Independent Test**: Can be tested by simulating backends with different TTFT profiles and verifying faster backends receive priority. Delivers measurable performance improvements.

**Acceptance Scenarios**:

1. **Given** multiple backends for a model with different TTFT metrics, **When** routing a new request, **Then** backends with lower TTFT receive higher quality scores
2. **Given** a backend's TTFT exceeds the penalty threshold, **When** calculating routing scores, **Then** the backend receives a scoring penalty proportional to its TTFT
3. **Given** TTFT metrics are being tracked, **When** viewing /v1/stats, **Then** each backend displays its average TTFT in milliseconds

---

### User Story 3 - Quality Metrics Observability (Priority: P3)

As a Nexus operator, I want to monitor quality metrics (error rates, success rates, TTFT) through Prometheus and API endpoints, so that I can understand system behavior and make informed operational decisions.

**Why this priority**: Operational visibility that supports P1 and P2. Essential for debugging and tuning but not required for basic functionality.

**Independent Test**: Can be tested by verifying metrics are exposed correctly in Prometheus and /v1/stats endpoints. Delivers operational insights without affecting routing behavior.

**Acceptance Scenarios**:

1. **Given** quality metrics are being tracked, **When** scraping Prometheus metrics, **Then** gauges for error_rate, success_rate_24h, and ttft_seconds are available per backend
2. **Given** quality tracking is active, **When** calling /v1/stats endpoint, **Then** each backend displays error_rate_1h, avg_ttft_ms, and success_rate_24h
3. **Given** a backend has processed requests in the last hour, **When** viewing metrics, **Then** request_count_1h reflects the actual number of requests processed

---

### User Story 4 - Configurable Quality Thresholds (Priority: P3)

As a Nexus operator, I want to configure quality thresholds (error rate limits, TTFT penalties, reconciliation intervals) via TOML configuration, so that I can tune the system behavior to match my specific requirements and service level objectives.

**Why this priority**: Operational flexibility that enhances P1-P3. Provides customization without being required for basic functionality.

**Independent Test**: Can be tested by modifying TOML configuration values and verifying the system applies new thresholds. Delivers deployment flexibility.

**Acceptance Scenarios**:

1. **Given** the TOML config specifies error_rate_threshold = 0.10, **When** a backend's error rate reaches 10%, **Then** the backend is excluded from routing
2. **Given** the TOML config specifies ttft_penalty_threshold_ms = 5000, **When** a backend's TTFT exceeds 5 seconds, **Then** the backend receives a scoring penalty
3. **Given** the TOML config specifies reconciliation_interval_secs = 30, **When** the quality reconciliation loop runs, **Then** metrics are recomputed every 30 seconds

---

### Edge Cases

- What happens when a backend has insufficient data (< 10 requests in 1 hour)? The system should use safe defaults (neutral quality score) rather than excluding the backend.
- What happens when ALL backends for a model exceed the error rate threshold? The system should include all backends with penalty scores rather than failing all requests.
- What happens during the initial 1-hour window after system startup when no metrics exist? Backends should operate with neutral quality scores until sufficient data is collected.
- What happens when request history storage is full or unavailable? The quality reconciliation loop should log warnings and continue using last known metrics.
- What happens if the reconciliation loop crashes? The system should continue routing with last computed metrics and restart the loop automatically.
- What happens when clocks are skewed on distributed instances? Timestamps should use monotonic time sources (Instant) for relative measurements, not wall-clock time.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST track rolling window statistics for 1-hour and 24-hour periods per model+backend combination
- **FR-002**: System MUST compute error_rate_1h as the ratio of failed requests to total requests in the last hour
- **FR-003**: System MUST compute avg_ttft_ms as the average time-to-first-token in milliseconds over the last hour
- **FR-004**: System MUST compute success_rate_24h as the ratio of successful requests to total requests in the last 24 hours
- **FR-005**: System MUST track request_count_1h as the total number of requests processed in the last hour
- **FR-006**: System MUST store last_failure_ts as the timestamp of the most recent request failure
- **FR-007**: QualityReconciler MUST exclude backends from routing when error_rate_1h exceeds the configured threshold
- **FR-008**: SchedulerReconciler MUST apply TTFT penalties to backends when avg_ttft_ms exceeds the configured threshold
- **FR-009**: System MUST run quality_reconciliation_loop in the background to recompute metrics at configurable intervals (default 30 seconds)
- **FR-010**: System MUST read quality metrics from QualityMetricsStore during routing decisions
- **FR-011**: System MUST expose quality metrics via Prometheus gauges: nexus_agent_error_rate, nexus_agent_success_rate_24h, nexus_agent_ttft_seconds
- **FR-012**: System MUST include quality metrics in /v1/stats endpoint response with per-backend granularity
- **FR-013**: System MUST allow operators to configure error_rate_threshold, ttft_penalty_threshold_ms, and reconciliation_interval_secs via TOML configuration
- **FR-014**: System MUST assign neutral quality scores to backends with no request history (safe defaults)
- **FR-015**: System MUST update Prometheus gauges during each reconciliation cycle
- **FR-016**: System MUST maintain thread-safe access to quality metrics across concurrent routing operations
- **FR-017**: System MUST persist quality profiles across reconciliation cycles but NOT across system restarts (in-memory only)
- **FR-018**: System MUST compute metrics from complete request history including both successful and failed requests
- **FR-019**: System MUST handle concurrent metric reads during routing without blocking the reconciliation loop
- **FR-020**: System MUST gracefully handle missing or incomplete request history data

### Key Entities

- **AgentQualityMetrics**: Represents the quality profile for a single model+backend agent. Contains error_rate_1h (f32 between 0.0 and 1.0), avg_ttft_ms (u32 milliseconds), success_rate_24h (f32 between 0.0 and 1.0), last_failure_ts (optional timestamp), and request_count_1h (u32 count). Used by reconcilers to make routing and scheduling decisions.

- **QualityMetricsStore**: Central storage for all agent quality metrics. Provides thread-safe read/write access to metrics. Maps (model, backend) tuples to AgentQualityMetrics. Updated by quality_reconciliation_loop, read by QualityReconciler and SchedulerReconciler.

- **QualityConfig**: Configuration for quality tracking behavior. Defines error_rate_threshold (threshold for excluding agents), ttft_penalty_threshold_ms (threshold for applying TTFT penalties), reconciliation_interval_secs (how often to recompute metrics). Loaded from TOML configuration file.

- **RequestHistoryEntry**: Individual request record used for computing rolling window statistics. Contains timestamps, success/failure status, TTFT measurement, model identifier, and backend identifier. Stored in request history system and read by quality reconciliation loop.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: When a backend error rate exceeds the configured threshold, requests are rerouted to healthy backends within one reconciliation interval (default 30 seconds)
- **SC-002**: Backends with TTFT below the penalty threshold receive prioritized routing, measurable by comparing request distribution before and after TTFT tracking
- **SC-003**: Quality metrics (error_rate_1h, avg_ttft_ms, success_rate_24h) are computed and updated every reconciliation interval for all active backends
- **SC-004**: Prometheus metrics endpoint exposes current quality gauges that match the values in QualityMetricsStore (verifiable by comparing /metrics and /v1/stats outputs)
- **SC-005**: System maintains consistent quality tracking across 10,000+ requests per hour without measurable routing latency impact (< 1ms overhead per request)
- **SC-006**: Backends with no request history receive neutral quality scores and are included in routing (measurable by testing with newly registered backends)
- **SC-007**: Configuration changes to quality thresholds take effect within one reconciliation interval without requiring system restart
- **SC-008**: Quality reconciliation loop continues operating during network partitions or temporary backend unavailability without crashing

## Constitution Alignment *(optional - include when relevant)*

This feature aligns with the following project constitution principles:

### Principle X: Precise Measurement
Quality Tracking implements continuous measurement of real backend performance metrics rather than relying on assumptions or static configurations. By tracking error rates, TTFT, and success rates from actual request history, the system makes routing decisions based on observed behavior rather than theoretical capacity.

### Principle V: Intelligent Routing
The quality profiles feed directly into the router scoring algorithm, enabling data-driven routing decisions. Backends are automatically deprioritized when metrics indicate degradation, and prioritized when metrics indicate superior performance. This creates a feedback loop where routing continuously improves based on real-world observations.

## Assumptions *(optional - include when relevant)*

1. **Request History Availability**: The system assumes request history is maintained in memory and accessible to the quality reconciliation loop. If history is unavailable, metrics default to safe values.

2. **Reconciliation Frequency**: Default 30-second reconciliation interval assumes metrics don't need real-time precision. This trades off freshness for system efficiency.

3. **In-Memory Storage**: Quality metrics are stored in memory only and reset on system restart. This assumes operators rely on Prometheus for long-term metric persistence.

4. **Time Source**: TTFT and timestamps use monotonic time sources (Instant) for relative measurements, assuming system clocks may be unreliable or skewed.

5. **Error Definition**: "Errors" include HTTP 5xx responses, timeouts, and connection failures. HTTP 4xx responses are considered user errors, not backend errors.

6. **Metric Retention Windows**: 1-hour and 24-hour windows are fixed. This assumes these timeframes balance responsiveness (1h) with stability (24h) for most use cases.

7. **Neutral Score Default**: Backends with no history receive neutral scores (neither penalty nor bonus), assuming new backends should be given opportunity to prove performance.

8. **Thread Safety Model**: Quality metrics use concurrent read access with exclusive write access during reconciliation. This assumes read contention is acceptable during routing.

## Dependencies *(optional - include when relevant)*

### Upstream Dependencies
- **Request History System**: Quality reconciliation requires access to historical request records including timestamps, success/failure status, and TTFT measurements. If history is incomplete or unavailable, metrics will be inaccurate or use defaults.

- **Prometheus Integration**: Metrics exposure depends on Prometheus client libraries and gauge registration. System functions without Prometheus but loses observability.

- **Configuration System**: Quality thresholds are loaded from TOML configuration. System must support dynamic config loading or require restart for threshold changes.

### Downstream Impact
- **Router Scoring Algorithm**: Quality metrics directly influence routing decisions. Changes to metric calculations or thresholds will affect request distribution across backends.

- **Scheduler Reconciler**: TTFT penalties affect backend prioritization in scheduling. High-TTFT backends will receive fewer requests, potentially creating cascading capacity effects.

- **/v1/stats Endpoint**: API endpoint must display quality metrics. Changes to metric structure require corresponding API response updates.

## Open Questions *(optional - include when relevant)*

None - all critical aspects are specified based on the provided RFC-001 architecture context.
