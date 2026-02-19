# Model Lifecycle & Fleet Intelligence API

## Overview

Nexus provides model lifecycle management (F20) and fleet intelligence (F19) for
controlling model placement across your inference fleet.

## Model Lifecycle Endpoints

### Load Model

```
POST /v1/models/load
```

Trigger model loading on a specific backend.

**Request:**
```json
{
  "model_id": "llama3:8b",
  "backend_id": "ollama-gpu-01"
}
```

**Response (202 Accepted):**
```json
{
  "operation_id": "op-abc123",
  "model_id": "llama3:8b",
  "backend_id": "ollama-gpu-01",
  "status": "in_progress"
}
```

**Headers:**
- `X-Nexus-Lifecycle-Status: loading`
- `X-Nexus-Lifecycle-Operation: op-abc123`

**Errors:**
- `400` — Insufficient VRAM
- `404` — Backend not found
- `409` — Concurrent operation already in progress

### Unload Model

```
DELETE /v1/models/{model_id}?backend_id={backend_id}
```

Unload a model from a specific backend, freeing VRAM.

**Response (200 OK):**
```json
{
  "operation_id": "op-unload-xyz",
  "model_id": "llama3:8b",
  "backend_id": "ollama-gpu-01",
  "status": "completed",
  "vram_free_bytes": 8000000000,
  "vram_free_gb": 8
}
```

**Headers:**
- `X-Nexus-Lifecycle-Status: completed`
- `X-Nexus-Lifecycle-Operation: op-unload-xyz`

**Errors:**
- `400` — Model not loaded on backend
- `404` — Backend not found
- `409` — Active inference requests in progress

### Migrate Model

```
POST /v1/models/migrate
```

Migrate a model from one backend to another with zero request drops.

**Request:**
```json
{
  "model_id": "llama3:8b",
  "source_backend_id": "ollama-gpu-01",
  "target_backend_id": "ollama-gpu-02"
}
```

**Response (202 Accepted):**
```json
{
  "operation_id": "op-migrate-def456",
  "model_id": "llama3:8b",
  "source_backend_id": "ollama-gpu-01",
  "target_backend_id": "ollama-gpu-02",
  "status": "in_progress",
  "message": "Migration initiated. Target backend is loading the model. Source backend will continue serving requests."
}
```

**Migration behavior:**
- Source backend continues serving requests during migration
- Target backend enters Loading state (excluded from routing)
- After target loads successfully, operator unloads from source

## Fleet Intelligence

### Get Recommendations

```
GET /v1/fleet/recommendations
```

Returns advisory pre-warming recommendations based on request pattern analysis.

**Response (200 OK):**
```json
{
  "recommendations": [
    {
      "recommendation_id": "rec-abc123",
      "model_id": "llama3:8b",
      "target_backend_ids": ["ollama-gpu-02"],
      "confidence_score": 0.85,
      "reasoning": "Time-of-day pattern detected: peak at 9:00 UTC (3.2x average). Based on 1500 requests over 14 days",
      "vram_required_bytes": null,
      "generated_at": "2025-01-20T08:30:00Z",
      "expires_at": "2025-01-20T09:30:00Z"
    }
  ],
  "fleet_enabled": true,
  "generated_at": "2025-01-20T08:45:00Z"
}
```

**Note:** Recommendations are advisory-only. They never auto-execute.
To act on a recommendation, use the load model endpoint.

## Configuration

```toml
[lifecycle]
timeout_ms = 300000            # 5 min timeout for hung operations
vram_headroom_percent = 20     # Minimum free VRAM required
vram_buffer_percent = 10       # Additional buffer for VRAM calculations

[fleet]
enabled = true                 # Enable fleet intelligence
min_sample_days = 7            # Minimum days of data for analysis
min_request_count = 100        # Minimum requests before analysis
analysis_interval_seconds = 3600  # How often to run analysis (1 hour)
max_recommendations = 5        # Maximum recommendations per cycle
```

## Routing Behavior

When a model is being loaded on a backend:
- The backend is excluded from routing (LifecycleReconciler)
- If all backends are loading, requests get 503 with `Retry-After` header
- Migration sources continue serving (zero request drops)

## Response Headers

All lifecycle endpoints include:
- `X-Nexus-Lifecycle-Status` — Current operation status (`loading`, `migrating`, `completed`)
- `X-Nexus-Lifecycle-Operation` — Operation ID for tracking
