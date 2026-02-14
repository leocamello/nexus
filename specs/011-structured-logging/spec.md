# Feature Specification: Structured Request Logging

**Feature Branch**: `011-structured-logging`  
**Created**: 2025-02-14  
**Status**: Draft  
**Input**: User description: "Structured, queryable logs for every request passing through Nexus. Every request gets a correlation ID that tracks it through retries and failovers."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Basic Request Logging (Priority: P1)

Platform operators need visibility into every request flowing through Nexus to monitor system health, diagnose issues, and understand usage patterns. Every request must produce a structured log entry with essential metadata (timestamp, request ID, model, backend, status, latency, token counts) that can be easily searched and filtered.

**Why this priority**: This is the foundation of the entire feature. Without basic structured logging, operators are blind to system behavior. This provides immediate operational value by enabling basic monitoring and troubleshooting.

**Independent Test**: Can be fully tested by sending a single request through Nexus and verifying that a structured log entry is emitted with all required fields (timestamp, request_id, model, backend, status, latency_ms, tokens). Delivers immediate value by enabling operators to see what's happening in the system.

**Acceptance Scenarios**:

1. **Given** Nexus is running with logging enabled, **When** a client sends a completion request, **Then** a structured log entry is emitted containing timestamp, unique request_id, model name, selected backend, response status, latency_ms, token counts (prompt and completion), and stream mode indicator
2. **Given** Nexus is configured for JSON log format, **When** a request is processed, **Then** the log entry is valid JSON that can be parsed by standard JSON tools
3. **Given** Nexus is configured for human-readable log format, **When** a request is processed, **Then** the log entry is formatted for easy reading in terminal/console with clear field labels
4. **Given** a request completes successfully, **When** viewing the logs, **Then** all metrics (latency, tokens) reflect actual measured values, not estimates or placeholders

---

### User Story 2 - Request Correlation Across Retries and Failovers (Priority: P1)

When a request fails and Nexus retries it with the same backend or fails over to a different backend, operators need to trace the entire journey of that logical request. All log entries related to the same original request must share a correlation ID, allowing operators to reconstruct the complete chain of attempts.

**Why this priority**: This is critical for understanding system reliability and debugging routing issues. Without correlation, operators cannot distinguish between independent requests and retry attempts, making failure analysis nearly impossible.

**Independent Test**: Can be fully tested by configuring a backend that fails intermittently, sending a request that triggers retry/failover, and verifying all log entries share the same request_id while showing different retry_count and backend values. Delivers value by enabling operators to trace problematic requests through the retry chain.

**Acceptance Scenarios**:

1. **Given** a backend fails and triggers a retry, **When** viewing the logs, **Then** both the failed attempt and retry share the same request_id with different retry_count values (0, 1, 2, etc.)
2. **Given** a request fails over to a different backend after exhausting retries, **When** viewing the logs, **Then** all attempts (original and failovers) share the same request_id with the fallback_chain field showing the progression
3. **Given** multiple concurrent requests are in flight with retries, **When** filtering logs by a specific request_id, **Then** only that request's attempts are returned, clearly showing the chronological sequence
4. **Given** a request succeeds on first try, **When** viewing the log, **Then** retry_count is 0 and fallback_chain is empty or shows only the primary backend

---

### User Story 3 - Routing and Backend Selection Visibility (Priority: P2)

Operators need to understand why Nexus selected a particular backend for each request. The logs must capture the routing decision rationale (e.g., "backend_score", "round_robin", "only_healthy_backend", "fallback_tier_2") so operators can validate routing logic and identify suboptimal selections.

**Why this priority**: This enables validation of the intelligent routing system and helps operators tune routing strategies. While less critical than basic logging and correlation, it's essential for optimizing the system.

**Independent Test**: Can be fully tested by configuring multiple backends with different health/load characteristics, sending requests, and verifying route_reason field explains the selection (e.g., "highest_score:backend1:0.95", "round_robin", "fallback:backend2_unhealthy"). Delivers value by making routing decisions transparent and debuggable.

**Acceptance Scenarios**:

1. **Given** multiple healthy backends exist, **When** the router selects a backend, **Then** the log entry includes a route_reason field explaining why that backend was chosen
2. **Given** primary backend is unhealthy, **When** router selects a fallback, **Then** the log entry shows route_reason indicating fallback scenario and the fallback_chain field lists the fallback progression
3. **Given** different routing strategies (load-based, round-robin, weighted), **When** comparing logs across requests, **Then** route_reason values reflect the configured strategy
4. **Given** a backend was selected due to model alias resolution, **When** viewing logs, **Then** both the requested model alias and resolved actual model are visible

