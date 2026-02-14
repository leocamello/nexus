# Quickstart: Health Checker

**Feature**: F02 Health Checker  
**Status**: ✅ Implemented  
**Prerequisites**: Rust 1.87+, Nexus codebase cloned, at least one LLM backend accessible

---

## Overview

The Health Checker is a background task that periodically probes registered backends to verify availability, measure latency, and discover available models. It uses backend-specific endpoints (e.g., `/api/tags` for Ollama, `/v1/models` for vLLM/OpenAI) and applies threshold-based state transitions to avoid flapping between healthy and unhealthy states.

This guide shows how to configure health checks, observe state transitions, and debug health issues.

---

## Development Setup

### 1. Build Nexus

```bash
cargo build
```

### 2. Create Config with Health Check Settings

```bash
cargo run -- config init
```

Edit `nexus.toml` to add a backend and tune health check settings:

```toml
[server]
port = 8000

[health_check]
enabled = true
interval_seconds = 30
timeout_seconds = 5

[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
priority = 1
```

### 3. Start a Test Backend

```bash
# Option A: Ollama
ollama serve

# Option B: Any OpenAI-compatible server
# (vLLM, LM Studio, llama.cpp with --api-server, etc.)
```

---

## Project Structure

```
nexus/
├── src/
│   ├── health/
│   │   ├── mod.rs              # HealthChecker — background loop, per-backend checks
│   │   ├── config.rs           # HealthCheckConfig — intervals, thresholds
│   │   ├── state.rs            # BackendHealthState, HealthCheckResult
│   │   ├── parser.rs           # Backend-specific response parsers (Ollama, vLLM, etc.)
│   │   ├── error.rs            # HealthCheckError types
│   │   └── tests.rs            # Unit tests
│   ├── registry/
│   │   ├── mod.rs              # Registry — updated by health checker
│   │   └── backend.rs          # BackendStatus enum (Healthy/Unhealthy/Unknown/Draining)
│   └── cli/
│       └── health.rs           # `nexus health` command
├── nexus.example.toml          # Example config with health_check section
└── tests/                      # Integration tests
```

---

## Configuration Reference

### Health Check Config (`nexus.toml`)

```toml
[health_check]
enabled = true              # Enable/disable health checks (default: true)
interval_seconds = 30       # Time between check cycles (default: 30)
timeout_seconds = 5         # HTTP timeout per backend check (default: 5)
# failure_threshold = 3     # Consecutive failures before marking unhealthy (default: 3)
# recovery_threshold = 2    # Consecutive successes before marking healthy (default: 2)
```

### Environment Variable Overrides

```bash
# Disable health checks entirely
NEXUS_HEALTH_CHECK=false cargo run -- serve

# Or via CLI flag
cargo run -- serve --no-health-check
```

### Health Check Endpoints by Backend Type

| Backend Type | Health Endpoint | What's Parsed |
|-------------|----------------|---------------|
| Ollama | `GET /api/tags` | Model list with names |
| vLLM | `GET /v1/models` | OpenAI-format model list |
| OpenAI | `GET /v1/models` | OpenAI-format model list |
| LM Studio | `GET /v1/models` | OpenAI-format model list |
| Exo | `GET /v1/models` | OpenAI-format model list |
| llama.cpp | `GET /health` | Status field (checks `"ok"`) |
| Generic | `GET /v1/models` | OpenAI-format model list |

---

## Usage Guide

### Step 1: Start Nexus and Observe Health Checks

```bash
# Start with debug logging to see health check activity
RUST_LOG=debug cargo run -- serve -c nexus.toml
```

Watch the logs for health check output:

```
DEBUG health: checking backend backend_id="local-ollama" url="http://localhost:11434"
DEBUG health: check result backend_id="local-ollama" status=Healthy latency_ms=12 models=3
INFO  health: backend status changed backend_id="local-ollama" old=Unknown new=Healthy
```

### Step 2: Check Health via CLI

```bash
# Table format
cargo run -- health

# Expected output:
# System Health: healthy
# Uptime: 2m 30s
# Version: 0.1.0
#
# Backends: 1 healthy / 0 unhealthy / 1 total
# Models: 3 available
#
# ┌──────────────┬────────┬────────┐
# │ Backend      │ Status │ Models │
# ├──────────────┼────────┼────────┤
# │ local-ollama │ ✓      │ 3      │
# └──────────────┴────────┴────────┘

# JSON format
cargo run -- health --json
```

### Step 3: Check Health via REST API

```bash
curl -s http://localhost:8000/health | jq .

# Expected:
# {
#   "status": "healthy",
#   "uptime_seconds": 150,
#   "backends": {
#     "total": 1,
#     "healthy": 1,
#     "unhealthy": 0
#   },
#   "models": 3
# }
```

**Status values:**
- `"healthy"` — all backends are healthy
- `"degraded"` — some backends are unhealthy
- `"unhealthy"` — no healthy backends

### Step 4: Observe State Transitions

The health checker uses thresholds to prevent flapping:

```
Unknown ──(2 consecutive successes)──▶ Healthy
Unknown ──(3 consecutive failures)───▶ Unhealthy
Healthy ──(3 consecutive failures)───▶ Unhealthy
Unhealthy ──(2 consecutive successes)──▶ Healthy
```

This means a backend won't flip to "unhealthy" on a single failed check — it needs 3 consecutive failures (configurable via `failure_threshold`).

---

## Manual Testing

### Test 1: Healthy Backend

Start Nexus with a reachable backend:

