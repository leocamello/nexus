# Changelog

All notable changes to Nexus will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Model capability detection via Ollama `/api/show` endpoint (real vision, tools, context_length)
- Name-based capability heuristics for all backends (gemma, llama4, pixtral, qwen, etc.)
- CHANGELOG.md

### Changed
- `/v1/models` now returns per-backend entries with `owned_by` set to backend name (was deduplicated with `owned_by: "nexus"`)

### Fixed
- Dashboard infinite loop in model matrix header cleanup (`querySelectorAll` returns static NodeList)
- Dashboard backend cards not rendering (missing `renderBackendCards()` implementation)
- Dashboard card flickering between stats polling and WebSocket updates
- Dashboard request history showing backend UUID instead of name
- Total request counter never incrementing (`increment_total_requests()` was not called)
- Model stats returning empty (stub `compute_model_stats()` implementation)

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

[Unreleased]: https://github.com/leocamello/nexus/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/leocamello/nexus/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/leocamello/nexus/releases/tag/v0.1.0