---

### User Story 4 - Privacy-Safe Logging with Debug Override (Priority: P2)

By default, Nexus must never log request or response message content to protect user privacy and comply with data handling policies. However, for local development and debugging, operators need the ability to opt-in to logging request content through explicit configuration.

**Why this priority**: Privacy and security are critical, but this doesn't affect the core logging functionality. It's essential for compliance but can be enforced through defaults and configuration.

**Independent Test**: Can be fully tested by sending requests with message content and verifying default logs contain no message text, then enabling debug content logging and verifying request content appears in logs. Delivers value by ensuring compliance while providing debugging capabilities.

**Acceptance Scenarios**:

1. **Given** default Nexus configuration, **When** processing requests with user messages, **Then** logs contain no message content, prompt text, or response text
2. **Given** debug content logging is explicitly enabled in configuration, **When** processing requests, **Then** logs include request message content with a clear indicator that debug mode is active
3. **Given** debug content logging is enabled, **When** Nexus starts, **Then** a warning is logged indicating sensitive data will be captured
4. **Given** streaming responses, **When** logging is active, **Then** response body content is never logged regardless of configuration (per OpenAI compatibility principle)

---

### User Story 5 - Configurable Log Levels per Component (Priority: P3)

Different components of Nexus may require different log verbosity. Operators need to configure log levels independently for core components (routing, backends, API gateway, health checker) to reduce noise and focus on relevant areas during troubleshooting.

**Why this priority**: This is a quality-of-life improvement that becomes more important as the system scales. It's not critical for initial functionality but improves operational experience.

**Independent Test**: Can be fully tested by setting routing component to DEBUG and API gateway to INFO, sending requests, and verifying routing logs show detailed debug information while API gateway shows only info-level messages. Delivers value by reducing log noise and enabling targeted debugging.

**Acceptance Scenarios**:

1. **Given** routing component is set to DEBUG level, **When** processing requests, **Then** detailed routing decision logs appear including score calculations and backend comparisons
2. **Given** API gateway is set to WARN level, **When** processing successful requests, **Then** no API gateway logs appear (only warnings and errors)
3. **Given** different log levels for different components, **When** viewing aggregated logs, **Then** each log entry clearly identifies its source component
4. **Given** log level configuration is changed, **When** applying new config, **Then** subsequent logs reflect the new levels without restarting Nexus

---

### User Story 6 - Log Aggregator Compatibility (Priority: P3)

Logs must be compatible with common log aggregation and analysis tools (Elasticsearch/ELK, Grafana Loki, Splunk, CloudWatch) so operators can integrate Nexus into existing observability infrastructure. JSON format should include standard fields that aggregators can automatically parse and index.

**Why this priority**: This enables enterprise adoption and integration into existing workflows, but the logs are useful even without aggregator integration. This is an enhancement for production deployments.

**Independent Test**: Can be fully tested by configuring Nexus for JSON output, piping logs to a test Loki or ELK instance, and verifying logs are automatically indexed and searchable without custom parsing. Delivers value by enabling centralized log management.

**Acceptance Scenarios**:

1. **Given** JSON log format is configured, **When** logs are ingested by Elasticsearch, **Then** all fields are automatically indexed and searchable without custom mappings
2. **Given** logs are sent to Grafana Loki, **When** querying by labels (request_id, model, backend), **Then** relevant log entries are returned efficiently
3. **Given** timestamp field in logs, **When** imported to time-series tools, **Then** timestamp is correctly parsed as RFC3339/ISO8601 format
4. **Given** structured fields in JSON logs, **When** building dashboards, **Then** all numeric fields (latency_ms, tokens) are queryable as numbers, not strings

---

### Edge Cases

- What happens when logging system itself fails (disk full, network partition for remote logging)?
  - Nexus should continue processing requests without blocking on logging
  - A separate error log or metric should indicate logging failures
  - Request processing must never fail due to logging errors

- How are extremely long-running streaming requests logged?
  - Initial request log entry is emitted when request starts
  - Completion log entry is emitted when stream completes with total latency
  - Partial tokens/progress are not logged mid-stream to avoid log spam

