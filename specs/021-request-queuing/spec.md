# Feature Specification: Request Queuing & Prioritization

**Feature Branch**: `021-request-queuing`  
**Created**: 2024-02-18  
**Status**: Implemented (Retrospective)  
**Input**: User description: "Request Queuing & Prioritization (F18)"

**Note**: This is a retrospective specification documenting an already-implemented feature.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Graceful Burst Traffic Handling (Priority: P1)

When all backend models are at capacity during traffic spikes, API clients should receive a queued response instead of immediate rejection, allowing the system to absorb temporary bursts without dropping requests.

**Why this priority**: Core value proposition of the feature - prevents 503 errors during peak usage and improves user experience by automatically handling temporary capacity constraints.

**Independent Test**: Can be fully tested by sending requests that exceed backend capacity and verifying they are queued and eventually processed instead of immediately rejected. Delivers immediate value by reducing error rates during traffic spikes.

**Acceptance Scenarios**:

1. **Given** all backend agents are at capacity handling requests, **When** a new chat completion request arrives, **Then** the request is placed in the queue and eventually processed when capacity becomes available
2. **Given** the system is operating normally with available capacity, **When** a chat completion request arrives, **Then** the request bypasses the queue and is immediately routed to an available backend
3. **Given** multiple requests are queued, **When** backend capacity becomes available, **Then** queued requests are dequeued and processed in priority order (high-priority first, then normal)

---

### User Story 2 - Priority-Based Request Handling (Priority: P2)

Critical or time-sensitive API requests should be processed before standard requests when the system is under load, ensuring high-priority workloads receive preferential treatment.

**Why this priority**: Enables different service tiers and ensures critical operations aren't delayed behind bulk/background requests. Important but requires the base queuing mechanism (P1) to exist first.

**Independent Test**: Can be tested independently by submitting mixed-priority requests during high load and verifying high-priority requests are dequeued first. Delivers value for multi-tenant or tiered-service scenarios.

**Acceptance Scenarios**:

1. **Given** multiple requests are in the queue, **When** the drain loop dequeues the next request, **Then** high-priority requests are selected before normal-priority requests
2. **Given** a client sends a chat completion request with `X-Nexus-Priority: high` header, **When** the request needs to be queued, **Then** it is placed in the high-priority queue
3. **Given** a client sends a request without a priority header (or with invalid value), **When** the request needs to be queued, **Then** it defaults to normal priority
4. **Given** a client sends `X-Nexus-Priority: normal`, **When** the request needs to be queued, **Then** it is placed in the normal-priority queue

---

### User Story 3 - Timeout Protection (Priority: P2)

Queued requests should not wait indefinitely. If a request cannot be processed within a reasonable timeframe, clients should receive a clear timeout response with guidance on retry timing.

**Why this priority**: Prevents indefinite hangs and resource leaks. Critical for production reliability but depends on the base queue (P1).

**Independent Test**: Can be tested by queuing requests and ensuring they timeout correctly after the configured max wait time. Delivers value by preventing hung client connections.

**Acceptance Scenarios**:

1. **Given** a request has been in the queue for longer than `max_wait_seconds`, **When** the timeout is reached, **Then** the client receives a 503 Service Unavailable response with `Retry-After` header
2. **Given** a request is dequeued and processed before timeout, **When** processing completes, **Then** the client receives the normal completion response (no timeout error)
3. **Given** the timeout period is configured to 30 seconds, **When** a queued request waits 31 seconds, **Then** it receives a timeout response with `Retry-After` header

---

### User Story 4 - Queue Visibility & Monitoring (Priority: P3)

Operations teams should be able to monitor queue depth and activity through metrics to understand system load and detect capacity issues before they impact users.

**Why this priority**: Operational visibility is important but not required for the feature to function. Can be added after core queueing is working.

**Independent Test**: Can be tested by querying Prometheus metrics endpoint and verifying `nexus_queue_depth` gauge updates correctly. Delivers value for operations and capacity planning.

