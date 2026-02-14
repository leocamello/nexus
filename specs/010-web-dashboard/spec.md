# Feature Specification: Web Dashboard

**Feature Branch**: `010-web-dashboard`  
**Created**: 2024-02-14  
**Status**: Draft  
**Input**: User description: "Simple web UI for monitoring Nexus status. Embedded in the binary, no external dependencies."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Monitor Backend Health Status (Priority: P1)

As a Nexus operator, I need to quickly check which backends are healthy and operational so that I can detect and respond to outages immediately.

**Why this priority**: Backend health is the most critical operational metric. Without healthy backends, the entire system is non-functional. This provides immediate visibility into system availability.

**Independent Test**: Can be fully tested by starting Nexus with multiple backends (some healthy, some down), accessing the dashboard, and verifying that backend status indicators accurately reflect the current state. Delivers immediate operational value even without other dashboard features.

**Acceptance Scenarios**:

1. **Given** Nexus is running with 3 backends (2 healthy, 1 unhealthy), **When** I open the dashboard, **Then** I see a list of all 3 backends with green indicators for healthy backends and red indicators for the unhealthy backend
2. **Given** a backend becomes unhealthy while I'm viewing the dashboard, **When** the health check completes, **Then** the backend's status indicator changes from green to red in real-time (via WebSocket) or within 5 seconds (via fallback refresh)
3. **Given** I'm viewing the dashboard, **When** I look at a backend's status, **Then** I see the last health check timestamp, pending request count, and average latency for that backend

---

### User Story 2 - View Model Availability Matrix (Priority: P2)

As a Nexus operator, I need to see which models are available on which backends so that I can understand my system's current capabilities and troubleshoot model routing issues.

**Why this priority**: Understanding model distribution across backends is essential for capacity planning and troubleshooting, but less urgent than knowing if backends are up/down. This helps operators understand "what can I serve right now?"

**Independent Test**: Can be tested independently by configuring multiple backends with different model sets, accessing the dashboard, and verifying the matrix shows accurate model-to-backend mappings. Delivers value for capacity planning and routing diagnostics.

**Acceptance Scenarios**:

1. **Given** Nexus has 3 backends each supporting different models, **When** I view the dashboard, **Then** I see a grid showing which models are available on which backends
2. **Given** a model has special capabilities (vision, tools, JSON mode), **When** I view the model in the matrix, **Then** I see capability indicators for that model
3. **Given** models have different context lengths, **When** I view the model matrix, **Then** I see the context length displayed for each model
4. **Given** a backend goes offline, **When** I view the model matrix, **Then** models only available on that backend show as unavailable

---

### User Story 3 - Review Request History (Priority: P3)

As a Nexus operator, I need to see recent requests and their outcomes so that I can identify patterns, troubleshoot errors, and understand system usage.

**Why this priority**: Request history is valuable for debugging and understanding patterns, but not critical for immediate operational awareness. Provides forensic capabilities after issues are detected.

**Independent Test**: Can be tested by sending various requests through Nexus (successful and failed), accessing the dashboard, and verifying the last 100 requests are displayed with accurate details. Delivers value for troubleshooting and usage analysis.

**Acceptance Scenarios**:

1. **Given** Nexus has processed 150 requests, **When** I view the dashboard, **Then** I see the most recent 100 requests in reverse chronological order
2. **Given** I'm viewing the request history, **When** I look at a request entry, **Then** I see the model name, backend used, duration, and status (success/error)
3. **Given** a request resulted in an error, **When** I click or expand that request, **Then** I see detailed error information
4. **Given** new requests are being processed, **When** I'm viewing the dashboard, **Then** the request history updates in real-time (via WebSocket) or within 5 seconds (via fallback)

---

### User Story 4 - Access Dashboard Without JavaScript (Priority: P4)

As a Nexus operator in a restricted environment or using a text-based browser, I need to view basic dashboard information without JavaScript so that I can monitor the system regardless of browser capabilities.

**Why this priority**: Accessibility and graceful degradation are important for resilience, but most users will have JavaScript enabled. This ensures the dashboard works in edge cases and restrictive environments.

**Independent Test**: Can be tested by disabling JavaScript in the browser, accessing the dashboard, and verifying that static information is displayed with a manual refresh option. Delivers basic monitoring capability in constrained environments.

**Acceptance Scenarios**:

1. **Given** I access the dashboard with JavaScript disabled, **When** the page loads, **Then** I see backend status and model availability in a static HTML table
2. **Given** JavaScript is disabled, **When** I need updated information, **Then** I see a refresh button that reloads the page with current data
3. **Given** JavaScript is enabled, **When** the page loads, **Then** I see real-time updates via WebSocket without needing manual refresh

---

### User Story 5 - View Dashboard on Mobile Device (Priority: P4)

As a Nexus operator on-call, I need to check system status from my mobile device so that I can monitor and respond to issues when away from my workstation.

**Why this priority**: Mobile access is important for on-call scenarios, but desktop access is the primary use case. This ensures operators can respond to alerts from anywhere.

