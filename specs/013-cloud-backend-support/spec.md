# Feature Specification: Cloud Backend Support with Nexus-Transparent Protocol

**Feature Branch**: `013-cloud-backend-support`  
**Created**: 2024  
**Status**: Draft  
**Input**: User description: "Register cloud LLM APIs (OpenAI, Anthropic, Google) as backends alongside local inference servers. Introduce the Nexus-Transparent Protocol: X-Nexus-* response headers that reveal routing decisions without modifying the OpenAI-compatible JSON response body."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Configure and Use Cloud Backend (Priority: P1)

As a Nexus operator, I need to register a cloud LLM provider (e.g., OpenAI GPT-4) as a backend so that requests can be routed to cloud services when local resources are unavailable or when higher-tier models are needed.

**Why this priority**: This is the core capability - without the ability to register and use cloud backends, the entire feature has no value. This enables basic cloud backend functionality and allows users to leverage powerful cloud models through Nexus.

**Independent Test**: Can be fully tested by adding a cloud backend configuration to nexus.toml, providing an API key, and sending a chat completion request that gets successfully routed to the cloud provider. Delivers immediate value by expanding available model capacity.

**Acceptance Scenarios**:

1. **Given** a valid TOML configuration with OpenAI backend details and `OPENAI_API_KEY` environment variable set, **When** Nexus starts, **Then** the OpenAI backend is registered and shows as healthy in the backend list
2. **Given** a registered OpenAI backend and a client request for `gpt-4`, **When** the router processes the request, **Then** the request is forwarded to OpenAI's API and the response is returned with `X-Nexus-Backend: openai-gpt4` header
3. **Given** a cloud backend configuration with `api_key_env = "OPENAI_API_KEY"`, **When** the environment variable is not set, **Then** the backend health check fails and logs an actionable error message indicating the missing API key

---

### User Story 2 - Observe Routing Decisions via Transparent Headers (Priority: P2)

As an API client consuming Nexus, I need visibility into which backend handled my request and why it was chosen, without any changes to the OpenAI-compatible response body, so that I can understand system behavior and debug routing issues.

**Why this priority**: Essential for production observability and debugging. The transparent protocol is what differentiates Nexus and maintains OpenAI compatibility while providing visibility. Must work after basic routing (P1) is functional.

**Independent Test**: Can be tested by sending requests under different conditions (local backend available, local backend saturated, privacy-sensitive data) and verifying that appropriate X-Nexus-* headers are present in all responses without modifying the response body JSON structure.

**Acceptance Scenarios**:

1. **Given** a request successfully routed to a cloud backend, **When** the response is returned, **Then** headers include `X-Nexus-Backend`, `X-Nexus-Backend-Type: cloud`, `X-Nexus-Route-Reason`, and `X-Nexus-Privacy-Zone: open`
2. **Given** a request routed to a cloud backend due to local capacity overflow, **When** the response is returned, **Then** `X-Nexus-Route-Reason: capacity-overflow` is present
3. **Given** a cloud backend request with estimated cost, **When** the response is returned, **Then** `X-Nexus-Cost-Estimated` header contains the estimated per-request cost in dollars
4. **Given** any proxied response, **When** comparing the response body to a direct OpenAI API call, **Then** the JSON structure is identical (only headers differ)

---

### User Story 3 - Handle Cloud API Translation (Priority: P3)

As a Nexus operator, I need requests to be automatically translated between OpenAI format and provider-specific formats (Anthropic, Google) so that I can use multiple cloud providers without changing my client integration.

**Why this priority**: Extends cloud backend support beyond OpenAI to other major providers. Depends on P1 (basic cloud backend) and follows the same transparent protocol patterns from P2. Less critical than core OpenAI support as OpenAI is most commonly used.

**Independent Test**: Can be tested by registering an Anthropic backend, sending an OpenAI-formatted chat completion request, and verifying that: (1) the request is successfully translated to Anthropic format, (2) the response is translated back to OpenAI format, (3) X-Nexus-* headers are present, and (4) both streaming and non-streaming modes work correctly.

**Acceptance Scenarios**:

1. **Given** an Anthropic backend configuration and a chat completion request in OpenAI format, **When** the request is routed to Anthropic, **Then** the message format is automatically translated (system/user/assistant roles), sent to Anthropic API, and the response is translated back to OpenAI format
2. **Given** an Anthropic backend and a streaming request, **When** the client requests streaming responses, **Then** Anthropic's streaming format is consumed and translated to OpenAI-compatible SSE events with proper data chunks
3. **Given** a Google AI backend configuration, **When** a chat completion request is routed to Google, **Then** request translation, response translation, and X-Nexus-* headers work correctly

---

### User Story 4 - Receive Actionable Error Responses (Priority: P2)

As an API client, when Nexus cannot fulfill my request, I need structured error information that tells me exactly what's wrong and what I can do about it, rather than generic error messages.

**Why this priority**: Critical for production reliability and developer experience. Without actionable errors, clients cannot programmatically handle failures or retry intelligently. Must work after basic routing (P1) is established.