**Acceptance Scenarios**:

1. **Given** the queue has N pending requests, **When** Prometheus metrics are scraped, **Then** the `nexus_queue_depth` gauge equals N
2. **Given** a request is enqueued, **When** metrics are updated, **Then** `nexus_queue_depth` increments by 1
3. **Given** a request is dequeued, **When** metrics are updated, **Then** `nexus_queue_depth` decrements by 1

---

### User Story 5 - Graceful Shutdown (Priority: P3)

When the Nexus service is shutting down, queued requests should be rejected immediately with actionable error responses rather than hanging or being lost.

**Why this priority**: Important for clean shutdowns and avoiding hung clients during deployments, but not core to the queueing functionality itself.

**Independent Test**: Can be tested by initiating shutdown with requests in queue and verifying all receive 503 responses. Delivers value during rolling deployments and restarts.

**Acceptance Scenarios**:

1. **Given** requests are in the queue, **When** a shutdown signal (CancellationToken) is received, **Then** all queued requests immediately receive 503 responses
2. **Given** shutdown has been initiated, **When** a new request arrives, **Then** it is not enqueued and receives an immediate error response
3. **Given** the drain loop is processing requests, **When** shutdown is signaled, **Then** the current request completes but remaining queued requests are rejected

---

### Edge Cases

- **Full Queue**: What happens when the queue reaches `max_size` and a new request arrives?  
  → Request immediately receives 503 Service Unavailable (no queueing), queue depth remains at max_size

- **Queue Disabled**: How does the system behave when `enabled = false` or `max_size = 0`?  
  → Requests that would be queued immediately receive 503 with no queueing attempt

- **Backend Failure During Drain**: What happens if routing or chat completion fails while processing a queued request?  
  → If not timed out, request is re-enqueued at the back of its priority queue; if timed out, receives 503 timeout response

- **Priority Header Case Sensitivity**: How are different case variations of the priority header handled?  
  → All variations are normalized to lowercase: "HIGH", "high", "HiGh" all map to `Priority::High`; invalid values default to `Priority::Normal`

- **Concurrent Enqueue at Capacity**: What happens when multiple threads try to enqueue simultaneously as the queue approaches max_size?  
  → The `depth` AtomicUsize is checked and incremented atomically; first requests to succeed get queued, subsequent ones see the queue as full and fail

- **Drain Loop Timing**: How frequently are queued requests checked and processed?  
  → The drain loop polls every 50ms, balancing responsiveness with CPU efficiency

- **Oneshot Channel Closure**: What happens if the client connection is dropped before a queued request is processed?  
  → The oneshot sender detects the closed receiver, logs a warning, and moves to the next queued request without error

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST queue incoming chat completion requests when all backend agents are at capacity instead of immediately rejecting them
- **FR-002**: System MUST support two priority levels: High and Normal, determined by the `X-Nexus-Priority` request header
- **FR-003**: System MUST parse priority headers case-insensitively and default to Normal priority for missing or invalid header values
- **FR-004**: System MUST enforce a bounded queue with configurable maximum size to prevent unbounded memory growth
- **FR-005**: System MUST dequeue high-priority requests before normal-priority requests when capacity becomes available
- **FR-006**: System MUST reject new requests with 503 Service Unavailable when the queue is full (depth equals `max_size`)
- **FR-007**: System MUST timeout queued requests that exceed the configured `max_wait_seconds` with 503 and `Retry-After` header
- **FR-008**: System MUST re-run routing decision logic for each dequeued request to account for changed backend state
- **FR-009**: System MUST re-enqueue requests that fail routing or processing if they have not exceeded timeout
- **FR-010**: System MUST support queue enable/disable via configuration without code changes
- **FR-011**: System MUST expose current queue depth via Prometheus `nexus_queue_depth` gauge metric
- **FR-012**: System MUST drain all queued requests with 503 responses during graceful shutdown
- **FR-013**: System MUST poll the queue for ready requests at regular intervals (50ms) to balance responsiveness and efficiency
- **FR-014**: System MUST track total depth across both priority queues using atomic operations to ensure consistency

