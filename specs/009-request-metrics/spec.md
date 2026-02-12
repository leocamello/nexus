# Feature Specification: Request Metrics (F09)

**Feature Branch**: `009-request-metrics`  
**Created**: 2025-01-10  
**Status**: Draft  
**Input**: User description: "Track request statistics for observability and debugging. Expose metrics in both Prometheus and JSON formats. This is the first feature of v0.2 (Observability) and lays the foundation for F10 (Web Dashboard) and F11 (Structured Logging)."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Basic Request Tracking (Priority: P1)

As a platform operator, I need to see how many requests are being processed, their success/failure rates, and which models and backends are being used, so I can monitor the health and usage patterns of the gateway.

**Why this priority**: Core observability - without request tracking, operators are blind to system behavior. This is the foundation for all other metrics and monitoring capabilities.

**Independent Test**: Can be fully tested by sending requests through the gateway and querying the /metrics and /v1/stats endpoints to verify counters increment correctly. Delivers immediate value by showing request volume and success rates.

**Acceptance Scenarios**:

1. **Given** the gateway is running with configured backends, **When** a successful request is made to a model, **Then** the request counter (nexus_requests_total) increments with correct model, backend, and status labels
2. **Given** the gateway is running, **When** multiple requests are made with different outcomes (success, error, timeout), **Then** each request is tracked separately with appropriate status codes and error types
3. **Given** the gateway has processed requests, **When** I access /metrics endpoint, **Then** I see Prometheus-compatible text format with all request counters
4. **Given** the gateway has processed requests, **When** I access /v1/stats endpoint, **Then** I see JSON formatted statistics with total/success/error counts and uptime

---

### User Story 2 - Performance Monitoring (Priority: P2)

As a platform operator, I need to track request duration and backend latency over time with histogram buckets, so I can identify performance issues, slow backends, and understand response time distributions.

**Why this priority**: Performance visibility is critical but secondary to knowing if the system is working at all. Builds on P1 by adding timing information.

**Independent Test**: Can be tested by sending requests and measuring actual response times, then verifying histogram buckets are populated correctly in /metrics. Delivers value by exposing performance bottlenecks.

**Acceptance Scenarios**:

1. **Given** the gateway is processing requests, **When** requests complete, **Then** request duration is recorded in nexus_request_duration_seconds histogram with appropriate bucket (0.1s, 0.25s, 0.5s, 1s, 2.5s, 5s, 10s, 30s, 60s, 120s, 300s)
2. **Given** backends are being health checked, **When** health checks complete, **Then** backend latency is recorded in nexus_backend_latency_seconds histogram
3. **Given** the gateway has processed requests with varying durations, **When** I query /metrics, **Then** I see histogram data with count, sum, and bucket distributions for each model and backend
4. **Given** the gateway has performance data, **When** I access /v1/stats, **Then** I see average latency per backend and average duration per model in milliseconds

---

### User Story 3 - Routing Intelligence Metrics (Priority: P3)

As a platform operator, I need to track fallback usage, token counts, and backend queue depths, so I can understand routing behavior and optimize backend allocation.

**Why this priority**: Advanced observability for optimization. Useful after basic monitoring (P1) and performance tracking (P2) are in place.

**Independent Test**: Can be tested by triggering fallback scenarios and observing fallback counters increment, and by monitoring pending request gauges during load. Delivers value by showing routing effectiveness.

**Acceptance Scenarios**:

1. **Given** a request fails on the primary model, **When** it falls back to an alternative model, **Then** the fallback counter (nexus_fallbacks_total) increments with from_model and to_model labels
2. **Given** the gateway is processing requests with token usage, **When** requests complete, **Then** token counts are recorded in nexus_tokens_total histogram with prompt and completion type labels
3. **Given** multiple backends are handling requests, **When** requests are queued or in progress, **Then** nexus_pending_requests gauge reflects current queue depth per backend
4. **Given** the backend fleet changes state, **When** backends become healthy or unhealthy, **Then** nexus_backends_healthy and nexus_backends_total gauges update immediately

---

### User Story 4 - Fleet State Visibility (Priority: P3)

As a platform operator, I need to see current fleet state (healthy backends, available models, pending requests), so I can understand system capacity and detect issues in real-time.

**Why this priority**: Real-time state awareness complements historical metrics. Equal priority to P3 routing metrics as both provide operational intelligence.

**Independent Test**: Can be tested by adding/removing backends, marking them healthy/unhealthy, and verifying gauge metrics reflect current state. Delivers value by showing instantaneous system capacity.

**Acceptance Scenarios**:

1. **Given** backends are registered with the gateway, **When** I query /metrics, **Then** nexus_backends_total gauge shows the total count of registered backends
2. **Given** backends have varying health states, **When** I query /metrics, **Then** nexus_backends_healthy gauge shows only healthy backends
3. **Given** models are configured and available, **When** I query /metrics, **Then** nexus_models_available gauge shows the count of distinct models that can be served
4. **Given** the gateway state changes (backends added, health changes), **When** I query /v1/stats, **Then** I see up-to-date per-backend breakdown showing current state

