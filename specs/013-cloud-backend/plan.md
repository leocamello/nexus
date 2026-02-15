# Implementation Plan: Cloud Backend Support with Nexus-Transparent Protocol

**Branch**: `013-cloud-backend` | **Date**: 2025-02-15 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/013-cloud-backend/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Enable Nexus to register cloud LLM APIs (OpenAI, Anthropic, Google) as backends alongside local inference servers. Introduce the Nexus-Transparent Protocol: X-Nexus-* response headers that reveal routing decisions (backend name, type, route reason, privacy zone, cost) without modifying the OpenAI-compatible JSON response body. Cloud backends participate in standard routing logic as overflow capacity when local backends reach limits. The feature enhances the existing OpenAIAgent, adds API translation layers for Anthropic and Google, and implements actionable 503 errors with structured context.

## Technical Context

**Language/Version**: Rust 1.75 (stable toolchain)  
**Primary Dependencies**: 
- Tokio (async runtime)
- Axum (HTTP framework)
- reqwest (HTTP client with connection pooling)
- serde/serde_json (serialization)
- async-trait (trait object support)
- tiktoken-rs (OpenAI token counting - to be integrated)

**Storage**: N/A (all state in-memory via DashMap)  
**Testing**: cargo test (unit + integration tests), property-based tests with proptest  
**Target Platform**: Linux, macOS, Windows (cross-platform via Rust)  
**Project Type**: Single binary server application  

**Performance Goals**: 
- Routing overhead: <5ms total (per Constitution)
- Token counting: <1ms for typical prompts
- Streaming responses: zero buffering (constant memory)
- Health check cycle: 30 seconds

**Constraints**: 
- API compatibility: 100% OpenAI-compatible responses (Constitution Principle III)
- Memory: <10KB per backend (Constitution target)
- No response body modification (headers only)
- Privacy zones: structural enforcement (Constitution Principle IX)

**Scale/Scope**: 
- Support 3 cloud providers initially (OpenAI, Anthropic, Google)
- ~10-15 new/modified source files
- Integration with existing routing, health check, and privacy systems

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Simplicity Gate
- [x] Using ≤3 main modules for initial implementation? 
  - YES: Core changes in 3 modules: agent/ (new cloud agents), api/ (header injection), config/ (zone/tier fields)
- [x] No speculative "might need" features?
  - YES: Only implementing P1/P2 features from spec. Cost-based routing, advanced rate limiting deferred
- [x] No premature optimization?
  - YES: Using existing reqwest client, standard async patterns. Token counting starts with tiktoken-rs
- [x] Start with simplest approach that could work?
  - YES: Enhance existing OpenAIAgent, add minimal API translation layer, use existing routing logic

### Anti-Abstraction Gate
- [x] Using Axum/Tokio/reqwest directly (no wrapper layers)?
  - YES: Direct use of reqwest for HTTP, Axum for response headers, no new abstractions
- [x] Single representation for each data type?
  - YES: Reusing existing InferenceAgent trait, ChatCompletionRequest/Response types
- [x] No "framework on top of framework" patterns?
  - YES: Cloud agents implement InferenceAgent directly, no intermediate abstractions
- [x] Abstractions justified by actual (not theoretical) needs?
  - YES: APITranslator needed for actual Anthropic/Google format differences (P3), not speculative

### Integration-First Gate
- [x] API contracts defined before implementation?
  - YES: X-Nexus-* headers documented in spec, OpenAI compatibility preserved
- [x] Integration tests planned with real/mock backends?
  - YES: Tests will use mock HTTP responses for cloud backends, verify header presence
- [x] End-to-end flow testable?
  - YES: Can test: config → agent registration → health check → routing → response headers

### Performance Gate
- [x] Routing decision target: < 1ms?
  - YES: No change to routing logic, header injection is <0.1ms overhead
- [x] Total overhead target: < 5ms?
  - YES: Header serialization negligible, token counting via tiktoken-rs is ~0.5ms typical
- [x] Memory baseline target: < 50MB?
  - YES: Cloud agents follow <10KB per backend target, no large state stored

**All gates pass. No violations to justify.**

## Project Structure

### Documentation (this feature)

```text
specs/013-cloud-backend/
├── spec.md              # Feature specification (already exists)
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
│   ├── nexus-headers.yaml          # X-Nexus-* header definitions (OpenAPI)
│   ├── actionable-error.json       # 503 context object schema
│   └── cloud-config.toml           # Backend configuration examples
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
src/
├── agent/
│   ├── mod.rs              # InferenceAgent trait (existing)
│   ├── openai.rs           # OpenAIAgent (existing - ENHANCE)
│   ├── anthropic.rs        # NEW: AnthropicAgent with API translation
│   ├── google.rs           # NEW: GoogleAgent with API translation
│   ├── translator.rs       # NEW: API format translation utilities
│   └── types.rs            # Agent types (existing - ADD TokenCount, cost)
│
├── config/
│   ├── backend.rs          # BackendConfig (existing - ADD zone, tier fields)
│   └── mod.rs              # Config loading (existing)
│
├── api/
│   ├── completions.rs      # Chat completion handlers (existing - ADD header injection)
│   ├── error.rs            # NEW: ActionableError types for 503
│   └── headers.rs          # NEW: X-Nexus-* header builder
│
├── routing/
│   └── mod.rs              # Routing logic (existing - already supports cloud backends)
│
└── health/
    └── mod.rs              # Health checker (existing - already supports cloud)

tests/
├── contract/
│   ├── test_nexus_headers.rs       # NEW: Verify header presence/format
│   └── test_actionable_errors.rs   # NEW: Verify 503 context structure
│
├── integration/
│   ├── test_cloud_backends.rs      # NEW: Mock cloud API responses
│   ├── test_api_translation.rs     # NEW: Anthropic/Google translation
│   └── test_streaming_headers.rs   # NEW: Headers in streaming responses
│
└── unit/
    ├── test_token_counting.rs      # NEW: tiktoken-rs integration
    └── test_cost_estimation.rs     # NEW: Cost calculation logic
```

**Structure Decision**: Single project structure (default). Cloud backend support integrates directly into existing agent/ and api/ modules. No new services or separate projects needed. API translation is localized to agent implementations. Header injection happens at the API response layer.

**Key Integration Points**:
1. **agent/openai.rs**: Enhance with tiktoken-rs token counting, cost estimation
2. **config/backend.rs**: Add zone (PrivacyZone) and tier (u8) fields to BackendConfig
3. **api/completions.rs**: Inject X-Nexus-* headers before returning responses
4. **api/error.rs**: Implement ActionableError with 503 context object

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

**No violations.** All constitution gates pass. This feature enhances existing infrastructure rather than adding new abstractions.