### Key Entities

- **QueuedRequest**: Represents a chat completion request waiting in the queue
  - Contains: routing intent, original request body, response channel, enqueue timestamp, priority level
  - Purpose: Holds all information needed to process the request once capacity is available

- **Priority**: Enumeration of request priority levels
  - Values: High, Normal
  - Purpose: Determines dequeue order when multiple requests are waiting

- **RequestQueue**: Bounded dual-priority queue structure
  - Maintains: separate high/normal tokio mpsc channels, atomic depth counter, configuration
  - Purpose: Manages request queueing, priority ordering, and capacity enforcement

- **QueueConfig**: Configuration parameters for queue behavior
  - Contains: enabled flag, max_size (u32), max_wait_seconds (u64)
  - Purpose: Allows runtime configuration of queue behavior without code changes

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: During traffic spikes, 95% of requests that would have received immediate 503 errors are successfully queued and processed within the timeout window
- **SC-002**: High-priority requests are processed with 90% lower queue wait time compared to normal-priority requests under load
- **SC-003**: Queue depth metric accurately reflects the current number of queued requests with <100ms lag
- **SC-004**: Queued requests that exceed `max_wait_seconds` receive timeout responses within 200ms of the timeout threshold
- **SC-005**: System successfully handles queue overflow (full queue) scenarios by rejecting new requests within 10ms
- **SC-006**: During graceful shutdown, all queued requests receive 503 responses within 1 second
- **SC-007**: The queue can handle at least 100 concurrent enqueue/dequeue operations without data corruption or depth counter drift
- **SC-008**: Queue configuration changes (enabled/disabled, max_size, timeout) take effect on service restart without requiring code changes

### Operational Success

- **SC-009**: Operations teams can detect queue saturation (depth approaching max_size) via Prometheus metrics
- **SC-010**: Failed routing/processing attempts are successfully re-queued without loss of request data or priority
- **SC-011**: Clients receive actionable `Retry-After` headers in timeout responses indicating when to retry (based on current queue depth)

## Configuration *(optional)*

### Queue Configuration

The queue behavior is controlled via the `[queue]` section in `nexus.toml`:

```toml
[queue]
# Enable request queuing when backends are saturated
enabled = true

# Maximum number of requests that can be queued
# Setting to 0 disables queueing (equivalent to enabled=false)
max_size = 100

# Maximum time a request can wait in the queue before timing out
# Timeouts receive 503 with Retry-After header
max_wait_seconds = 30
```

### Configuration Behavior

- **enabled = false**: Queue is disabled; requests that would be queued immediately return 503
- **max_size = 0**: Functionally equivalent to `enabled = false`
- **max_wait_seconds**: Applied per-request from enqueue time; longer waits result in 503 timeout

## API Integration *(optional)*

### Priority Header

Clients can specify request priority using the `X-Nexus-Priority` header:

```http
POST /v1/chat/completions
X-Nexus-Priority: high
Content-Type: application/json

{
  "model": "gpt-4",
  "messages": [...]
}
```

**Header Values**:
- `high`: Request is placed in high-priority queue
- `normal`: Request is placed in normal-priority queue (default)
- Missing or invalid: Defaults to `normal`
- Case-insensitive: "HIGH", "high", "High" all work

### Timeout Response

When a queued request exceeds `max_wait_seconds`, clients receive:

```http
HTTP/1.1 503 Service Unavailable
Retry-After: 30
Content-Type: application/json

{
  "error": {
    "message": "Request timed out after 30s in queue",
    "type": "service_unavailable",
    "code": "queue_timeout"
  }
}
```

The `Retry-After` header indicates the suggested wait time before retrying.

## Metrics *(optional)*

### Prometheus Metrics

- **nexus_queue_depth** (gauge): Current number of requests in the queue (high + normal priority)
  - Updated atomically on enqueue and dequeue operations
  - Useful for capacity planning and alerting on queue saturation