---

### Edge Cases

- What happens when a request is in-flight during a metrics query? (Metrics are captured atomically via lock-free atomic operations; in-flight requests won't corrupt counters)
- How does the system handle metric collection if a request fails before completion? (Error paths still record metrics with error type labels; partial request data is captured)
- What happens to metrics when the gateway restarts? (Metrics reset to zero; no persistent storage is provided - this is a known limitation, operators should scrape regularly)
- How are metrics handled for extremely long-running requests (>5 minutes)? (Histograms have buckets up to 300s/5min; requests longer than that fall into the +Inf bucket)
- What happens if a backend name or model name contains Prometheus-incompatible characters? (Labels are sanitized to ensure Prometheus compatibility, replacing invalid chars with underscores)
- How does the system handle high request volumes (10k+ req/s) without metrics overhead? (Lock-free atomic operations and efficient histogram recording keep overhead <0.1ms per request)

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST expose metrics in Prometheus-compatible text format at GET /metrics endpoint
- **FR-002**: System MUST expose metrics in JSON format at GET /v1/stats endpoint
- **FR-003**: System MUST track total request count with labels for model name, backend name, and HTTP status code (nexus_requests_total counter)
- **FR-004**: System MUST track error count with labels for error type including timeout, backend_error, no_backend, etc. (nexus_errors_total counter)
- **FR-005**: System MUST track fallback events with labels for original model and fallback model (nexus_fallbacks_total counter)
- **FR-006**: System MUST record request duration in histogram buckets [0.1, 0.25, 0.5, 1, 2.5, 5, 10, 30, 60, 120, 300] seconds with labels for model and backend (nexus_request_duration_seconds)
- **FR-007**: System MUST record backend health check latency in histogram with label for backend name (nexus_backend_latency_seconds)
- **FR-008**: System MUST track token counts in histogram with labels for model, backend, and type (prompt/completion) (nexus_tokens_total)
- **FR-009**: System MUST maintain gauge metric for count of healthy backends (nexus_backends_healthy)
- **FR-010**: System MUST maintain gauge metric for total count of backends (nexus_backends_total)
- **FR-011**: System MUST maintain gauge metric for pending request count per backend (nexus_pending_requests)
- **FR-012**: System MUST maintain gauge metric for count of available models (nexus_models_available)
- **FR-013**: JSON stats endpoint MUST include uptime in seconds since gateway start
- **FR-014**: JSON stats endpoint MUST include per-backend breakdown with request count, average latency in milliseconds, and pending request count
- **FR-015**: JSON stats endpoint MUST include per-model breakdown with request count and average duration in milliseconds
- **FR-016**: System MUST record metrics with overhead less than 0.1ms per request (performance requirement)
- **FR-017**: System MUST use thread-safe metric collection with measured overhead < 0.1ms per recording
- **FR-018**: Metrics MUST reset to zero on gateway restart (no persistent storage)
- **FR-019**: System MUST sanitize backend and model names to ensure Prometheus label compatibility
- **FR-020**: System MUST continue serving /v1/* OpenAI-compatible endpoints without interference from new metrics endpoints

### Key Entities

- **Request Metric**: Represents a single request event with model, backend, status code, duration, and token counts
- **Error Metric**: Represents an error event with error type classification (timeout, backend_error, no_backend, etc.)
- **Fallback Metric**: Represents a fallback routing event with source and destination model names
- **Backend State**: Represents current state of a backend including health status, pending request count, and cumulative latency metrics
- **Model State**: Represents aggregated statistics for a model including request count and average duration
- **Fleet State**: Represents overall gateway state including total/healthy backend counts and available model count

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Operators can access Prometheus-compatible metrics at /metrics endpoint within 1 second of gateway startup
- **SC-002**: Operators can access JSON statistics at /v1/stats endpoint within 1 second of gateway startup
- **SC-003**: Request counters increment accurately for 100% of requests (verified by comparing metrics to access logs)
- **SC-004**: Request duration histograms capture timing with accuracy within 5ms of actual measured duration
- **SC-005**: Metrics collection adds less than 0.1ms overhead per request (measured via before/after performance comparison)
- **SC-006**: Error rates by type can be calculated from metrics (e.g., timeout rate = nexus_errors_total{type="timeout"} / nexus_requests_total)
- **SC-007**: Fallback usage rate can be calculated per model (e.g., fallback rate = nexus_fallbacks_total{from_model="X"} / nexus_requests_total{model="X"})
- **SC-008**: Backend health gauge reflects actual backend state within 1 health check interval (typically 10-30 seconds)
- **SC-009**: Per-backend and per-model breakdowns in /v1/stats endpoint contain data for all active backends and models
- **SC-010**: Metrics survive backend registration changes (cumulative counts continue across backend add/remove operations)
- **SC-011**: System handles 10,000+ requests per second without metrics collection causing performance degradation
- **SC-012**: Prometheus scraping at 15-second intervals captures all metric updates without data loss
