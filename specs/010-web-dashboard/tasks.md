# Tasks: Web Dashboard

**Feature Branch**: `010-web-dashboard`  
**Input**: Design documents from `/specs/010-web-dashboard/`  
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/ ✓

**Tests**: Following TDD workflow - tests written FIRST, must FAIL before implementation

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `- [ ] [ID] [P?] [Story] Description`

- **Checkbox**: ALWAYS starts with `- [ ]` (markdown checkbox)
- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and basic structure for dashboard feature

- [x] T001 Create dashboard module structure in src/dashboard/ with mod.rs, handler.rs, websocket.rs, history.rs, types.rs
- [x] T002 Add rust-embed dependency to Cargo.toml for static asset embedding
- [x] T003 [P] Create dashboard assets directory at repository root: dashboard/
- [x] T004 [P] Add mime_guess dependency to Cargo.toml for MIME type detection
- [x] T005 Register dashboard module in src/lib.rs

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core dashboard infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

### Dashboard Types & Data Structures

- [x] T006 [P] Create HistoryEntry struct in src/dashboard/types.rs with timestamp, model, backend_id, duration_ms, status, error_message fields
- [x] T007 [P] Create RequestStatus enum in src/dashboard/types.rs (Success, Error variants)
- [x] T008 [P] Create WebSocketUpdate struct in src/dashboard/types.rs with update_type and data fields
- [x] T009 [P] Create UpdateType enum in src/dashboard/types.rs (BackendStatus, RequestComplete, ModelChange variants)
- [x] T010 Implement Serialize/Deserialize derives for all dashboard types

### Ring Buffer Implementation (TDD)

- [x] T011 Write unit test for RequestHistory::new() in src/dashboard/history.rs #[cfg(test)] module (MUST FAIL)
- [x] T012 Write unit test for RequestHistory::push() with capacity management in src/dashboard/history.rs (MUST FAIL)
- [x] T013 Write unit test for RequestHistory::get_all() in src/dashboard/history.rs (MUST FAIL)
- [x] T014 Write unit test for RequestHistory ring buffer FIFO eviction when exceeding 100 entries in src/dashboard/history.rs (MUST FAIL)
- [x] T015 Implement RequestHistory struct in src/dashboard/history.rs with RwLock<VecDeque<HistoryEntry>>
- [x] T016 Implement RequestHistory::new() method with capacity of 100
- [x] T017 Implement RequestHistory::push() method with FIFO eviction when full
- [x] T018 Implement RequestHistory::get_all() method returning Vec<HistoryEntry>
- [x] T019 Implement RequestHistory::len() method for test verification
- [x] T020 Run unit tests for RequestHistory - verify all tests PASS

### AppState Extension

- [x] T021 Add request_history: Arc<RequestHistory> field to AppState in src/api/mod.rs
- [x] T022 Add ws_broadcast: broadcast::Sender<WebSocketUpdate> field to AppState in src/api/mod.rs
- [x] T023 Update AppState::new() to initialize RequestHistory with RequestHistory::new()
- [x] T024 Update AppState::new() to create broadcast channel with capacity of 1000 messages
- [x] T025 Add tokio::sync::broadcast import to src/api/mod.rs

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Monitor Backend Health Status (Priority: P1) 🎯 MVP

**Goal**: Display real-time backend health status with indicators, metrics, and WebSocket updates

**Independent Test**: Start Nexus with multiple backends (some healthy, some down), access dashboard at http://localhost:3000/, verify backend status indicators accurately reflect current state and update in real-time

### Contract Tests for US1 (TDD - Write FIRST)

- [x] T026 [P] [US1] Write contract test for WebSocketUpdate deserialization with BackendStatus type in tests/contract/dashboard_websocket_test.rs (MUST FAIL)
- [x] T027 [P] [US1] Write contract test for backend_status update message schema validation in tests/contract/dashboard_websocket_test.rs (MUST FAIL)
- [x] T028 [P] [US1] Write contract test verifying backend_status data includes all required fields in tests/contract/dashboard_websocket_test.rs (MUST FAIL)