**Independent Test**: Can be tested by creating scenarios where requests cannot be fulfilled (all backends down, insufficient tier, privacy mismatch) and verifying that 503 responses include the structured context object with required_tier, available_backends, and eta_seconds fields.

**Acceptance Scenarios**:

1. **Given** a request requiring tier 4 models and only tier 2 backends available, **When** the request cannot be fulfilled, **Then** a 503 response is returned with JSON body containing `{"error": {...}, "context": {"required_tier": 4, "available_backends": ["backend1"], "eta_seconds": null}}`
2. **Given** all backends are temporarily unavailable, **When** a request is received, **Then** the 503 response context includes `available_backends: []` and an estimated `eta_seconds` if health checks indicate recovery is imminent
3. **Given** a cloud backend API key is invalid or expired, **When** a request is routed to that backend, **Then** the error response clearly indicates the authentication failure and names the affected backend

---

### Edge Cases

- What happens when a cloud provider returns a non-200 status (rate limit, service error)? System should preserve the provider's error response, add X-Nexus-* headers, and return to client with appropriate HTTP status code
- What happens when an API key environment variable is set but empty? Backend health check should fail with clear error message indicating the empty API key
- What happens when a cloud provider's response format changes unexpectedly? System should log the translation failure, attempt to return the raw response with error headers, and mark the backend as unhealthy
- What happens during streaming when the connection to the cloud provider is lost mid-stream? System should emit an error SSE event and close the stream gracefully
- What happens when multiple cloud backends support the same model? Router uses standard priority/tier logic to select the backend, with no cloud-specific override
- What happens if cost estimation fails (e.g., cannot count tokens)? X-Nexus-Cost-Estimated header is omitted rather than returning an incorrect value
- What happens when a cloud backend request times out? Return 504 Gateway Timeout with X-Nexus-* headers indicating which backend timed out
- What happens when Anthropic or Google APIs change their authentication or request formats? Health checks should fail early rather than allowing malformed requests; version tracking in AgentProfile helps identify compatibility issues

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST support registering cloud backends (OpenAI, Anthropic, Google) via TOML configuration with fields: name, url, backend_type, api_key_env, zone, tier
- **FR-002**: System MUST read API keys exclusively from environment variables specified in `api_key_env` field; API keys MUST NOT be stored in configuration files
- **FR-003**: System MUST implement OpenAIAgent as an InferenceAgent that forwards chat_completion, embeddings, and health_check calls to OpenAI's API using Bearer token authentication
- **FR-004**: System MUST implement AnthropicAgent as an InferenceAgent that translates between OpenAI format and Anthropic's message format for both streaming and non-streaming requests
- **FR-005**: System MUST implement GoogleAIAgent as an InferenceAgent that translates between OpenAI format and Google's API format
- **FR-006**: System MUST add X-Nexus-Backend header to all proxied responses containing the backend name that handled the request
- **FR-007**: System MUST add X-Nexus-Backend-Type header with value "local" or "cloud" to indicate backend classification
- **FR-008**: System MUST add X-Nexus-Route-Reason header containing one of: capability-match, capacity-overflow, privacy-requirement, failover
- **FR-009**: System MUST add X-Nexus-Privacy-Zone header with value "restricted" or "open" indicating the privacy classification of the backend used
- **FR-010**: System MUST add X-Nexus-Cost-Estimated header for cloud backend responses, containing estimated per-request cost in dollars (e.g., "0.0025")
- **FR-011**: System MUST preserve OpenAI-compatible JSON response body structure; headers MUST be the only modification to responses
- **FR-012**: System MUST return 503 Service Unavailable responses with structured context object when requests cannot be fulfilled, including fields: required_tier (integer), available_backends (array of strings), eta_seconds (integer or null)
- **FR-013**: Cloud backends MUST set their AgentProfile with PrivacyZone::Open to indicate they process data outside restricted environments
- **FR-014**: System MUST perform health checks on cloud backends that verify API key validity and endpoint connectivity; failed health checks MUST mark backends as unavailable
- **FR-015**: OpenAIAgent MUST implement exact token counting using tiktoken-rs for accurate cost estimation
- **FR-016**: Cloud backends MUST participate in standard routing and failover logic without special-case treatment beyond privacy zone filtering
- **FR-017**: System MUST handle streaming responses from cloud providers by translating their SSE formats to OpenAI-compatible SSE events while preserving X-Nexus-* headers
- **FR-018**: System MUST log all cloud API interactions (excluding request/response bodies with potential PII) for observability and debugging
- **FR-019**: System MUST support both streaming and non-streaming modes for all cloud backends

### Key Entities

