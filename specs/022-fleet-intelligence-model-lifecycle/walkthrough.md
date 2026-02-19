# Code Walkthrough: Fleet Intelligence & Model Lifecycle (F19/F20)

**Feature**: 022 — Fleet Intelligence and Model Lifecycle Management  
**Branch**: `022-fleet-intelligence-model-lifecycle`  
**Date**: 2026-02-10

---

## Overview

This feature adds two capabilities to Nexus:

1. **Model Lifecycle Management (F20)** — API-driven model load/unload/migrate across backends
2. **Fleet Intelligence (F19)** — Background analysis of request patterns to recommend pre-warming

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│  HTTP API Layer (src/api/lifecycle.rs)                   │
│  POST /v1/models/load     DELETE /v1/models/:id         │
│  POST /v1/models/migrate  GET /v1/fleet/recommendations │
└───────────┬───────────────────────────┬─────────────────┘
            │                           │
            ▼                           ▼
┌───────────────────────┐   ┌──────────────────────────┐
│  InferenceAgent trait  │   │  FleetReconciler          │
│  (src/agent/)          │   │  (src/routing/reconciler/ │
│  - load_model()        │   │   fleet.rs)               │
│  - unload_model()      │   │  - record_request()       │
│  - resource_usage()    │   │  - analyze()              │
└───────────┬────────────┘   │  - get_recommendations()  │
            │                └──────────────────────────┘
            ▼
┌───────────────────────┐   ┌──────────────────────────┐
│  Registry              │   │  LifecycleReconciler      │
│  (src/registry/)       │   │  (src/routing/reconciler/ │
│  - current_operation   │   │   lifecycle.rs)           │
│  - add_model_to_backend│   │  - Filters InProgress     │
│  - remove_model        │   │    backends from routing  │
└────────────────────────┘   └──────────────────────────┘
```

## Key Files

### New Files

| File | Purpose |
|------|---------|
| `src/api/lifecycle.rs` | HTTP handlers for lifecycle operations |
| `src/config/lifecycle.rs` | `LifecycleConfig` (timeout_ms, vram_headroom_percent) |
| `src/config/fleet.rs` | `FleetConfig` (enabled, min_sample_days, analysis_interval) |
| `src/routing/reconciler/fleet.rs` | FleetReconciler — pattern analysis and recommendations |
| `src/routing/reconciler/lifecycle.rs` | LifecycleReconciler — routing pipeline filter |
| `tests/lifecycle_api_test.rs` | API contract tests |
| `tests/ollama_lifecycle_test.rs` | OllamaAgent lifecycle tests |
| `tests/resource_usage_test.rs` | VRAM calculation tests |
| `docs/api/lifecycle.md` | API documentation |

### Modified Files

| File | Change |
|------|--------|
| `src/agent/types.rs` | Added LifecycleOperation, OperationType, ResourceUsage, PrewarmingRecommendation |
| `src/agent/ollama.rs` | Implemented load_model, unload_model, resource_usage |
| `src/api/mod.rs` | Added fleet_tracker to AppState, registered lifecycle routes |
| `src/api/completions.rs` | Fleet request tracking, Retry-After header |
| `src/registry/backend.rs` | Added current_operation field |
| `src/registry/mod.rs` | Added update_operation, add_model_to_backend, remove_model_from_backend |
| `src/routing/mod.rs` | Registered LifecycleReconciler in pipeline |
| `src/health/mod.rs` | Lifecycle operation timeout detection |
| `src/cli/serve.rs` | Fleet analysis background loop |
| `nexus.example.toml` | Added [lifecycle] and [fleet] sections |

## Design Decisions

### 1. FleetReconciler is NOT a Reconciler trait impl

The `FleetReconciler` runs as a **background analysis task** on a configurable interval, not in the per-request routing pipeline. This avoids adding latency to every request for pattern analysis that only needs to run periodically.

```rust
// src/cli/serve.rs — background loop
async fn fleet_analysis_loop(fleet_tracker: Arc<FleetReconciler>, cancel: CancellationToken) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(interval) => { fleet_tracker.analyze(); }
        }
    }
}
```

### 2. LifecycleReconciler position in pipeline

Placed **after RequestAnalyzer but before PrivacyReconciler** — early filtering prevents wasted work by downstream reconcilers on backends that can't serve requests anyway.

### 3. Migration source preservation

During migration (unload from A, load on B), the LifecycleReconciler does NOT filter the source backend. This ensures zero request drops — requests continue routing to A until B is ready and the operator manually unloads A.

### 4. Operator-managed completion

For v0.5, migration is a two-step manual process:
1. `POST /v1/models/migrate` → starts load on target
2. Operator monitors progress, then `DELETE /v1/models/{id}?backend_id=source` when ready

No auto-completion avoids risk of premature unloading.

### 5. Fleet intelligence storage

In-memory `DashMap<String, Vec<i64>>` maps model_id → unix timestamps. 30-day retention with periodic cleanup. No persistence — analysis restarts from scratch on Nexus restart.

### 6. Suggestion-first intelligence

Fleet intelligence only **recommends** pre-warming; it never auto-loads models. The admin reviews recommendations via `GET /v1/fleet/recommendations` and acts manually.

## Ollama API Integration

| Operation | Ollama Endpoint | Details |
|-----------|----------------|---------|
| Load model | `POST /api/pull` | Body: `{"name": "model_id"}` — streams NDJSON progress |
| Unload model | `POST /api/generate` | Body: `{"model": "id", "keep_alive": 0}` — immediate unload |
| Resource usage | `GET /api/ps` | Returns `{"models": [...]}` with `size_vram` per model |

## Fleet Intelligence Algorithm

1. **Record**: Every completed request → `record_request(model_id)`
2. **Analyze** (periodic): For each model with sufficient history:
   - Build 24-hour profile (hourly request counts)
   - Calculate trend (recent 7d vs previous 7d)
   - Compute confidence score (pattern_strength × 0.5 + sample_factor × 0.25 + days_factor × 0.25)
3. **Recommend**: Find eligible backends (healthy, no active ops, model not already loaded)
4. **Guard**: Never recommend unloading hot models or exceeding VRAM headroom

## Test Summary

| File | Tests | Coverage |
|------|-------|----------|
| `src/routing/reconciler/fleet.rs` | 15 | Pattern analysis, trends, confidence, hot model protection |
| `src/routing/reconciler/lifecycle.rs` | 11 | Routing filter, migration preservation, edge cases |
| `tests/lifecycle_api_test.rs` | 8+ | API contracts, error codes, response formats |
| `tests/ollama_lifecycle_test.rs` | 4+ | OllamaAgent lifecycle via mock Ollama |
| `tests/resource_usage_test.rs` | 6 | VRAM calculations, free bytes |

## Configuration

```toml
[lifecycle]
timeout_ms = 300000          # 5 minutes max for load operations
vram_headroom_percent = 20   # Keep 20% VRAM free

[fleet]
enabled = false              # Disabled by default
min_sample_days = 3          # Minimum data before recommending
min_request_count = 10       # Minimum requests per model
analysis_interval_seconds = 3600  # Analyze every hour
max_recommendations = 10     # Cap recommendations per cycle
```

## Known Limitations

1. **No real Ollama testing** — All tests use mock HTTP; T092 deferred
2. **In-memory only** — Fleet history lost on restart
3. **No auto-completion for migrations** — Operator must manually unload source
4. **Ollama-only lifecycle** — Other backends return `AgentError::Unsupported`
5. **No auth on lifecycle API** — Planned for v0.5 Multi-Tenant (F21)