## Implementation Notes *(optional)*

### Architecture Decisions (for retrospective context)

1. **Dual-Channel Design**: Separate tokio mpsc channels for high/normal priority with shared depth counter
   - Allows independent buffer sizing while enforcing global max_size
   - High-priority channel drained first via poll ordering

2. **50ms Drain Interval**: Balances request processing latency with CPU overhead
   - Fast enough for reasonable responsiveness (<50ms added latency)
   - Infrequent enough to avoid busy-looping

3. **Re-routing on Dequeue**: Each dequeued request re-runs routing logic
   - Accounts for backend state changes while request was queued
   - Prevents routing to backends that became unavailable

4. **Oneshot Channels**: Used for request/response communication
   - Prevents multiple response deliveries
   - Automatically handles client disconnection (closed receiver)

5. **Atomic Depth Tracking**: AtomicUsize for total queue depth
   - Ensures consistent capacity enforcement under concurrent enqueue
   - Enables lock-free depth metric reporting

### Key Files (for reference)

- `src/queue/mod.rs` (632 lines): Core queue implementation with 14 unit tests
- `src/config/queue.rs` (58 lines): Configuration structure
- `src/routing/reconciler/decision.rs`: RoutingDecision::Queue variant
- `src/api/completions.rs` (lines 409-450): API integration and priority parsing
- `src/cli/serve.rs`: Queue initialization and drain loop startup
- `tests/queue_test.rs` (169 lines): 2 integration tests

## Dependencies *(optional)*

### Feature Dependencies

- **F17 (Routing Reconciler)**: Queue integration triggered by RoutingDecision::Queue when all candidates are excluded
- **F12 (Chat Completions API)**: Queue is invoked from chat completions handler when routing fails
- **F14 (Cost & Budget Tracking)**: Queued requests maintain budget context throughout queue lifecycle

### External Dependencies

- **tokio::sync::mpsc**: Async multi-producer single-consumer channels for dual-priority queues
- **tokio::sync::oneshot**: Request/response communication between API handler and drain loop
- **tokio::time::timeout**: Enforces max_wait_seconds timeout on queued requests

## Assumptions *(optional)*

1. **Queue depth of 100 is sufficient**: Based on expected traffic patterns, a default of 100 queued requests should handle typical burst scenarios without excessive memory usage
2. **30-second timeout is acceptable**: Most LLM requests complete within 10-15 seconds; 30 seconds provides margin while preventing indefinite waits
3. **Two priority levels are sufficient**: High/Normal split covers most use cases (critical vs. standard); more granular levels would add complexity without clear value
4. **50ms drain interval is appropriate**: Balances responsiveness (max 50ms added latency) with CPU efficiency; faster polling would increase overhead without meaningful benefit
5. **Re-routing on dequeue is necessary**: Backend health/capacity changes frequently enough that stale routing decisions would cause failures
6. **Priority header is client-controlled**: Assumes clients are trusted to set appropriate priorities; no server-side policy enforcement (could be added later if needed)
7. **FIFO within priority level**: Within each priority level, requests are processed in enqueue order (no further sub-prioritization)

## Out of Scope *(optional)*

- **Persistent Queue**: Queue is in-memory only; queued requests are lost on service restart
- **Cross-Instance Queue**: Each Nexus instance has an independent queue; no shared queue across replicas
- **Advanced Scheduling**: No support for weighted priorities, deadline-based scheduling, or fair-share algorithms
- **Queue Analytics**: No historical metrics on queue wait times, utilization trends, or per-priority statistics
- **Dynamic Configuration**: Queue config requires service restart; no runtime updates via API
- **Per-Model Queues**: Single global queue for all models/backends; no model-specific queue sizing
- **Backpressure to Clients**: No rate limiting or client backoff signals beyond 503 responses
- **Priority Enforcement**: No authorization checks on priority header; clients can claim any priority
