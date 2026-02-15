# Feature Specification: Cloud Backend Support with Nexus-Transparent Protocol

**Feature Branch**: `013-cloud-backend`  
**Created**: 2025-02-15  
**Status**: Draft  
**Input**: User description: "Cloud Backend Support with Nexus-Transparent Protocol (F12) - Register cloud LLM APIs (OpenAI, Anthropic, Google) as backends alongside local inference servers. Introduce the Nexus-Transparent Protocol: X-Nexus-* response headers that reveal routing decisions without modifying the OpenAI-compatible JSON response body."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Configure Cloud Backend for Overflow Capacity (Priority: P1)

An infrastructure operator needs to configure OpenAI GPT-4 as a cloud backend to handle overflow traffic when local inference servers reach capacity. They add the backend configuration to the TOML file, set the API key via environment variable, and restart Nexus. Incoming requests are now automatically routed to the cloud backend when local capacity is exhausted, with routing decisions visible in response headers.

**Why this priority**: This is the core value proposition - enabling cloud backends as overflow capacity. Without this, the feature provides no value. It represents the minimum viable implementation that solves the primary use case.

**Independent Test**: Can be fully tested by configuring one cloud backend, sending requests that exceed local capacity, and verifying requests are routed to the cloud backend with appropriate X-Nexus-* headers in responses.

**Acceptance Scenarios**:

1. **Given** a Nexus instance with local backends at capacity, **When** a request arrives for a model available on both local and cloud backends, **Then** the request is routed to the cloud backend and response includes `X-Nexus-Backend: openai-gpt4` and `X-Nexus-Route-Reason: capacity-overflow`
2. **Given** a cloud backend configured with invalid API key, **When** Nexus starts up, **Then** health check fails and backend is marked unavailable
3. **Given** a valid cloud backend configuration, **When** environment variable for API key is missing, **Then** Nexus logs an error and does not register the backend

---

### User Story 2 - Transparent Routing Visibility (Priority: P2)

A developer debugging model behavior needs to understand which backend served their request and why. They examine the response headers and see X-Nexus-Backend indicating "openai-gpt4", X-Nexus-Backend-Type showing "cloud", and X-Nexus-Route-Reason explaining "capacity-overflow". This allows them to understand the routing decision without parsing or modifying the response body.

**Why this priority**: This enables observability and debugging, which is critical for production operations but doesn't provide value without the core routing functionality (P1). It's a key differentiator (the "Nexus-Transparent Protocol") but depends on P1 being implemented first.

**Independent Test**: Can be tested by sending requests through Nexus and validating that all X-Nexus-* headers are present and contain accurate information about routing decisions, backend types, and costs.

**Acceptance Scenarios**:

1. **Given** a request successfully routed to a cloud backend, **When** response is returned, **Then** headers include X-Nexus-Backend, X-Nexus-Backend-Type, X-Nexus-Route-Reason, X-Nexus-Privacy-Zone, and X-Nexus-Cost-Estimated
2. **Given** a request routed to a local backend, **When** response is returned, **Then** headers include all X-Nexus-* headers except X-Nexus-Cost-Estimated
3. **Given** a cloud request with streaming enabled, **When** chunks are streamed back, **Then** X-Nexus-* headers are included in the initial response before streaming begins

---

### User Story 3 - Multi-Provider Cloud Support (Priority: P3)

An operator wants to use Anthropic Claude for privacy-sensitive workloads and Google Gemini for cost optimization. They configure multiple cloud backends with different providers. Nexus automatically translates between each provider's native API format and the OpenAI-compatible format, allowing clients to use a single API interface while Nexus handles provider-specific translation.

**Why this priority**: Multi-provider support adds flexibility and vendor optionality but requires significant API translation work. The core value can be delivered with a single cloud provider (P1). This can be implemented incrementally (add Anthropic, then Google) as separate sub-features.

**Independent Test**: Can be tested by configuring Anthropic and Google backends, sending identical requests through Nexus, and verifying that responses are correctly translated to OpenAI format regardless of which cloud backend served the request.

**Acceptance Scenarios**:

1. **Given** an Anthropic backend configured, **When** a chat completion request is sent, **Then** Nexus translates the OpenAI format request to Anthropic message format, forwards it, and translates the response back to OpenAI format
2. **Given** a Google AI backend configured, **When** a streaming request is sent, **Then** Nexus translates streaming chunks from Google's format to OpenAI's SSE format with `data:` prefix
3. **Given** multiple cloud providers configured for the same model capability, **When** one provider fails, **Then** Nexus fails over to another cloud provider and includes `X-Nexus-Route-Reason: backend-failover`