**Independent Test**: Can be tested by accessing the dashboard from various mobile screen sizes and verifying that the layout adapts appropriately and information remains readable. Delivers monitoring capability for on-call scenarios.

**Acceptance Scenarios**:

1. **Given** I access the dashboard from a mobile device, **When** the page loads, **Then** the layout adapts to the small screen with readable text and touch-friendly controls
2. **Given** I'm viewing the dashboard on mobile, **When** I interact with expandable elements, **Then** they respond appropriately to touch gestures
3. **Given** my device is in dark mode, **When** I access the dashboard, **Then** the interface automatically uses a dark color scheme

---

### Edge Cases

- **What happens when no backends are registered?** Dashboard should display "No backends configured" message instead of an empty list
- **What happens when WebSocket connection fails or disconnects?** Dashboard should automatically fall back to polling every 5 seconds and show a reconnection indicator
- **What happens when a backend is registered but has never been health checked?** Dashboard should show "Pending" status with a gray/yellow indicator
- **What happens when request history buffer is empty?** Dashboard should display "No requests recorded" message
- **What happens when a model name is very long or contains special characters?** Model names should be truncated with ellipsis or wrapped appropriately to prevent layout breaking
- **What happens when accessing the dashboard from a browser with cookies/storage disabled?** Dashboard should function normally since it doesn't require client-side persistence
- **What happens when latency values are extremely high or null?** Dashboard should display values gracefully (e.g., ">10s" or "N/A")
- **What happens during concurrent updates to backend status?** Dashboard should handle race conditions gracefully, showing the most recent state without flickering

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST serve a web dashboard at the root path `/` that displays backend status, model availability, and request history
- **FR-002**: System MUST embed all dashboard assets (HTML, CSS, JavaScript) in the compiled binary with no external file dependencies
- **FR-003**: Dashboard MUST display each registered backend with a status indicator showing healthy (green), unhealthy (red), unknown/pending (yellow), or draining (orange)
- **FR-004**: Dashboard MUST show for each backend: name, status, last health check timestamp, pending request count, and average latency
- **FR-005**: Dashboard MUST display a model availability matrix showing which models are available on which backends
- **FR-006**: Dashboard MUST indicate model capabilities including vision support, tools support, JSON mode support, and context length
- **FR-007**: Dashboard MUST display the last 100 requests in an in-memory ring buffer with model, backend, duration, and status information
- **FR-008**: Dashboard MUST provide expandable detail view for failed requests showing error messages and relevant context
- **FR-009**: Dashboard MUST establish a WebSocket connection at `/ws` for real-time updates of backend status, model availability, and request history
- **FR-010**: Dashboard MUST fall back to polling every 5 seconds when WebSocket connection is unavailable or fails
- **FR-011**: Dashboard MUST function with JavaScript disabled, displaying static content with a manual refresh option
- **FR-012**: Dashboard MUST be responsive and display correctly on mobile devices with screen widths down to 320px
- **FR-013**: Dashboard MUST support dark mode using CSS `prefers-color-scheme` media query
- **FR-014**: System MUST serve dashboard static assets (JavaScript, CSS) from `/assets/*` path
- **FR-015**: Dashboard MUST consume data from existing `/v1/stats` and `/v1/models` JSON endpoints
- **FR-016**: Dashboard MUST display connection status indicator showing WebSocket connection state or last refresh time
- **FR-017**: Dashboard MUST show system uptime and total request count in a summary header
- **FR-018**: Dashboard MUST handle concurrent updates to displayed data without UI flickering or race conditions
- **FR-019**: System MUST maintain existing API routes (`/v1/chat/completions`, `/v1/models`, `/v1/stats`, `/metrics`) unchanged
- **FR-020**: Dashboard MUST load and display within 2 seconds on a 10 Mbps connection

### Key Entities

- **Backend Status Entry**: Represents the current state of a registered backend including name, health status (healthy/unhealthy/pending), last health check timestamp, pending request count, and average latency
- **Model Availability Entry**: Represents a model's availability including model name, list of backends serving it, capabilities (vision, tools, JSON mode), and context length
- **Request History Entry**: Represents a completed request including timestamp, model name, backend used, duration in milliseconds, status (success/error), and error details if applicable
- **WebSocket Update Message**: Represents a real-time update containing changed backend status, new request completion, or model availability change

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Operators can identify backend health status within 3 seconds of opening the dashboard
- **SC-002**: Dashboard displays accurate real-time status updates within 5 seconds of backend state changes
- **SC-003**: Dashboard loads completely in under 2 seconds on a 10 Mbps connection
- **SC-004**: Dashboard remains functional and displays all core information with JavaScript disabled
- **SC-005**: Dashboard is fully usable on mobile devices with screen widths as small as 320px
- **SC-006**: System binary size increases by no more than 200KB due to embedded dashboard assets
- **SC-007**: Dashboard WebSocket connection maintains stability for at least 24 hours of continuous operation
- **SC-008**: Request history buffer correctly maintains the most recent 100 requests without memory leaks
- **SC-009**: Dashboard supports at least 50 concurrent viewer connections without performance degradation
- **SC-010**: All backend health status changes are reflected in the dashboard within 5 seconds