```bash
# Terminal 1: Start Ollama
ollama serve

# Terminal 2: Start Nexus
cargo run -- serve -c nexus.toml

# Terminal 3: Check health
curl -s http://localhost:8000/health | jq .status
# Expected: "healthy"
```

### Test 2: Backend Goes Down

Simulate a backend failure by stopping the backend:

```bash
# Terminal 1: Stop Ollama (Ctrl+C or kill the process)

# Terminal 3: Wait for health checks (default 30s interval)
# With failure_threshold=3, it takes 3 checks (~90s) to transition to unhealthy

# Check health during transition
curl -s http://localhost:8000/health | jq .
# After threshold: { "status": "degraded", ... "unhealthy": 1 }
```

To see transitions faster, use a shorter interval:

```toml
[health_check]
interval_seconds = 5
failure_threshold = 2
```

### Test 3: Backend Recovery

Restart the backend and observe recovery:

```bash
# Terminal 1: Restart Ollama
ollama serve

# Terminal 3: Wait for recovery_threshold (default 2) successes
# With interval_seconds=5, recovery takes ~10s

curl -s http://localhost:8000/health | jq .status
# Expected: "healthy" (after 2 consecutive successful checks)
```

### Test 4: Health Check Timeout

Add a backend with a very slow or unreachable URL:

```toml
[[backends]]
name = "slow-backend"
url = "http://10.255.255.1:11434"    # Non-routable IP — will timeout
type = "ollama"
priority = 99
```

```bash
# Start with short timeout to see timeouts quickly
RUST_LOG=debug cargo run -- serve -c nexus.toml
# Watch for: WARN health: check failed backend_id="slow-backend" error=Timeout(5)
```

### Test 5: Models Discovered via Health Check

The health checker populates the model list for each backend:

```bash
# Start Nexus with Ollama running
cargo run -- serve -c nexus.toml

# Wait for first health check cycle (up to 30s)
sleep 5

# Models should now be populated
cargo run -- models
curl -s http://localhost:8000/v1/models | jq '.data[].id'
```

### Test 6: Health Checks Disabled

```bash
# Via config
cat > /tmp/nexus-nohealth.toml << 'EOF'
[server]
port = 8003

[health_check]
enabled = false

[discovery]
enabled = false

[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
EOF

cargo run -- serve -c /tmp/nexus-nohealth.toml

# Backend stays in "Unknown" status forever
curl -s http://localhost:8003/health | jq .
# Expected: backends all have "unknown" status, models = 0

# Or disable via CLI flag
cargo run -- serve --no-health-check -c nexus.toml
```

### Test 7: Parse Error Handling

If a backend returns unexpected response format, the health checker preserves the previous model list:

```bash
# Run with debug logging
RUST_LOG=debug cargo run -- serve -c nexus.toml
# Watch for: WARN health: parse error, preserving previous models
```

### Test 8: Run Unit Tests

```bash
# All health checker tests
cargo test health::

# Specific test modules
cargo test health::tests::
```

---

## Debugging Tips

### Backend Stuck in "Unknown"

1. Health checks might be disabled:
   ```bash
   grep -A3 'health_check' nexus.toml
   ```
2. Verify the check interval hasn't passed yet — first check runs after `interval_seconds`.
3. Run with debug logging:
   ```bash
   RUST_LOG=debug cargo run -- serve
   ```

### Backend Always "Unhealthy"

1. Verify the backend URL is correct:
   ```bash
   # For Ollama
   curl -s http://localhost:11434/api/tags | jq .
   # For vLLM/OpenAI
   curl -s http://localhost:8000/v1/models | jq .
   # For llama.cpp
   curl -s http://localhost:8080/health | jq .
   ```

2. Check that `backend_type` matches the actual server:
   - Wrong type → wrong health endpoint → always fails

3. Check for DNS/network issues:
   ```bash
   RUST_LOG=debug cargo run -- serve 2>&1 | grep "check failed"
   # Look for: ConnectionFailed, DnsError, TlsError, HttpError, Timeout
   ```

### Health Check Error Types

| Error | Cause | Fix |
|-------|-------|-----|
| `Timeout(5)` | Backend didn't respond within timeout | Increase `timeout_seconds` or fix backend |
| `ConnectionFailed` | Can't connect to URL | Check URL, firewall, network |
| `DnsError` | Hostname resolution failed | Verify hostname or use IP |
| `TlsError` | SSL/TLS handshake failed | Check certificates |
| `HttpError(500)` | Backend returned error status | Check backend logs |
| `ParseError` | Response format unexpected | Verify backend type matches server |

### Latency EMA

The health checker updates latency using exponential moving average (α=0.2):

```
new_avg = (sample + 4 × old_avg) / 5
```

This smooths out spikes while still tracking trends. View current latency:

```bash
cargo run -- backends list
# The "Latency" column shows the EMA in ms
```

---

## Code Style

- `HealthChecker` runs as a `tokio::spawn` task with `CancellationToken` for shutdown
- Per-backend state uses `DashMap<String, BackendHealthState>` — no global mutex
- Health check results update the registry via `registry.update_status()` and `registry.update_models()`
- Parsers are pure functions — easy to unit test in isolation
- All errors are typed via `HealthCheckError` — no string-based error handling

---

## References

- **Feature Spec**: `specs/002-health-checker/spec.md`
- **Data Model**: `specs/002-health-checker/data-model.md`
- **Implementation Walkthrough**: `specs/002-health-checker/walkthrough.md`
- **Example Config**: `nexus.example.toml` (see `[health_check]` section)
- **Tokio CancellationToken**: https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html