### Integration Tests for US1 (TDD - Write FIRST)

- [x] T029 [P] [US1] Write integration test for GET / dashboard endpoint returns 200 with HTML in tests/integration/dashboard_test.rs (MUST FAIL)
- [x] T030 [P] [US1] Write integration test for WebSocket /ws endpoint accepts connections in tests/integration/dashboard_test.rs (MUST FAIL)
- [x] T031 [P] [US1] Write integration test for WebSocket sends backend_status update on health change in tests/integration/dashboard_test.rs (MUST FAIL)
- [x] T032 [P] [US1] Write integration test for GET /assets/styles.css returns CSS with correct MIME type in tests/integration/dashboard_test.rs (MUST FAIL)

### Static Assets for US1

- [x] T033 [P] [US1] Create index.html in dashboard/ with HTML5 structure, meta viewport, and dark mode support
- [x] T034 [P] [US1] Add backend status section to index.html with container for backend cards
- [x] T035 [P] [US1] Add connection status indicator to index.html showing WebSocket state or last refresh time
- [x] T036 [P] [US1] Create styles.css in dashboard/ with Tailwind utility classes for responsive layout
- [x] T037 [P] [US1] Add dark mode color scheme to styles.css using @media (prefers-color-scheme: dark)
- [x] T038 [P] [US1] Add responsive grid styles for backend cards in styles.css (mobile: 1 col, tablet: 2 col, desktop: 3 col)

### HTTP Handler Implementation for US1