## Assumptions *(if any)*

- **Assumption 1**: Nexus will have at least one backend registered for the dashboard to display meaningful data (though it should gracefully handle zero backends)
- **Assumption 2**: Operators accessing the dashboard have network access to the Nexus instance (dashboard does not support offline mode)
- **Assumption 3**: The existing `/v1/stats` endpoint provides all necessary backend and request metrics required by the dashboard
- **Assumption 4**: Tailwind CSS will be precompiled and included as a static asset, not generated at runtime
- **Assumption 5**: The 100-request history buffer is sufficient for troubleshooting needs (older requests can be found in structured logs from F11)
- **Assumption 6**: WebSocket connections will be used for updates at intervals no more frequent than once per second to avoid overwhelming clients
- **Assumption 7**: The dashboard will be accessed by a small number of concurrent users (expected < 10) rather than public-facing traffic
- **Assumption 8**: Browser compatibility targets modern browsers (Chrome, Firefox, Safari, Edge) from the last 2 years

## Out of Scope *(if any)*

- **User authentication or authorization** - Dashboard is assumed to be accessible to anyone who can reach the Nexus instance; security is handled at the network level (v0.3 feature)
- **Historical metrics and long-term trends** - Dashboard shows current state and recent history only; time-series visualization requires external tools like Grafana with the existing `/metrics` endpoint
- **Request filtering or search** - The 100-request buffer displays in chronological order only; advanced querying requires structured logs (F11)
- **Dashboard customization or preferences** - Layout and displayed information is fixed; no user-configurable views or saved preferences
- **Alerts or notifications** - Dashboard is view-only; proactive alerting requires external monitoring tools
- **Request replay or debugging tools** - Dashboard shows request results but doesn't provide request/response inspection or retry capabilities
- **Multi-language support** - Dashboard text is English-only
- **Backend management actions** - Dashboard is read-only; no ability to add/remove backends or trigger health checks manually
- **Export or download functionality** - No CSV/JSON export of displayed data; operators can access raw data via `/v1/stats` endpoint
- **Custom theming beyond dark/light mode** - Color scheme follows system preference only

## Dependencies *(if any)*

- **Dependency 1**: F09 (Request Metrics) - Dashboard consumes metrics collected by the MetricsCollector including per-backend latency and request counts
- **Dependency 2**: F02 (Health Checker) - Dashboard displays health check results and timestamps from the existing health checking system
- **Dependency 3**: F01 (Backend Registry) - Dashboard reads backend list and metadata from the DashMap-based Registry
- **Dependency 4**: Existing `/v1/stats` endpoint - Dashboard uses this JSON endpoint as its primary data source for backend and request statistics
- **Dependency 5**: Existing `/v1/models` endpoint - Dashboard uses this endpoint to build the model availability matrix

## Risks & Mitigations *(if any)*

- **Risk 1**: Embedding static assets increases binary size significantly
  - *Mitigation*: Use compression for embedded assets and keep CSS/JS minimal; success criteria limits increase to 200KB
- **Risk 2**: WebSocket connections may be blocked by corporate firewalls or proxies
  - *Mitigation*: Implement automatic fallback to HTTP polling every 5 seconds when WebSocket fails
- **Risk 3**: In-memory request history buffer could grow unbounded if not properly managed
  - *Mitigation*: Implement fixed-size ring buffer (100 entries) with automatic oldest-entry eviction
- **Risk 4**: Concurrent WebSocket updates could cause race conditions in UI rendering
  - *Mitigation*: Implement message sequencing or state diffing to ensure consistent UI updates
- **Risk 5**: Dashboard may expose sensitive information about backend infrastructure
  - *Mitigation*: Document that dashboard access should be restricted at network level; plan authentication for v0.3
- **Risk 6**: Maintaining vanilla JavaScript code without a framework may become complex
  - *Mitigation*: Keep JavaScript minimal and well-structured; focus on consuming JSON endpoints rather than complex client-side logic

## Notes *(if any)*

- The dashboard aligns with the v0.2 "Observability" theme alongside F09 (metrics) and F11 (structured logging)
- Constitutional principle "single binary, no external dependencies" is maintained through rust-embed
- The dashboard provides human-friendly visualization of data already available through `/v1/stats` and `/metrics` endpoints
- WebSocket implementation will use axum's built-in WebSocket support for consistency with existing stack
- Tailwind CSS will be used in precompiled form to avoid build-time dependencies
- The 100-request in-memory buffer is intentionally ephemeral; persistent request logs will come from F11 (structured logging)
- Dashboard does not replace Prometheus/Grafana for production monitoring but provides quick operational visibility
- Mobile responsiveness and dark mode support follow modern web standards without requiring additional dependencies