- **CloudBackendConfig**: TOML configuration representing a cloud backend with attributes: name (string), url (string), backend_type (enum: openai, anthropic, google), api_key_env (string), zone (enum: restricted, open), tier (integer 1-5). Extends existing BackendConfig structure.
- **CloudInferenceAgent**: Implementation of InferenceAgent trait for cloud providers, with specific subtypes: OpenAIAgent, AnthropicAgent, GoogleAIAgent. Contains API client, authentication credentials, and format translation logic.
- **NexusTransparentHeaders**: Standard set of response headers that reveal routing decisions: X-Nexus-Backend, X-Nexus-Backend-Type, X-Nexus-Route-Reason, X-Nexus-Cost-Estimated, X-Nexus-Privacy-Zone. Always present on proxied responses.
- **ActionableErrorContext**: Structured error context object returned in 503 responses with fields: required_tier (why request failed), available_backends (what options exist), eta_seconds (when service might recover).
- **APITranslator**: Component responsible for bidirectional translation between OpenAI format and provider-specific formats (Anthropic messages, Google AI format). Handles both streaming and non-streaming modes.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Cloud backends (OpenAI, Anthropic, Google) can be registered via TOML configuration and become operational within 5 seconds of Nexus startup when valid API keys are provided
- **SC-002**: All proxied responses include complete X-Nexus-* header set (5 headers: Backend, Backend-Type, Route-Reason, Privacy-Zone, and Cost-Estimated for cloud) with 100% consistency
- **SC-003**: OpenAI-compatible JSON response bodies remain byte-identical to direct provider responses (zero modifications to response body structure)
- **SC-004**: API format translation between OpenAI and Anthropic completes with zero data loss for all supported message types (system, user, assistant) in both streaming and non-streaming modes
- **SC-005**: 503 error responses include actionable context object 100% of the time when requests cannot be fulfilled, enabling clients to make informed retry decisions
- **SC-006**: Token counting accuracy for OpenAI models achieves 99%+ precision using tiktoken-rs, enabling cost estimates within $0.0001 of actual costs
- **SC-007**: Cloud backend health checks complete within 3 seconds and accurately detect invalid API keys, expired credentials, and connectivity issues
- **SC-008**: System handles cloud provider failures gracefully with failover to alternative backends completing within 2 seconds when fallback options exist
- **SC-009**: Streaming responses from cloud providers are translated and relayed with less than 100ms added latency per chunk
- **SC-010**: All cloud backend operations are fully observable through logs and headers, enabling diagnosis of routing decisions and backend selection within 30 seconds

### Assumptions

- Cloud provider APIs (OpenAI, Anthropic, Google) maintain stable authentication mechanisms (Bearer tokens, API keys) and don't introduce breaking changes to their request/response formats during this feature's development
- tiktoken-rs library provides accurate token counting for OpenAI models and is actively maintained
- API keys have sufficient permissions for both chat completion and embedding endpoints on their respective platforms
- Network connectivity to cloud providers is generally reliable; temporary outages are handled by failover but sustained outages are acceptable failure modes
- Cost estimation is based on token counts and published pricing; actual billing may vary slightly due to provider-specific calculations
- The existing InferenceAgent trait and AgentProfile structures are sufficient for cloud backends without requiring breaking changes
- PrivacyReconciler (F13) will control cloud backend access based on privacy zones; this feature focuses only on registration and transparent routing
- Clients can handle additional HTTP headers without breaking; X-Nexus-* headers use non-standard but widely compatible naming
- Rate limiting and quota management are handled by the cloud providers themselves; Nexus does not implement additional rate limiting for cloud backends in this phase
- The OpenAI-compatible format is well-defined and stable enough to serve as the canonical interchange format for all providers

### Dependencies

- **Existing InferenceAgent trait**: Cloud agents implement the existing trait without modifications (dependency: NII Phase 1 complete)
- **AgentProfile with PrivacyZone**: Already supports PrivacyZone::Open for cloud backends (dependency: NII Phase 1 complete)
- **Factory pattern (create_agent)**: Needs extension to support cloud backend types and API key loading from environment variables
- **BackendConfig structure**: Requires addition of `zone` and `tier` fields to support privacy filtering and capability routing
- **Response header injection**: Current response pipeline already supports header insertion; needs standardization for X-Nexus-* headers
- **RoutingResult**: Existing structure contains route_reason and backend information; needs cost estimation field
- **tiktoken-rs crate**: External dependency for accurate OpenAI token counting; must be added to Cargo.toml
- **HTTP client with streaming support**: Existing reqwest client must support both regular and streaming responses from cloud APIs

### Out of Scope

- Privacy policy enforcement and reconciliation (covered by F13: PrivacyReconciler)
- Cloud backend-specific rate limiting or quota management beyond what providers enforce
- Billing integration or cost tracking beyond per-request estimation in headers
- Support for cloud providers beyond OpenAI, Anthropic, and Google (additional providers can be added in future phases)
- Automatic API key rotation or secret management integration (operators manage keys via environment variables)
- Cloud-specific performance optimizations like persistent connections or request batching
- Retry logic with exponential backoff for cloud API failures (handled by standard failover)
- Token counting for non-OpenAI models (Anthropic, Google use approximate counting or provider estimates)
- Response caching for cloud requests to reduce costs
- Multi-region cloud backend configuration and geographic routing
- Custom request/response transformation beyond standard format translation