- What if request_id generation collides (duplicate IDs)?
  - Request IDs should use UUID v4 or similar with negligible collision probability
  - If collision detection is implemented, append a disambiguating suffix

- How are requests logged when no backend is available (all unhealthy)?
  - Log entry is still emitted with status indicating failure
  - backend field shows "none_available" or similar sentinel value
  - route_reason explains why no backend was selected

- What happens with malformed requests that fail before routing?
  - Log entry is still emitted with partial information (timestamp, request_id, status=error)
  - Missing fields (backend, model) are marked as "not_applicable" or null
  - Error details are logged at appropriate level (WARN or ERROR)

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST emit a structured log entry for every request received, regardless of success or failure
- **FR-002**: System MUST assign a unique request_id (correlation ID) to each incoming request that persists across all retry and failover attempts
- **FR-003**: System MUST include the following fields in every log entry: timestamp (RFC3339 format), request_id, model (requested model name), backend (selected backend identifier), backend_type (backend category), status (success/failure/error code)
- **FR-004**: System MUST include performance metrics in every log entry: latency_ms (total request duration in milliseconds), tokens_prompt (input token count), tokens_completion (output token count), stream (boolean indicating streaming mode)
- **FR-005**: System MUST include routing metadata in every log entry: route_reason (explanation of backend selection), retry_count (number of retry attempts, starting at 0), fallback_chain (ordered list of backends attempted)
- **FR-006**: System MUST support both JSON and human-readable log output formats, selectable via configuration
- **FR-007**: System MUST allow independent log level configuration for each component (routing, backends, API gateway, health checker, etc.)
- **FR-008**: System MUST NOT log request message content or response body content by default
- **FR-009**: System MUST provide a configuration option to enable request content logging for debugging purposes, with clear indication that sensitive data will be captured
- **FR-010**: System MUST emit logs in a non-blocking manner such that logging failures do not cause request processing to fail or block
- **FR-011**: System MUST use the existing `tracing` crate infrastructure for log emission and formatting
- **FR-012**: System MUST emit log entries at the INFO level for successful requests and WARN/ERROR levels for failures
- **FR-013**: System MUST include null or sentinel values for fields that are not applicable to a particular request (e.g., backend="none" when all backends are unhealthy)
- **FR-014**: System MUST ensure all timestamp fields are in UTC timezone
- **FR-015**: System MUST ensure numeric fields (latency_ms, tokens_prompt, tokens_completion, retry_count) are logged as numbers, not strings, in JSON format

### Key Entities

- **RequestLogEntry**: Represents a single structured log entry for a request
  - Core identifiers: timestamp, request_id
  - Request metadata: model, backend, backend_type, status, stream
  - Performance metrics: latency_ms, tokens_prompt, tokens_completion
  - Routing metadata: route_reason, retry_count, fallback_chain
  - All fields are accurately measured, never estimated
  - Emitted when request completes (success or failure)

- **LoggingConfig**: Configuration for logging behavior (already exists in codebase)
  - level: overall log level threshold
  - format: output format (JSON or human-readable)
  - component_levels: map of component-specific log levels
  - enable_content_logging: opt-in flag for debug content logging (defaults to false)

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of requests produce at least one structured log entry (verified by comparing request count metrics to log entry count)
- **SC-002**: Request correlation ID successfully tracks requests through retry chains - operators can filter logs by request_id and see complete history of a failed request within 10 seconds of searching
- **SC-003**: Log entries contain accurate measured values - latency_ms matches actual request duration within 1ms, token counts match exact values returned by backends
- **SC-004**: Zero instances of message content appearing in logs when content logging is disabled (verified by auditing production logs)
- **SC-005**: Logs are successfully ingested by standard log aggregators (ELK, Loki) without custom parsing configuration - 95% of JSON fields are automatically indexed
- **SC-006**: Log format switching (JSON vs human-readable) takes effect within 5 seconds of configuration change without service restart
- **SC-007**: Logging system handles 10,000 requests per minute without blocking request processing or introducing more than 1ms of latency overhead per request
- **SC-008**: Component-level log filtering reduces log volume by 60-80% in production (by setting non-critical components to WARN level)
- **SC-009**: Operators successfully diagnose 90% of retry/failover issues using correlation ID to trace request chains without needing additional debugging tools
- **SC-010**: Log entries are queryable by all key fields (model, backend, status, time range) with sub-second query response times in log aggregators for datasets up to 1M entries
