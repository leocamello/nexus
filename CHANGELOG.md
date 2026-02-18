# Changelog

All notable changes to Nexus will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0] - 2026-02-18

### Added
- **Speculative Router (F15)**: Request analysis and candidate pre-filtering
  - `RequestAnalyzer` inspects payload to extract model, vision, tools, and context requirements
  - Alias resolution with max 3-level chaining and circular detection
  - Candidate population from registry model-to-backend index
  - Sub-millisecond request analysis (< 0.5ms p95)
- **Quality Tracking & Backend Profiling (F16)**: Rolling-window performance profiles
  - Per-model+backend statistics: error rate, TTFT, success rate (1h and 24h windows)
  - `QualityReconciler` deprioritizes degraded backends (error_rate > 20%, TTFT > 2000ms)
  - Background `quality_reconciliation_loop` computes metrics every 30s
  - `AgentSchedulingProfile` with error_rate_1h, avg_ttft_ms, success_rate_24h
  - Quality metrics exposed via Prometheus and `/v1/stats`
- **Embeddings API (F17)**: OpenAI-compatible embeddings endpoint
  - `POST /v1/embeddings` routes to capable backends through reconciler pipeline
  - Batch embedding support (multiple inputs in single request)
  - `OllamaAgent`: forwards to `/api/embed` with format translation
  - `OpenAIAgent` / `LMStudioAgent`: forwards to `/v1/embeddings`
  - Embedding capability tracked in registry model metadata
  - `X-Nexus-*` headers on all embedding responses
- **Request Queuing & Prioritization (F18)**: Bounded queue for busy backends
  - `RoutingDecision::Queue` variant in reconciler pipeline
  - Priority levels via `X-Nexus-Priority` header (1=critical, 5=best-effort)
  - Configurable max queue size and timeout (default: 100 items, 30s)
  - Queue drain loop re-runs reconciler pipeline for queued requests
  - Oldest low-priority requests dropped first when queue is full
  - Timeout produces actionable 503 with `eta_seconds`
  - Queue depth exposed via Prometheus (`nexus_queue_depth`) and `/v1/stats`
  - TOML configuration: `[queue]` section with enabled, max_size, default_timeout_seconds
- **Red Team Phase**: Added adversarial review phase to Feature Development Lifecycle
  - Architectural & pattern compliance checks
  - Adversarial attack vector analysis
  - Spec integrity verification
  - Test coverage audit
  - Four-tier verdict system (PASS, CONDITIONAL PASS, FAIL-IMPLEMENTATION, FAIL-SPECIFICATION)

### Changed
- Test coverage expanded from 76% to 89% (1490 tests, +612 from v0.3.0)
- Feature Development Lifecycle expanded to 5 phases (added Red Team Phase before Merge)
- Documentation restructured for consistency across all spec artifacts (F15-F18)
- Version bumped to 0.4.0

### Fixed
- TOCTOU race condition in queue `enqueue()` — replaced load+check with `compare_exchange` CAS loop
- Unbounded memory in quality tracking — capped request history to 10,000 entries
- `RwLock` panic potential in quality tracker — replaced with `DashMap` for lock-free access
- Dead TOML config for quality settings — wired `[quality]` config to reconciler

## [0.3.0] - 2026-02-17

### Added
- **Cloud Backend Support (F12)**: Register cloud LLM APIs alongside local inference servers
  - `AnthropicAgent`: Full Anthropic Claude API ↔ OpenAI format translation (streaming and non-streaming)
  - `GoogleAIAgent`: Google Gemini API ↔ OpenAI format translation (NDJSON and SSE streaming)
  - Cloud backends configured via TOML with `api_key_env` for secure credential management
  - API keys loaded from environment variables (never stored in config files)
  - Cloud backends participate in standard routing and failover
- **Nexus-Transparent Protocol**: X-Nexus-* response headers reveal routing decisions
  - `X-Nexus-Backend`: Backend name that handled the request
  - `X-Nexus-Backend-Type`: `local` or `cloud`
  - `X-Nexus-Route-Reason`: `capability-match`, `capacity-overflow`, or `privacy-requirement`
  - `X-Nexus-Cost-Estimated`: Per-request cost estimation (cloud backends)
  - `X-Nexus-Privacy-Zone`: `restricted` or `open`