---

### User Story 4 - Actionable Error Responses (Priority: P3)

A client application receives a 503 Service Unavailable response when all backends are at capacity. Instead of a generic error, the response includes a structured context object with `required_tier`, `available_backends` list, and `eta_seconds` for when capacity may be available. The client can use this information to implement intelligent retry logic or display meaningful error messages to end users.

**Why this priority**: Enhanced error handling improves user experience but is not essential for core functionality. The system can function with standard 503 errors, and this enhancement can be added after the basic routing is working.

**Independent Test**: Can be tested by saturating all backends and sending a request that cannot be routed, then validating the 503 response contains the context object with actionable information.

**Acceptance Scenarios**:

1. **Given** all backends at capacity for tier 4 models, **When** a request arrives for a tier 4 model, **Then** response is 503 with context object containing `required_tier: 4`, list of tier 4 backends with their status, and `eta_seconds` based on current request queue
2. **Given** no backends support a requested model, **When** request arrives, **Then** response is 503 with context indicating no backends match the capability requirement
3. **Given** privacy requirements exclude all available backends, **When** request arrives with privacy restrictions, **Then** response is 503 with context explaining privacy constraint mismatch

---

### Edge Cases

- What happens when a cloud API key expires mid-operation? System should detect authentication failures during health checks, mark the backend unhealthy, and route traffic to other available backends. Return 503 with context if no alternatives exist.
- How does the system handle cloud API rate limits? Cloud backends should return 429 responses which Nexus passes through with X-Nexus-* headers indicating the backend that hit the rate limit. Future enhancement could implement automatic backoff and failover.
- What happens when API translation fails due to unsupported features? Return 422 Unprocessable Entity with details about which provider-specific feature is not supported in the translation layer. Log translation errors for debugging.
- How does the system handle streaming failures mid-response? If a cloud backend connection drops during streaming, close the stream and log the error. Client will receive partial response. X-Nexus-* headers will already be sent in initial response.
- What happens when cost estimation is not available? X-Nexus-Cost-Estimated header is omitted if cost cannot be calculated (e.g., unknown model pricing). This is graceful degradation.
- How does the system handle cloud backends that become available/unavailable dynamically? Health checks run periodically and update backend availability status. Routing decisions use current health status at request time.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST support registration of cloud backends (OpenAI, Anthropic, Google) via TOML configuration with fields: name, url, backend_type, api_key_env, zone, tier
- **FR-002**: System MUST load cloud API keys from environment variables specified in configuration, never storing keys in configuration files
- **FR-003**: System MUST implement cloud backends as NII agents (OpenAIAgent, AnthropicAgent, GoogleAgent) conforming to the InferenceAgent trait
- **FR-004**: Cloud agents MUST implement all InferenceAgent trait methods: chat_completion, embeddings, count_tokens, health_check
- **FR-005**: System MUST verify cloud backend connectivity and API key validity during health checks
- **FR-006**: System MUST include X-Nexus-Backend header in all responses indicating which backend served the request
- **FR-007**: System MUST include X-Nexus-Backend-Type header with value "local" or "cloud"
- **FR-008**: System MUST include X-Nexus-Route-Reason header explaining routing decision with values: capability-match, capacity-overflow, privacy-requirement, or backend-failover
- **FR-009**: System MUST include X-Nexus-Privacy-Zone header indicating the privacy zone (restricted or open)
- **FR-010**: System MUST include X-Nexus-Cost-Estimated header for cloud backend requests with per-request cost estimate in USD
- **FR-011**: System MUST NOT modify the OpenAI-compatible JSON response body when adding X-Nexus-* headers (headers-only transparency)
- **FR-012**: System MUST translate Anthropic API format to/from OpenAI format for chat completion requests
- **FR-013**: System MUST translate Anthropic streaming format to/from OpenAI SSE format
- **FR-014**: System MUST translate Google AI API format to/from OpenAI format for chat completion requests
- **FR-015**: System MUST translate Google AI streaming format to/from OpenAI SSE format
- **FR-016**: System MUST return 503 Service Unavailable when no backend can satisfy a request
- **FR-017**: 503 responses MUST include structured context object with fields: required_tier (integer), available_backends (array of backend status objects), eta_seconds (integer, optional)
- **FR-018**: Cloud backends MUST participate in standard routing logic alongside local backends
- **FR-019**: Cloud backends MUST participate in failover logic when local backends are unavailable
- **FR-020**: System MUST use exact token counting via tiktoken-rs for OpenAI cloud backends
- **FR-021**: System MUST respect PrivacyReconciler decisions when routing to cloud backends (integration with F13)
- **FR-022**: System MUST log cloud backend routing decisions for observability
- **FR-023**: System MUST handle cloud API authentication failures by marking backend unhealthy and excluding it from routing