- [x] T039 [US1] Implement DashboardAssets struct with RustEmbed derive in src/dashboard/handler.rs for dashboard/ folder
- [x] T040 [US1] Implement dashboard_handler for GET / in src/dashboard/handler.rs serving index.html with stats data
- [x] T041 [US1] Implement assets_handler for GET /assets/* in src/dashboard/handler.rs serving static files with MIME types
- [x] T042 [US1] Add error handling for missing assets returning 404 in src/dashboard/handler.rs
- [x] T043 [US1] Export dashboard_handler and assets_handler from src/dashboard/mod.rs

### WebSocket Implementation for US1

- [x] T044 [US1] Implement websocket_handler for GET /ws in src/dashboard/websocket.rs accepting WebSocketUpgrade
- [x] T045 [US1] Implement handle_socket function in src/dashboard/websocket.rs splitting socket into sender/receiver
- [x] T046 [US1] Subscribe to ws_broadcast channel in handle_socket and forward messages to WebSocket client
- [x] T047 [US1] Handle WebSocket close messages and ping/pong for keep-alive in src/dashboard/websocket.rs
- [x] T048 [US1] Add error handling for closed connections in src/dashboard/websocket.rs
- [x] T049 [US1] Export websocket_handler from src/dashboard/mod.rs

### JavaScript Client for US1

- [x] T050 [US1] Create dashboard.js in dashboard/ with WebSocket connection logic to ws://host/ws
- [x] T051 [US1] Implement connectWebSocket() function with automatic reconnection on failure
- [x] T052 [US1] Implement handleBackendStatusUpdate() function to update backend card DOM elements
- [x] T053 [US1] Add WebSocket message routing based on update_type field in dashboard.js
- [x] T054 [US1] Implement renderBackendCard() function creating status indicator (green/red/yellow) and metrics display
- [x] T055 [US1] Add connection status indicator updates on WebSocket open/close/error events

### Routing Integration for US1

- [x] T056 [US1] Import dashboard handlers in src/api/mod.rs (dashboard_handler, assets_handler, websocket_handler)
- [x] T057 [US1] Add route("/, get(dashboard_handler)) to router in src/api/mod.rs
- [x] T058 [US1] Add route("/assets/*path", get(assets_handler)) to router in src/api/mod.rs
- [x] T059 [US1] Add route("/ws", get(websocket_handler)) to router in src/api/mod.rs

### Backend Status Broadcasting for US1

- [x] T060 [US1] Hook into health checker to send backend_status updates via ws_broadcast in src/health/mod.rs
- [x] T061 [US1] Create BackendStatusUpdate helper function in src/dashboard/websocket.rs to construct update messages
### System Summary Header (FR-017)

- [x] T062a [US1] Write unit test for system uptime calculation from AppState.start_time in src/dashboard/handler.rs (MUST FAIL)
- [x] T062b [US1] Write unit test for total request count extraction from StatsResponse in src/dashboard/handler.rs (MUST FAIL)
- [x] T062c [US1] Implement system summary data in dashboard_handler: calculate uptime from start_time, extract total requests from /v1/stats
- [x] T062d [P] [US1] Add summary header section to dashboard/index.html showing uptime and total request count
- [x] T062e [US1] Update dashboard.js to render and refresh summary header from /v1/stats JSON

### Real-Time Updates (WebSocket Broadcasting)

- [x] T062 [US1] Broadcast backend_status updates when health check completes in src/health/mod.rs
- [x] T063 [US1] Broadcast backend_status updates when pending request count changes in src/routing/mod.rs (covered by health check broadcasts)

### Testing & Verification for US1

- [x] T064 [US1] Run contract tests - verify WebSocket message schema tests PASS
- [x] T065 [US1] Run integration tests - verify dashboard endpoints and WebSocket tests PASS
- [x] T066 [US1] Run unit tests for RequestHistory - verify all tests PASS
- [x] T067 [US1] Manual test: Start Nexus with 3 backends (2 healthy, 1 unhealthy), verify status indicators correct
- [x] T068 [US1] Manual test: Stop a backend, verify status changes from green to red within 5 seconds
- [x] T069 [US1] Manual test: Verify WebSocket connection in browser DevTools Network tab

**Checkpoint**: User Story 1 complete - Backend health monitoring fully functional with real-time updates

---

## Phase 4: User Story 2 - View Model Availability Matrix (Priority: P2)

**Goal**: Display which models are available on which backends with capability indicators

**Independent Test**: Configure multiple backends with different model sets, access dashboard, verify matrix shows accurate model-to-backend mappings with capability icons

### Contract Tests for US2 (TDD - Write FIRST)

- [x] T070 [P] [US2] Write contract test for model_change WebSocket update message schema in tests/contract/dashboard_websocket_test.rs (MUST FAIL)
- [x] T071 [P] [US2] Write contract test verifying model data includes capabilities fields in tests/contract/dashboard_websocket_test.rs (MUST FAIL)

### Integration Tests for US2 (TDD - Write FIRST)

- [x] T072 [P] [US2] Write integration test for WebSocket sends model_change update when backend models updated in tests/integration/dashboard_test.rs (MUST FAIL)
- [x] T073 [P] [US2] Write integration test verifying model matrix reflects model removal when backend goes offline in tests/integration/dashboard_test.rs (MUST FAIL)

### Static Assets for US2

- [x] T074 [P] [US2] Add model availability matrix section to dashboard/index.html with table structure
- [x] T075 [P] [US2] Add capability indicator icons (vision, tools, JSON mode) to dashboard/index.html
- [x] T076 [P] [US2] Add styles for model matrix table in dashboard/styles.css with responsive layout
- [x] T077 [P] [US2] Add badge styles for capability indicators in dashboard/styles.css

### JavaScript Implementation for US2

- [x] T078 [US2] Implement handleModelChangeUpdate() function in dashboard/dashboard.js
- [x] T079 [US2] Implement renderModelMatrix() function creating backend-to-model grid in dashboard/dashboard.js
- [x] T080 [US2] Implement renderModelCapabilities() function displaying vision/tools/JSON mode icons in dashboard/dashboard.js
- [x] T081 [US2] Add context length formatting (e.g., "32k") in dashboard/dashboard.js
- [x] T082 [US2] Handle model unavailability when backend offline in dashboard/dashboard.js

### Model Change Broadcasting for US2

- [x] T083 [US2] Hook into health checker to send model_change updates via ws_broadcast in src/health/mod.rs
- [x] T084 [US2] Create ModelChangeUpdate helper function in src/dashboard/websocket.rs
- [x] T085 [US2] Broadcast model_change updates when models added to backend in src/health/mod.rs
- [x] T086 [US2] Broadcast model_change updates when backend fails (empty model list) in src/health/mod.rs

### Initial Data Loading for US2

- [x] T087 [US2] Fetch /v1/models endpoint data on dashboard initial load in dashboard/dashboard.js
- [x] T088 [US2] Render initial model matrix before WebSocket connection established in dashboard/dashboard.js

### Testing & Verification for US2

- [x] T089 [US2] Run contract tests - verify model_change message schema tests PASS
- [x] T090 [US2] Run integration tests - verify model change WebSocket updates PASS
- [x] T091 [US2] Manual test: Configure 3 backends with different models, verify matrix shows correct availability
- [x] T092 [US2] Manual test: Verify capability indicators (vision, tools, JSON) display correctly
- [x] T093 [US2] Manual test: Stop a backend, verify models only on that backend show as unavailable

**Checkpoint**: User Story 2 complete - Model availability matrix fully functional with capability indicators

---

## Phase 5: User Story 3 - Review Request History (Priority: P3)

**Goal**: Display recent request history with real-time updates showing model, backend, duration, and status

**Independent Test**: Send various requests through Nexus (successful and failed), access dashboard, verify last 100 requests displayed with accurate details and real-time updates

### Contract Tests for US3 (TDD - Write FIRST)

- [x] T094 [P] [US3] Write contract test for request_complete WebSocket update message schema in tests/contract/dashboard_websocket_test.rs (MUST FAIL)
- [x] T095 [P] [US3] Write contract test verifying HistoryEntry serialization includes all fields in tests/contract/dashboard_websocket_test.rs (MUST FAIL)
- [x] T096 [P] [US3] Write contract test for error_message field nullable in request_complete updates in tests/contract/dashboard_websocket_test.rs (MUST FAIL)

### Integration Tests for US3 (TDD - Write FIRST)

- [x] T097 [P] [US3] Write integration test for request history contains max 100 entries in tests/integration/dashboard_test.rs (MUST FAIL)
- [x] T098 [P] [US3] Write integration test for WebSocket sends request_complete update when request finishes in tests/integration/dashboard_test.rs (MUST FAIL)
- [x] T099 [P] [US3] Write integration test verifying oldest entries evicted when buffer exceeds 100 in tests/integration/dashboard_test.rs (MUST FAIL)

### Static Assets for US3

- [x] T100 [P] [US3] Add request history section to dashboard/index.html with table structure
- [x] T101 [P] [US3] Add expandable error detail section to dashboard/index.html
- [x] T102 [P] [US3] Add styles for request history table in dashboard/styles.css with alternating row colors
- [x] T103 [P] [US3] Add status badge styles (success=green, error=red) in dashboard/styles.css

### JavaScript Implementation for US3

- [x] T104 [US3] Implement handleRequestCompleteUpdate() function in dashboard/dashboard.js
- [x] T105 [US3] Implement renderRequestHistory() function creating table rows in dashboard/dashboard.js
- [x] T106 [US3] Implement renderRequestRow() function with timestamp, model, backend, duration, status in dashboard/dashboard.js
- [x] T107 [US3] Add click handler for error rows to expand and show error_message details in dashboard/dashboard.js
- [x] T108 [US3] Format duration as milliseconds with proper units (ms, s) in dashboard/dashboard.js
- [x] T109 [US3] Display only most recent 100 entries in reverse chronological order in dashboard/dashboard.js

### Request Completion Hook for US3

- [x] T110 [US3] Hook into request completion to create HistoryEntry in src/api/completions.rs
- [x] T111 [US3] Push HistoryEntry to request_history ring buffer on success in src/api/completions.rs
- [x] T112 [US3] Push HistoryEntry to request_history ring buffer on error with error_message in src/api/completions.rs
- [x] T113 [US3] Create RequestCompleteUpdate helper function in src/dashboard/websocket.rs
- [x] T114 [US3] Broadcast request_complete update via ws_broadcast in src/api/completions.rs

### Initial Data Loading for US3

- [x] T115 [US3] Fetch current request_history on dashboard initial load in dashboard/dashboard.js
- [x] T116 [US3] Add GET /v1/history endpoint in src/dashboard/handler.rs returning request_history.get_all()
- [x] T117 [US3] Add route("/v1/history", get(history_handler)) to router in src/api/mod.rs

### Testing & Verification for US3

- [x] T118 [US3] Run contract tests - verify request_complete message schema tests PASS
- [x] T119 [US3] Run integration tests - verify request history and WebSocket updates PASS
- [x] T120 [US3] Manual test: Send 150 requests, verify only most recent 100 shown
- [x] T121 [US3] Manual test: Send successful and failed requests, verify status indicators correct
- [x] T122 [US3] Manual test: Click error entry, verify error details expand with error message
- [x] T123 [US3] Manual test: Send new request, verify history updates within 5 seconds

**Checkpoint**: User Story 3 complete - Request history with error details fully functional

---

## Phase 6: User Story 4 - Access Dashboard Without JavaScript (Priority: P4)

**Goal**: Display static dashboard information when JavaScript is disabled with manual refresh option

**Independent Test**: Disable JavaScript in browser, access dashboard, verify static information displays with refresh button

### Static HTML Rendering for US4

- [x] T124 [P] [US4] Add <noscript> section to dashboard/index.html with "JavaScript disabled" message
- [x] T125 [P] [US4] Add <meta http-equiv="refresh" content="5"> inside <noscript> for auto-refresh
- [x] T126 [P] [US4] Add manual refresh button with link to "/" in <noscript> section
- [x] T127 [P] [US4] Add <script id="initial-data" type="application/json"> with embedded stats in dashboard/index.html

### Server-Side Data Injection for US4

- [x] T128 [US4] Fetch /v1/stats data in dashboard_handler before serving HTML in src/dashboard/handler.rs
- [x] T129 [US4] Fetch /v1/models data in dashboard_handler in src/dashboard/handler.rs
- [x] T130 [US4] Inject stats JSON into index.html template as <script id="initial-data"> content in src/dashboard/handler.rs
- [x] T131 [US4] Add static "Enable JavaScript for full dashboard experience" message with refresh link in <noscript> section of dashboard/index.html

### Progressive Enhancement for US4

- [x] T132 [US4] Update dashboard.js to check for initial-data script element and load from it
- [x] T133 [US4] Display static backend status table from initial data before WebSocket connects in dashboard/dashboard.js
- [x] T134 [US4] Hide <noscript> content when JavaScript is enabled via CSS in dashboard/styles.css

### Testing & Verification for US4

- [x] T135 [US4] Manual test: Disable JavaScript, verify static HTML table displays backend status
- [x] T136 [US4] Manual test: Verify refresh button reloads page with updated data
- [x] T137 [US4] Manual test: Verify page auto-refreshes every 5 seconds with JavaScript disabled
- [x] T138 [US4] Manual test: Enable JavaScript, verify real-time updates work and static content hidden

**Checkpoint**: User Story 4 complete - Dashboard works without JavaScript with graceful degradation

---

## Phase 7: User Story 5 - View Dashboard on Mobile Device (Priority: P4)

**Goal**: Responsive layout that adapts to mobile screen sizes with touch-friendly controls

**Independent Test**: Access dashboard from mobile device (or DevTools device emulation), verify layout adapts and remains readable on screens down to 320px width

### Responsive Layout for US5

- [x] T139 [P] [US5] Add viewport meta tag to dashboard/index.html for proper mobile scaling
- [x] T140 [P] [US5] Update backend status cards to use responsive grid in dashboard/styles.css (1 col on mobile)
- [x] T141 [P] [US5] Update model matrix to horizontal scroll on mobile in dashboard/styles.css
- [x] T142 [P] [US5] Update request history table to stack columns on mobile in dashboard/styles.css
- [x] T143 [P] [US5] Ensure minimum font size 16px on mobile to prevent zoom in dashboard/styles.css

### Touch Interaction for US5

- [x] T144 [P] [US5] Increase touch target size for expandable elements to min 44x44px in dashboard/styles.css
- [x] T145 [P] [US5] Add touch-friendly padding to clickable areas in dashboard/styles.css
- [x] T146 [US5] Update error expansion click handlers to support touch events in dashboard/dashboard.js

### Dark Mode Refinement for US5

- [x] T147 [P] [US5] Test dark mode color contrast meets WCAG AA standards in dashboard/styles.css
- [x] T148 [P] [US5] Ensure dark mode badge colors remain readable in dashboard/styles.css

### Testing & Verification for US5

- [x] T149 [US5] Manual test: Open DevTools responsive mode, test at 320px, 375px, 768px, 1024px widths
- [x] T150 [US5] Manual test: Verify all text readable without horizontal scroll at 320px width
- [x] T151 [US5] Manual test: Verify touch targets at least 44x44px on mobile
- [x] T152 [US5] Manual test: Test on actual mobile device (iOS and Android if possible)
- [x] T153 [US5] Manual test: Verify dark mode activates based on system preference on mobile

**Checkpoint**: User Story 5 complete - Dashboard fully responsive and mobile-friendly

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories and final quality checks

### Documentation

- [x] T154 [P] Update README.md with dashboard feature description and access instructions
- [x] T155 [P] Document WebSocket protocol in docs/ with message examples
- [x] T156 [P] Add dashboard section to quickstart.md with screenshots or examples

### Error Handling & Edge Cases

- [x] T157 [P] Handle "no backends configured" case with friendly message in dashboard/dashboard.js
- [x] T158 [P] Handle "no requests recorded" case with friendly message in dashboard/dashboard.js
- [x] T159 [P] Handle WebSocket reconnection with exponential backoff in dashboard/dashboard.js
- [x] T160 [P] Truncate very long model names with ellipsis in dashboard/dashboard.js
- [x] T161 [P] Handle null/undefined latency values gracefully with "N/A" display in dashboard/dashboard.js

### Performance Optimization

- [x] T162 [P] Add debouncing to DOM updates to prevent flickering in dashboard/dashboard.js
- [x] T163 [P] Verify embedded assets compressed with gzip in src/dashboard/handler.rs
- [x] T164 [P] Measure and document binary size increase (target: <200KB)
- [x] T165 Test dashboard with 50 concurrent WebSocket connections

### Fallback Polling Implementation

- [x] T166 Implement startPolling() function with 5-second interval in dashboard/dashboard.js
- [x] T167 Implement stopPolling() function to clear interval in dashboard/dashboard.js
- [x] T168 Add exponential backoff for repeated failures: 5s → 10s → 30s → 60s cap in dashboard/dashboard.js
- [x] T169 Automatically switch from WebSocket to polling on connection error in dashboard/dashboard.js
- [x] T170 Display polling status indicator when fallback active in dashboard/dashboard.js

### Security & Validation

- [x] T171 [P] Validate HistoryEntry fields: timestamp not in future, model max 256 chars in src/dashboard/history.rs
- [x] T172 [P] Truncate error_message to 1024 chars max in src/dashboard/history.rs
- [x] T173 [P] Set WebSocket message size limit to 10KB in src/dashboard/websocket.rs
- [x] T174 [P] Add rate limiting considerations documentation for dashboard endpoints

### Code Quality

- [x] T175 [P] Run cargo fmt on all dashboard module files
- [x] T176 [P] Run cargo clippy and fix warnings in dashboard module
- [x] T177 [P] Add doc comments to all public functions in src/dashboard/ modules
- [x] T178 [P] Review error handling patterns across all dashboard code

### Final Validation

- [x] T179 Run full test suite: cargo test (all unit, integration, contract tests must PASS)
- [x] T180 Run quickstart.md validation steps from specs/010-web-dashboard/quickstart.md
- [x] T181 Test all acceptance scenarios from spec.md user stories
- [x] T182 Verify all success criteria from spec.md measurable outcomes
- [x] T183 Final smoke test: Start fresh Nexus instance, access dashboard, verify all features work

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-7)**: All depend on Foundational phase completion
  - User stories can proceed in parallel (if staffed)
  - Or sequentially in priority order (US1 → US2 → US3 → US4 → US5)
- **Polish (Phase 8)**: Depends on all user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) - No dependencies on other stories
- **User Story 2 (P2)**: Can start after Foundational (Phase 2) - Integrates with US1 dashboard but independently testable
- **User Story 3 (P3)**: Can start after Foundational (Phase 2) - Uses same dashboard layout but independently testable
- **User Story 4 (P4)**: Depends on US1 completion (needs backend status rendering) - Enhances US1/US2/US3 with no-JS support
- **User Story 5 (P4)**: Can start after Foundational (Phase 2) - Adds responsive CSS to existing layouts

### Within Each User Story (TDD Order)

1. **Contract tests FIRST** → MUST FAIL (red)
2. **Integration tests SECOND** → MUST FAIL (red)
3. **Static assets THIRD** → Prepare UI structure
4. **Implementation FOURTH** → Tests start PASSING (green)
5. **Manual testing FIFTH** → Verify acceptance criteria
6. **Refactor if needed** → Tests remain PASSING

### Parallel Opportunities

#### Phase 1: Setup
- T003 (create directory) and T004 (add dependency) can run in parallel

#### Phase 2: Foundational
- T006-T010 (all type definitions) can run in parallel
- T026-T028 (all contract tests) can run in parallel
- T029-T032 (all integration tests) can run in parallel
- T033-T038 (all static assets) can run in parallel

#### Phase 3: User Story 1
- T026-T028 (all contract tests) can run in parallel
- T029-T032 (all integration tests) can run in parallel
- T033-T038 (all static asset files) can run in parallel

#### Phase 4: User Story 2
- T070-T071 (contract tests) can run in parallel
- T072-T073 (integration tests) can run in parallel
- T074-T077 (static assets) can run in parallel

#### Phase 5: User Story 3
- T094-T096 (contract tests) can run in parallel
- T097-T099 (integration tests) can run in parallel
- T100-T103 (static assets) can run in parallel

#### Phase 6: User Story 4
- T124-T127 (noscript HTML updates) can run in parallel

#### Phase 7: User Story 5
- T139-T145 (all CSS responsive updates) can run in parallel
- T147-T148 (dark mode refinements) can run in parallel

#### Phase 8: Polish
- T154-T156 (documentation) can run in parallel
- T157-T161 (error handling) can run in parallel
- T162-T164 (performance) can run in parallel
- T171-T174 (security) can run in parallel
- T175-T178 (code quality) can run in parallel

---

## Parallel Example: User Story 1

```bash
# Step 1: Launch all contract tests together (TDD - MUST FAIL):
Task T026: "Write contract test for WebSocketUpdate deserialization with BackendStatus type"
Task T027: "Write contract test for backend_status update message schema validation"
Task T028: "Write contract test verifying backend_status data includes all required fields"

# Step 2: Launch all integration tests together (TDD - MUST FAIL):
Task T029: "Write integration test for GET / dashboard endpoint returns 200 with HTML"
Task T030: "Write integration test for WebSocket /ws endpoint accepts connections"
Task T031: "Write integration test for WebSocket sends backend_status update on health change"
Task T032: "Write integration test for GET /assets/styles.css returns CSS with correct MIME type"

# Step 3: Launch all static assets together:
Task T033: "Create index.html in dashboard/"
Task T034: "Add backend status section to index.html"
Task T035: "Add connection status indicator to index.html"
Task T036: "Create styles.css in dashboard/"
Task T037: "Add dark mode color scheme to styles.css"
Task T038: "Add responsive grid styles for backend cards in styles.css"

# Step 4: Implement backend (sequential - dependencies exist):
Task T039: "Implement DashboardAssets struct..."
Task T040: "Implement dashboard_handler for GET /..."
# ... etc.
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete **Phase 1**: Setup
2. Complete **Phase 2**: Foundational (CRITICAL - blocks all stories)
3. Complete **Phase 3**: User Story 1 (Backend Health Monitoring)
4. **STOP and VALIDATE**: Test US1 independently with acceptance scenarios
5. Deploy/demo if ready - provides immediate operational value

**MVP Deliverable**: Real-time backend health dashboard with status indicators, metrics, and WebSocket updates

### Incremental Delivery (Recommended)

1. **Foundation** (Phase 1 + 2) → Foundation ready
2. **MVP Release** (+ Phase 3) → Backend health monitoring → Deploy/Demo
3. **v0.2.1** (+ Phase 4) → Model availability matrix → Deploy/Demo
4. **v0.2.2** (+ Phase 5) → Request history → Deploy/Demo
5. **v0.2.3** (+ Phase 6 + 7) → Full accessibility & mobile support → Deploy/Demo
6. **v0.2.4** (+ Phase 8) → Polish and optimization → Final release

Each increment adds value without breaking previous functionality.

### Parallel Team Strategy

With multiple developers after Phase 2 completes:

- **Developer A**: User Story 1 (Backend Health) - P1 priority
- **Developer B**: User Story 2 (Model Matrix) - P2 priority  
- **Developer C**: User Story 3 (Request History) - P3 priority

User Stories 1-3 are independently testable and can be developed in parallel after foundational phase.

User Stories 4-5 enhance US1-3 and should be done after those are complete.

---

## Notes

- **[P] tasks** = different files, no dependencies, can run in parallel
- **[Story] labels** map tasks to specific user stories for traceability
- **TDD workflow**: Write tests FIRST (red) → Implement (green) → Refactor (green)
- **Each user story** should be independently completable and testable
- **Verify tests fail** before implementing to confirm TDD workflow
- **Commit** after each task or logical group
- **Stop at checkpoints** to validate story independently
- **Binary size target**: <200KB increase for embedded assets
- **WebSocket fallback**: Automatic polling if WebSocket unavailable
- **Graceful degradation**: Works without JavaScript (US4)
- **Mobile-first**: Responsive design down to 320px (US5)

---

## Total Task Count: 183 tasks

- **Setup**: 5 tasks
- **Foundational**: 20 tasks (BLOCKS all user stories)
- **User Story 1 (P1)**: 44 tasks
- **User Story 2 (P2)**: 24 tasks
- **User Story 3 (P3)**: 30 tasks
- **User Story 4 (P4)**: 15 tasks
- **User Story 5 (P4)**: 15 tasks
- **Polish**: 30 tasks

**Parallelizable tasks**: ~60 tasks marked with [P]

**Suggested MVP**: Phases 1 + 2 + 3 (69 tasks) delivers backend health monitoring with real-time updates