- **Privacy Zones & Capability Tiers (F13)**: Structural enforcement of privacy boundaries and quality levels
  - Privacy zones: backends declare `zone = "restricted"` or `"open"` in TOML config
  - PrivacyReconciler enforces zone constraints — restricted traffic never routes to cloud backends
  - Capability tiers: backends declare `tier = 1..5` for quality level enforcement
  - TierReconciler prevents silent quality downgrades during failover
  - `X-Nexus-Strict` / `X-Nexus-Flexible` request headers for client-controlled tier enforcement
  - Strict mode (default): only same-or-higher tier backends accepted
  - Flexible mode: higher-tier substitution allowed, but never downgrades
  - Actionable 503 responses include `privacy_zone_required` and `required_tier` context
  - `X-Nexus-Rejection-Reasons` and `X-Nexus-Rejection-Details` response headers
  - Zero-config backward compatible: no policies → no filtering
- **Budget Management (F14)**: Token-aware cost tracking with automatic spend limits
  - Monthly budget configuration with hard and soft limits
  - Per-request cost estimation using model-specific pricing tables
  - BudgetReconciler enforces spending limits in the routing pipeline
  - Budget stats exposed via `/v1/stats` API (current spend, limit, utilization)
  - Background budget reconciliation loop with month rollover detection
- **Control Plane — Reconciler Pipeline (RFC-001 Phase 2)**: Trait-based pipeline replacing imperative routing
  - `Reconciler` trait with `name()` and `reconcile()` methods
  - `ReconcilerPipeline` executor with per-reconciler timing and metrics
  - `RoutingIntent` shared state object for pipeline data flow
  - `RoutingDecision` enum: Route, Queue, Reject with actionable context
  - **RequestAnalyzer**: Alias resolution (max 3 levels) and candidate population
  - **PrivacyReconciler**: Traffic policy matching via pre-compiled glob patterns
  - **BudgetReconciler**: Cost estimation and hard/soft limit enforcement
  - **TierReconciler**: Capability tier enforcement with strict and flexible modes
  - **QualityReconciler**: Pass-through placeholder for future quality metrics
  - **SchedulerReconciler**: Health/capability filtering, strategy-based scoring, budget-aware adjustment
  - Pipeline observability: `nexus_reconciler_duration_seconds`, `nexus_pipeline_duration_seconds`, `nexus_reconciler_exclusions_total` metrics
  - Pipeline tracing at `trace!` level for production-safe diagnostics
  - Routing benchmarks for full pipeline (<1ms p95) and RequestAnalyzer (<0.5ms)
- **NII Agent Abstraction (RFC-001 Phase 1)**: `InferenceAgent` trait with built-in agents for all backend types
  - `OllamaAgent`: health via `/api/tags`, model enrichment via `/api/show`, chat via OpenAI-compat endpoint
  - `OpenAIAgent`: cloud backend with Bearer auth support
  - `LMStudioAgent`: local OpenAI-compatible backend
  - `GenericOpenAIAgent`: covers vLLM, exo, llama.cpp, and other OpenAI-compatible backends
  - Agent factory: `create_agent()` maps `BackendType` to correct implementation
  - Forward-looking trait methods with safe defaults (embeddings, load_model, count_tokens)
- Registry dual storage: `Arc<dyn InferenceAgent>` alongside existing `Backend` struct
- Health checker delegates to `agent.health_check()` and `agent.list_models()`
- Completions handler delegates to `agent.chat_completion()` and `agent.chat_completion_stream()`
- Model capability detection via Ollama `/api/show` endpoint (real vision, tools, context_length)
- Name-based capability heuristics for all backends (gemma, llama4, pixtral, qwen, etc.)
- Documentation reorganized into industry-standard structure (getting-started, api/rest, api/websocket, roadmap, troubleshooting, faq)
- SECURITY.md, CODE_OF_CONDUCT.md, and CONTRIBUTING.md governance files
- Comprehensive test coverage: 1005 tests across 14 modules