### Key Entities

- **CloudBackendConfig**: Configuration for a cloud backend including name, url, backend_type (enum: openai, anthropic, google), api_key_env (environment variable name), zone (privacy zone), tier (capability tier)
- **CloudAgent**: Implementation of InferenceAgent trait for cloud providers, containing API client, authentication credentials, model mappings, and translation logic
- **NexusTransparentHeaders**: Response header collection containing backend name, backend type, route reason, privacy zone, and cost estimate
- **ActionableErrorContext**: Structured error context for 503 responses containing required_tier, available_backends status array, and eta_seconds estimate
- **APITranslator**: Component responsible for translating between provider-specific formats (Anthropic, Google) and OpenAI-compatible format for requests and responses

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Cloud backends can be configured and registered through TOML configuration without code changes
- **SC-002**: All responses from cloud backends include complete set of X-Nexus-* headers with accurate information
- **SC-003**: OpenAI-compatible JSON response bodies remain unmodified when proxied through cloud backends (validated via JSON schema comparison)
- **SC-004**: Anthropic API translation achieves 100% compatibility for standard chat completion features (messages, roles, streaming)
- **SC-005**: Google AI API translation achieves 100% compatibility for standard chat completion features
- **SC-006**: Cloud backends participate in failover within 100ms when local backends are unavailable
- **SC-007**: 503 responses include actionable context objects that enable client retry logic
- **SC-008**: Cost estimates for OpenAI requests are accurate within 5% (validated against actual billing)
- **SC-009**: Cloud backend health checks detect authentication failures within one health check cycle (typically 30 seconds)
- **SC-010**: System handles streaming responses from all cloud providers without buffering entire responses (constant memory usage regardless of response size)
- **SC-011**: Routing decisions respect privacy zones 100% of the time (no restricted data sent to open zone backends)

## Dependencies

- RFC-001: NII Architecture - Cloud backends must implement InferenceAgent trait
- F13: Privacy-Aware Routing - PrivacyReconciler controls when cloud backends can receive traffic
- Constitution Principle III: OpenAI-Compatible Responses - Headers only, never modify response JSON
- Constitution Principle IX: Explicit Contracts - Actionable 503s with context object
- Constitution Principle X: Precise Measurement - Cost estimation in response headers

## Assumptions

- Cloud providers (OpenAI, Anthropic, Google) maintain stable API contracts for chat completions and embeddings
- API keys are managed securely at the infrastructure level via environment variables
- Cost estimation for OpenAI models can be calculated using token counts and published pricing (pricing data maintained in configuration)
- Health check intervals are configured system-wide (default 30 seconds) and apply to cloud backends
- Streaming responses use Server-Sent Events (SSE) format per OpenAI specification
- Token counting for non-OpenAI providers uses approximation methods (exact counting only for OpenAI via tiktoken-rs)
- Cloud provider rate limits are handled via HTTP 429 responses passed through to clients
- Retry logic for transient cloud failures is handled by clients, not Nexus (Nexus only does failover to alternative backends)
- Cost estimates are logged but not used for routing decisions in this feature (cost-based routing is a future enhancement)

## Out of Scope

The following are explicitly excluded from this feature:

- **Cost-based routing**: Selecting backends based on cost optimization (future enhancement)
- **Advanced rate limit handling**: Automatic backoff and retry for cloud provider rate limits (future enhancement)
- **Cost tracking and billing**: Aggregating and reporting cloud API costs (future enhancement)
- **Provider-specific optimizations**: Using provider-specific features not available in OpenAI format (e.g., Anthropic's prompt caching)
- **Dynamic pricing updates**: Fetching current pricing from cloud providers (assumes static pricing configuration)
- **Multi-region cloud routing**: Selecting cloud regions based on latency or data residency requirements
- **Cloud backend load balancing**: Distributing load across multiple instances of the same cloud provider
- **Request transformation for optimization**: Modifying requests to optimize for provider-specific features