### Changed
- `/v1/models` now returns per-backend entries with `owned_by` set to backend name (was deduplicated with `owned_by: "nexus"`)
- Test coverage improved to 76%+ (1005 tests)
- Version bumped to 0.3.0

### Fixed
- 128k context length detection in Ollama name heuristics (was matching "8k" before "128k")
- Missing API key env var no longer crashes server startup (skips backend with warning)
- Dashboard infinite loop in model matrix header cleanup
- Dashboard backend cards not rendering (missing `renderBackendCards()` implementation)
- Dashboard card flickering between stats polling and WebSocket updates
- Dashboard request history showing backend UUID instead of name
- Total request counter never incrementing
- Model stats returning empty

## [0.2.0] - 2026-02-14

### Added
- **F09: Request Metrics** — Prometheus metrics (`/metrics`) and JSON stats API (`/v1/stats`)
  - Per-backend request counts, latency histograms, pending request gauges
  - Aggregate request statistics (total, success, errors)
  - System uptime tracking
- **F10: Web Dashboard** — Real-time monitoring UI at `/`
  - Backend status cards with name, type, URL, health badge, and metrics
  - Model availability matrix with per-backend columns
  - Request history (last 100) with duration and status
  - WebSocket real-time updates with polling fallback
  - Embedded via `rust-embed` (no external dependencies)
  - Dark mode support
- **F11: Structured Request Logging** — JSON/pretty log output
  - Per-request structured fields (request_id, model, backend, latency, status)
  - Request correlation across retries and fallbacks
  - Component-level log configuration
  - Privacy-safe logging (opt-in content logging)
- Docker multi-arch builds (linux/amd64, linux/arm64)
- GitHub Releases and crates.io publishing CI
- Code coverage reporting via Codecov
- LM Studio backend support (`type = "lmstudio"`)
- Routing benchmarks and property-based tests for scoring logic

### Changed
- Test coverage improved from 76.9% to 81%+
- Version bumped to 0.2.0

## [0.1.0] - 2026-01-15

### Added
- **F02: Backend Registry** — DashMap-based concurrent backend/model storage
  - Thread-safe with atomic counters for pending requests, total requests, latency EMA
  - Model-to-backend index for fast lookup
- **F03: Health Checker** — Background health monitoring
  - Backend-specific health endpoints (Ollama `/api/tags`, OpenAI `/v1/models`, llama.cpp `/health`)
  - Configurable interval, timeout, failure/recovery thresholds
  - Automatic model discovery on health check success
- **F01: Core API Gateway** — OpenAI-compatible HTTP API
  - `POST /v1/chat/completions` (streaming and non-streaming)
  - `GET /v1/models` (model listing)
  - `GET /health` (system health)
  - SSE streaming with proper `data: [DONE]` termination
- **F04: CLI and Configuration** — Clap-based CLI with TOML config
  - Commands: `serve`, `backends`, `models`, `health`, `config`, `completions`
  - Config precedence: CLI args > env vars (`NEXUS_*`) > config file > defaults
- **F05: mDNS Discovery** — Zero-config backend discovery via `mdns-sd`
  - Service types: `_ollama._tcp.local`, `_llm._tcp.local`
  - Automatic registration and removal with grace period
- **F06: Intelligent Router** — Capability-aware request routing
  - Strategies: Smart (default), RoundRobin, PriorityOnly, Random
  - Scoring: weighted priority + load + latency
  - Capability matching: vision, tools, context window
- **F07: Model Aliases** — Map common names to local models
  - Up to 3-level alias chaining with circular detection
  - Configurable via `[routing.aliases]` in TOML
- **F08: Fallback Chains** — Automatic failover
  - Configurable fallback chains per model
  - `X-Nexus-Fallback-Model` response header for transparency

[Unreleased]: https://github.com/leocamello/nexus/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/leocamello/nexus/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/leocamello/nexus/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/leocamello/nexus/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/leocamello/nexus/releases/tag/v0.1.0
