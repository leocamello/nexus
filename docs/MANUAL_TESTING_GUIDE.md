# Nexus Manual Testing Guide

A hands-on walkthrough of every Nexus feature — from first install to real-time dashboard monitoring. Follow this guide to see exactly what Nexus can do.

## Prerequisites

Before testing, ensure you have:

1. **Rust 1.87+** toolchain installed (`cargo` available)
2. **At least one LLM backend** running (e.g., [Ollama](https://ollama.com))
3. **curl** for HTTP requests
4. **jq** for JSON formatting
5. **wscat** (optional) for WebSocket testing — `npm install -g wscat`

### Quick Setup

```bash
# Build Nexus
cargo build --release

# Add to PATH for convenience
alias nexus="./target/release/nexus"

# Verify installation
nexus --version
# nexus-orchestrator 0.2.0

# If using Ollama, ensure it's running with at least one model
ollama serve &              # or: systemctl start ollama
ollama pull llama3.2:latest # download a model if needed
ollama list                 # verify models are available
```

> **Tip**: For the best experience, open **two terminals** — one for the Nexus server and one for running commands.

---

## Table of Contents

| # | Feature | What You'll Test |
|---|---------|------------------|
| 1 | [CLI and Configuration](#1-cli-and-configuration-f04) | Config files, env vars, CLI overrides, shell completions |
| 2 | [Backend Registry](#2-backend-registry-f02) | Add/remove/list backends, dynamic management |
| 3 | [Health Checker](#3-health-checker-f03) | Health status, failure detection, recovery |
| 4 | [Core API Gateway](#4-core-api-gateway-f01) | Chat completions (streaming + non-streaming), models, errors |
| 5 | [mDNS Discovery](#5-mdns-discovery-f05) | Auto-discovery, grace period, fallback |
| 6 | [Intelligent Router](#6-intelligent-router-f06) | Smart routing, strategies, scoring |
| 7 | [Model Aliases](#7-model-aliases-f07) | Name mapping, alias chaining |
| 8 | [Fallback Chains](#8-fallback-chains-f08) | Automatic model failover |
| 9 | [Request Metrics](#9-request-metrics-f09) | Prometheus metrics, JSON stats |
| 10 | [Web Dashboard](#10-web-dashboard-f10) | Live UI, WebSocket updates |
| 11 | [Structured Logging](#11-structured-logging-f11) | JSON logs, component levels, correlation IDs |
| — | [E2E Test Script](#e2e-test-script) | Automated smoke test |
| — | [Troubleshooting](#troubleshooting) | Common issues and solutions |

---

## 1. CLI and Configuration (F04)

### 1.1 Generate Configuration File

```bash
nexus config init
cat nexus.toml
```

**Expected**: A `nexus.toml` file with all default sections:

```toml
[server]
host = "0.0.0.0"
port = 8000
request_timeout_seconds = 300

[discovery]
enabled = true

[health_check]
enabled = true
interval_seconds = 30

[routing]
strategy = "smart"

[logging]
level = "info"
format = "pretty"
```

### 1.2 Custom Output Path

```bash
nexus config init --output /tmp/my-nexus.toml
cat /tmp/my-nexus.toml
```

### 1.3 Environment Variable Overrides

```bash
# Override port via environment variable
NEXUS_PORT=9000 nexus serve &
SERVER_PID=$!
sleep 2

curl -s http://localhost:9000/health | jq .
# {"status":"healthy","uptime_seconds":2,"backends":{"total":0,...},"models":0}

kill $SERVER_PID
```

**Supported env vars**: `NEXUS_PORT`, `NEXUS_HOST`, `NEXUS_LOG_LEVEL`, `NEXUS_LOG_FORMAT`, `NEXUS_DISCOVERY`, `NEXUS_HEALTH_CHECK`.

### 1.4 CLI Overrides (Highest Priority)

```bash
nexus serve --port 9001 --host 127.0.0.1 &
SERVER_PID=$!
sleep 2

curl -s http://127.0.0.1:9001/health | jq .
kill $SERVER_PID
```

**Config precedence**: CLI args > env vars > config file > defaults.

### 1.5 Shell Completions

```bash
nexus completions bash > /tmp/nexus.bash
nexus completions zsh  > /tmp/nexus.zsh
nexus completions fish > /tmp/nexus.fish

# Install for current session (bash example)
source /tmp/nexus.bash
nexus <TAB><TAB>  # shows: serve, backends, models, health, config, completions
```

---

## 2. Backend Registry (F02)

### 2.1 Create Configuration with a Backend

```bash
cat > nexus.toml << 'EOF'
[server]
host = "0.0.0.0"
port = 8000

[discovery]
enabled = false

[health_check]
enabled = true
interval_seconds = 30

[logging]
level = "info"
format = "pretty"

[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
priority = 1
EOF
```

### 2.2 Start Server and List Backends

```bash
# Terminal 1: Start the server
nexus serve

# Terminal 2: Query backends
nexus backends list
```

**Expected**:
```
Backends:
  local-ollama (ollama)
    URL: http://localhost:11434
    Status: Healthy
    Priority: 1
    Models: llama3.2:latest, ...
```

### 2.3 JSON Output

```bash
nexus backends list --json | jq '.[0] | {name, status, models: [.models[].id]}'
```

### 2.4 Add Backend Dynamically

```bash
nexus backends add http://192.168.1.100:11434 --name gpu-server --backend-type ollama --priority 2
nexus backends list
```

### 2.5 Filter by Status

```bash
nexus backends list --status healthy
```

### 2.6 Remove Backend

```bash
nexus backends remove gpu-server
nexus backends list  # gpu-server is gone
```

---

## 3. Health Checker (F03)

### 3.1 CLI Health Check

```bash
nexus health
```

**Expected**:
```
System Health: Healthy

Backends:
  ✓ local-ollama (Healthy)
    Last check: 2s ago
    Response time: 45ms
```

### 3.2 HTTP Health Endpoint

```bash
curl -s http://localhost:8000/health | jq .
```

**Expected**:
```json
{
  "status": "healthy",
  "uptime_seconds": 42,
  "backends": {
    "total": 1,
    "healthy": 1,
    "unhealthy": 0
  },
  "models": 3
}
```

### 3.3 Simulate Backend Failure

```bash
# Add an unreachable backend
nexus backends add http://localhost:99999 --name dead-backend --backend-type generic

# Wait for health check cycle (30s default)
sleep 35

nexus health
```

**Expected**: System shows `Degraded` with `dead-backend` marked `Unhealthy`.

### 3.4 Cleanup

```bash
nexus backends remove dead-backend
```

---

## 4. Core API Gateway (F01)

### 4.1 List Models (OpenAI-Compatible)

```bash
curl -s http://localhost:8000/v1/models | jq '.data[] | {id, owned_by}'
```

**Expected**: OpenAI-format model list:
```json
{"id": "llama3.2:latest", "owned_by": "local-ollama"}
```

### 4.2 List Models via CLI

```bash
nexus models
nexus models --backend local-ollama  # filter by backend
```

### 4.3 Non-Streaming Chat Completion

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.2:latest",
    "messages": [
      {"role": "user", "content": "Say hello in exactly 3 words"}
    ]
  }' | jq .
```

**Expected**: Standard OpenAI response with `id`, `choices`, `usage`.

### 4.4 Streaming Chat Completion

```bash
curl -sN http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.2:latest",
    "messages": [{"role": "user", "content": "Count from 1 to 5"}],
    "stream": true
  }'
```

**Expected**: Server-Sent Events with `data: {...}` chunks ending in `data: [DONE]`.

### 4.5 Multi-Turn Conversation

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.2:latest",
    "messages": [
      {"role": "system", "content": "You are a helpful math tutor."},
      {"role": "user", "content": "What is 2+2?"},
      {"role": "assistant", "content": "2+2 equals 4."},
      {"role": "user", "content": "And if I add 3 more?"}
    ]
  }' | jq '.choices[0].message.content'
```

**Expected**: Context-aware response mentioning "7".

### 4.6 Temperature and Max Tokens

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.2:latest",
    "messages": [{"role": "user", "content": "What is 1+1?"}],
    "temperature": 0.0,
    "max_tokens": 10
  }' | jq '.choices[0].message.content'
```

### 4.7 Error Handling

```bash
# Invalid model
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "nonexistent", "messages": [{"role": "user", "content": "Hi"}]}' | jq .
# → {"error":{"message":"Model 'nonexistent' not found","type":"invalid_request_error","code":"model_not_found"}}

# Missing messages field
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "llama3.2:latest"}' | jq .

# Invalid JSON
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d 'not valid json' | jq .
```

All errors follow the OpenAI error format with `error.message`, `error.type`, `error.code`.

---

## 5. mDNS Discovery (F05)

> **Requires**: A local network with another machine running Ollama or an mDNS advertiser. For single-machine testing, use Avahi (Linux) or dns-sd (macOS).

### 5.1 Enable Discovery

```bash
cat > nexus.toml << 'EOF'
[server]
port = 8000

[discovery]
enabled = true
service_types = ["_ollama._tcp.local", "_llm._tcp.local"]
grace_period_seconds = 60

[logging]
level = "info"

[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
EOF
```

### 5.2 Watch Discovery in Action

```bash
# Start with debug logging to see discovery messages
RUST_LOG=debug nexus serve
```

**Expected logs**:
```
INFO  mDNS service daemon started
INFO  Browsing for mDNS service: _ollama._tcp.local.
```

### 5.3 Verify Discovered Backends

```bash
nexus backends list
```

Discovered backends appear with `[mdns]` source tag alongside `[static]` configured backends.

### 5.4 Disable Discovery

```bash
nexus serve --no-discovery
```

### 5.5 Single-Machine mDNS Test (Linux)

```bash
# Advertise a fake LLM service via Avahi
sudo apt install avahi-utils  # if needed
avahi-publish -s "Test LLM" _llm._tcp 8080 "type=generic" &
AVAHI_PID=$!

# Start Nexus — it should discover the advertised service
RUST_LOG=debug nexus serve &
sleep 10
nexus backends list

kill $AVAHI_PID
```

---

## 6. Intelligent Router (F06)

The router selects the best backend for each request using scoring based on priority, load, and latency.

### 6.1 Configure Routing Strategy

```bash
cat > nexus.toml << 'EOF'
[server]
port = 8000

[discovery]
enabled = false

[routing]
strategy = "smart"    # smart | round_robin | priority_only | random
max_retries = 2

[routing.weights]
priority = 50         # favor higher-priority backends
load = 30             # favor less-loaded backends
latency = 20          # favor lower-latency backends

[logging]
level = "info"

[[backends]]
name = "fast-gpu"
url = "http://localhost:11434"
type = "ollama"
priority = 1

# Uncomment if you have a second backend:
# [[backends]]
# name = "slow-cpu"
# url = "http://192.168.1.100:11434"
# type = "ollama"
# priority = 50
EOF
```

### 6.2 Observe Routing Decisions

```bash
# Start with debug logging to see scoring
RUST_LOG=nexus::routing=debug nexus serve
```

Make a request in another terminal:
```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.2:latest",
    "messages": [{"role": "user", "content": "Hi"}],
    "max_tokens": 5
  }' | jq .
```

**Expected log output** (server terminal):
```
DEBUG routing: Scoring backend fast-gpu: priority=99, load=100, latency=100, total=99
DEBUG routing: Selected backend: fast-gpu (score: 99, reason: highest_score)
```

### 6.3 Scoring Formula

The Smart strategy scores each backend:

```
score = (priority_score × 50 + load_score × 30 + latency_score × 20) / 100

priority_score = 100 − min(priority, 100)       # lower priority value = higher score
load_score     = 100 − min(pending_requests, 100)
latency_score  = 100 − min(avg_latency_ms/10, 100)
```

### 6.4 Test with Load

If you have multiple backends, send concurrent requests to see load balancing:

```bash
for i in $(seq 1 5); do
  curl -s http://localhost:8000/v1/chat/completions \
    -H "Content-Type: application/json" \
    -d '{"model":"llama3.2:latest","messages":[{"role":"user","content":"test"}],"max_tokens":5}' &
done
wait
```

Watch the server logs — the router distributes requests based on real-time load.

---

## 7. Model Aliases (F07)

Aliases let you use familiar model names (like `gpt-4`) that map to your local models.

### 7.1 Configure Aliases

```bash
cat > nexus.toml << 'EOF'
[server]
port = 8000

[discovery]
enabled = false

[routing]
strategy = "smart"

[routing.aliases]
"gpt-4" = "llama3.2:latest"
"gpt-3.5-turbo" = "llama3.2:latest"
"claude" = "llama3.2:latest"

[logging]
level = "info"

[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
EOF
```

Restart the server with this config.

### 7.2 Use an Alias

```bash
# Request using "gpt-4" — routed to llama3.2:latest
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4",
    "messages": [{"role": "user", "content": "What model are you?"}],
    "max_tokens": 20
  }' | jq '.model'
```

**Expected**: The response `model` field shows the resolved model name.

### 7.3 Alias Chaining (Max 3 Levels)

```toml
[routing.aliases]
"fast" = "gpt-4"
"gpt-4" = "llama3.2:latest"
# fast → gpt-4 → llama3.2:latest
```

---

## 8. Fallback Chains (F08)

When a requested model isn't available, Nexus tries fallback models automatically.

### 8.1 Configure Fallbacks

```bash
cat > nexus.toml << 'EOF'
[server]
port = 8000

[discovery]
enabled = false

[routing]
strategy = "smart"

[routing.aliases]
"gpt-4" = "llama3.2:latest"

[routing.fallbacks]
"unavailable-model" = ["llama3.2:latest"]

[logging]
level = "info"

[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
EOF
```

Restart the server.

### 8.2 Trigger a Fallback

```bash
curl -sD - http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "unavailable-model",
    "messages": [{"role": "user", "content": "Hello"}],
    "max_tokens": 10
  }' 2>&1 | grep -i "x-nexus\|HTTP/"
```

**Expected**: The response includes a fallback header:
```
HTTP/1.1 200 OK
x-nexus-fallback-model: llama3.2:latest
```

This tells the client that the original model wasn't available and a fallback was used.

---

## 9. Request Metrics (F09)

Nexus exposes Prometheus-compatible metrics and JSON statistics.

### 9.1 Prometheus Metrics

```bash
curl -s http://localhost:8000/metrics
```

**Expected** (Prometheus exposition format):
```
# HELP nexus_backends_total Total registered backends
# TYPE nexus_backends_total gauge
nexus_backends_total 1

# HELP nexus_backends_healthy Healthy backends count
# TYPE nexus_backends_healthy gauge
nexus_backends_healthy 1

# HELP nexus_models_available Unique models available
# TYPE nexus_models_available gauge
nexus_models_available 3

# HELP nexus_requests_total Total requests by model, backend, and status
# TYPE nexus_requests_total counter
nexus_requests_total{backend="local-ollama",model="llama3.2:latest",status="success"} 5

# HELP nexus_request_duration_seconds Request duration histogram
# TYPE nexus_request_duration_seconds histogram
nexus_request_duration_seconds_bucket{backend="local-ollama",model="llama3.2:latest",le="1"} 2
nexus_request_duration_seconds_bucket{backend="local-ollama",model="llama3.2:latest",le="5"} 4
nexus_request_duration_seconds_bucket{backend="local-ollama",model="llama3.2:latest",le="+Inf"} 5
```

### 9.2 JSON Stats

```bash
curl -s http://localhost:8000/v1/stats | jq .
```

**Expected**:
```json
{
  "uptime_seconds": 120,
  "requests": {
    "total": 5,
    "success": 5,
    "errors": 0
  },
  "backends": [
    {
      "id": "local-ollama",
      "requests": 5,
      "average_latency_ms": 1250.5,
      "pending": 0
    }
  ],
  "models": []
}
```

### 9.3 Request History

```bash
curl -s http://localhost:8000/v1/history | jq '.[0]'
```

**Expected**: Last 100 requests in a ring buffer:
```json
{
  "timestamp": 1739571600,
  "model": "llama3.2:latest",
  "backend_id": "local-ollama",
  "duration_ms": 1500,
  "status": "Success",
  "error_message": null
}
```

### 9.4 Generate Metrics Data

Make a few requests to populate metrics, then query:

```bash
# Send 3 requests
for i in 1 2 3; do
  curl -s http://localhost:8000/v1/chat/completions \
    -H "Content-Type: application/json" \
    -d '{"model":"llama3.2:latest","messages":[{"role":"user","content":"Say OK"}],"max_tokens":5}' > /dev/null
done

# Check metrics
curl -s http://localhost:8000/metrics | grep nexus_requests_total
# nexus_requests_total{backend="local-ollama",model="llama3.2:latest",status="success"} 3

# Check history
curl -s http://localhost:8000/v1/history | jq 'length'
# 3
```

### 9.5 Prometheus Scrape Config

To monitor Nexus with Prometheus, add to `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'nexus'
    static_configs:
      - targets: ['localhost:8000']
    metrics_path: '/metrics'
    scrape_interval: 15s
```

---

## 10. Web Dashboard (F10)

Nexus includes an embedded web dashboard — no external files needed.

### 10.1 Open the Dashboard

Open in your browser:

```
http://localhost:8000/
```

**What you'll see**:
- **System Summary**: Uptime, total requests, active backends, available models
- **Backend Status Grid**: Cards for each backend with health indicators (green/yellow/red)
- **Model Availability**: Which models are available on which backends
- **Request History Table**: Recent requests with model, backend, duration, status

### 10.2 Dashboard Features

| Feature | How to Test |
|---------|-------------|
| **Health indicators** | Backend cards show green (healthy), red (unhealthy), yellow (unknown) |
| **Real-time updates** | Make a request — it appears in the history table within seconds |
| **Dark mode** | Set your OS/browser to dark mode — the dashboard follows `prefers-color-scheme` |
| **Mobile responsive** | Resize your browser window or open on a phone |
| **Works without JS** | Disable JavaScript — the page still shows static data with a refresh button |

### 10.3 WebSocket Live Updates

The dashboard uses WebSocket for real-time updates. Test manually:

```bash
# Install wscat if needed: npm install -g wscat
wscat -c ws://localhost:8000/ws
```

**Messages you'll receive** (JSON):

Backend status broadcast:
```json
{"update_type":"BackendStatus","data":[{"id":"local-ollama","status":"Healthy",...}]}
```

After making a request — request completion event:
```json
{"update_type":"RequestComplete","data":{"model":"llama3.2:latest","backend_id":"local-ollama","duration_ms":1200,"status":"Success"}}
```

### 10.4 Screenshot-Worthy Demo

To get a good dashboard screenshot for social media:

```bash
# 1. Configure multiple backends (even if some are the same Ollama)
cat > nexus.toml << 'EOF'
[server]
port = 8000

[discovery]
enabled = false

[routing]
strategy = "smart"

[routing.aliases]
"gpt-4" = "llama3.2:latest"
"gpt-3.5-turbo" = "llama3.2:latest"

[logging]
level = "info"

[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
priority = 1
EOF

# 2. Start Nexus
nexus serve &
sleep 3

# 3. Generate some traffic for the history table
for i in $(seq 1 10); do
  curl -s http://localhost:8000/v1/chat/completions \
    -H "Content-Type: application/json" \
    -d "{\"model\":\"llama3.2:latest\",\"messages\":[{\"role\":\"user\",\"content\":\"Say hello #$i\"}],\"max_tokens\":10}" > /dev/null &
done
wait

# 4. Open http://localhost:8000/ in your browser and take a screenshot!
```

---

## 11. Structured Logging (F11)

Nexus provides structured, queryable logs for every request.

### 11.1 Pretty Format (Default)

```bash
nexus serve  # uses pretty format by default
```

Make a request — server logs show:
```
INFO  nexus::api: Request completed
  request_id=req-a1b2c3 model=llama3.2:latest backend=local-ollama
  latency_ms=1250 status=success stream=false
```

### 11.2 JSON Format (Production)

```bash
nexus serve --log-level info
```

Or configure in `nexus.toml`:
```toml
[logging]
level = "info"
format = "json"
```

**Expected JSON output** (one line per event):
```json
{"timestamp":"2026-02-14T22:00:00Z","level":"INFO","target":"nexus::api","message":"Request completed","request_id":"req-a1b2c3","model":"llama3.2:latest","backend":"local-ollama","latency_ms":1250}
```

JSON logs are compatible with ELK, Loki, Splunk, CloudWatch, and other log aggregators.

### 11.3 Component-Level Logging

Debug specific subsystems without flooding the console:

```toml
[logging]
level = "info"              # global baseline

[logging.component_levels]
routing = "debug"           # verbose routing decisions
api = "info"                # standard API logging
health = "warn"             # only health check failures
discovery = "debug"         # detailed mDNS activity
```

Or via environment:
```bash
NEXUS_LOG_LEVEL=info RUST_LOG=nexus::routing=debug nexus serve
```

### 11.4 Request Correlation IDs

Every request gets a unique ID that tracks through retries and fallbacks:

```bash
# Start with debug logging
RUST_LOG=debug nexus serve
```

Make a request and find its correlation ID in the logs:
```
DEBUG request_id=req-x7y8z9 Resolving alias: gpt-4 → llama3.2:latest
DEBUG request_id=req-x7y8z9 Scoring backend local-ollama: score=95
INFO  request_id=req-x7y8z9 Request completed: model=llama3.2:latest latency_ms=800
```

### 11.5 Privacy: Content Never Logged by Default

Message content is **never** logged unless explicitly opted in:

```toml
[logging]
# Only enable for local debugging — never in production!
enable_content_logging = true
```

Without this flag, logs contain model names, backends, and latency — but never the actual messages or responses.

---

## 12. Cloud Backend Support (F12)

F12 adds cloud LLM APIs (OpenAI, Anthropic, Google) as backends alongside local inference
servers, with X-Nexus-* response headers for routing transparency and actionable 503 errors.

### 12.1 Cloud Backend Configuration

Create a config with cloud backends:

```toml
# /tmp/nexus-cloud-test.toml
[server]
port = 8000

[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
priority = 50

[[backends]]
name = "openai-gpt4"
url = "https://api.openai.com"
type = "openai"
priority = 40
api_key_env = "OPENAI_API_KEY"
zone = "open"
tier = 4

[[backends]]
name = "anthropic-claude"
url = "https://api.anthropic.com"
type = "anthropic"
priority = 40
api_key_env = "ANTHROPIC_API_KEY"
zone = "open"
tier = 4
```

```bash
# Set API keys (required for cloud backends)
export OPENAI_API_KEY="sk-your-key"
export ANTHROPIC_API_KEY="sk-ant-your-key"

# Start with cloud config
nexus serve --config /tmp/nexus-cloud-test.toml
```

**Expected**: Server starts, cloud backends appear in health check:
```bash
curl -s http://localhost:8000/health | jq .
# Should show cloud backends with their health status
```

### 12.2 Transparent Protocol Headers

Send a request and inspect the X-Nexus-* headers:

```bash
curl -si http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-4", "messages": [{"role": "user", "content": "Say hello"}]}' \
  2>&1 | grep -i "x-nexus"
```

**Expected** (5 headers for cloud backends):
```
x-nexus-backend: openai-gpt4
x-nexus-backend-type: cloud
x-nexus-route-reason: capability-match
x-nexus-privacy-zone: open
x-nexus-cost-estimated: 0.0042
```

For local backends:
```bash
curl -si http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "llama3.2:latest", "messages": [{"role": "user", "content": "Say hello"}]}' \
  2>&1 | grep -i "x-nexus"
```

**Expected** (4 headers, no cost):
```
x-nexus-backend: local-ollama
x-nexus-backend-type: local
x-nexus-route-reason: capability-match
x-nexus-privacy-zone: restricted
```

### 12.3 Streaming with Transparent Headers

Headers are injected into the HTTP response (not SSE events):

```bash
curl -si --no-buffer http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-4", "messages": [{"role": "user", "content": "Say hello"}], "stream": true}' \
  2>&1 | head -20
```

**Expected**: X-Nexus-* headers in HTTP response headers, SSE chunks have no extra headers:
```
HTTP/1.1 200 OK
content-type: text/event-stream
x-nexus-backend: openai-gpt4
x-nexus-backend-type: cloud
x-nexus-route-reason: capability-match
x-nexus-privacy-zone: open

data: {"id":"chatcmpl-...","choices":[{"delta":{"content":"Hello"},...}]}
```

### 12.4 Actionable 503 Responses

When no backend can serve a model, the 503 error includes context:

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "nonexistent-model", "messages": [{"role": "user", "content": "Hi"}]}' | jq .
```

**Expected** (OpenAI-format error with context):
```json
{
  "error": {
    "message": "No backend available for model 'nonexistent-model'",
    "type": "server_error",
    "code": "model_not_found"
  }
}
```

### 12.5 Privacy Zone Verification

Verify cloud backends report `open` and local backends report `restricted`:

```bash
# List models — check which backends are available
curl -s http://localhost:8000/v1/models | jq '.data[] | {id, owned_by}'

# Send to each and check privacy zone header
for model in "gpt-4" "llama3.2:latest"; do
  echo "--- $model ---"
  curl -si http://localhost:8000/v1/chat/completions \
    -H "Content-Type: application/json" \
    -d "{\"model\": \"$model\", \"messages\": [{\"role\": \"user\", \"content\": \"Hi\"}]}" \
    2>&1 | grep "x-nexus-privacy-zone"
done
```

### 12.6 Cost Estimation

Cost is estimated from response usage data for cloud backends:

```bash
curl -si http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-4", "messages": [{"role": "user", "content": "Write a short poem"}]}' \
  2>&1 | grep -i "x-nexus-cost"
```

**Expected**: `x-nexus-cost-estimated: 0.XXXX` (4 decimal places, USD)

The cost is only present for cloud backends where the model is in the pricing table.
Local backends never have cost estimation headers.

### 12.7 Config Validation

Cloud backends without `api_key_env` should fail validation:

```toml
# Missing api_key_env — should fail
[[backends]]
name = "bad-openai"
url = "https://api.openai.com"
type = "openai"
```

```bash
nexus serve --config /tmp/bad-config.toml
# Expected: Error message about missing api_key_env
```

---

## 13. Control Plane — Reconciler Pipeline (Phase 2)

The Control Plane replaces the monolithic `Router::select_backend()` with a pipeline
of independent Reconcilers that annotate a shared `RoutingIntent`. This enables
privacy zones, budget management, and capability tier enforcement.

### 13.1 Zero-Config Behavior (Default)

Without any `[routing.policies]` configuration, the pipeline passes through all
requests unchanged — existing routing behavior is preserved.

```bash
# Start with standard config (no policies)
nexus serve &

# Request should route normally
curl -s http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "llama3:8b", "messages": [{"role": "user", "content": "hello"}]}' | jq .

# Expected: Normal response, no X-Nexus-Rejection-Reasons header
```

### 13.2 Privacy Zone Enforcement

Configure a traffic policy to restrict sensitive models to local backends only.

```toml
# nexus.toml
[[routing.policies]]
model_pattern = "llama3:*"
privacy = "restricted"

[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
backend_type = "ollama"

[[backends]]
name = "openai-gpt4"
url = "https://api.openai.com"
backend_type = "openai"
api_key_env = "OPENAI_API_KEY"
```

```bash
nexus serve --config nexus.toml &

# Request for llama3:8b should only route to local-ollama
curl -s http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "llama3:8b", "messages": [{"role": "user", "content": "test"}]}' \
  -v 2>&1 | grep -i "x-nexus"

# Expected: X-Nexus-Backend points to local-ollama, NOT openai-gpt4
```

### 13.3 Budget Management

Configure monthly spending limits for cloud backends.

```toml
# nexus.toml
[routing.budget]
monthly_limit = 50.0
soft_limit_percent = 80
hard_limit_action = "block_cloud"
```

```bash
nexus serve --config nexus.toml &

# At soft limit (80%): local backends are preferred
# At hard limit (100%): cloud backends are blocked entirely
# Check current spending via stats
curl -s http://localhost:3000/v1/stats | jq '.budget'
```

### 13.4 Capability Tier Enforcement

Prevent silent quality downgrades by requiring minimum capability tiers.

```toml
# nexus.toml
[[routing.policies]]
model_pattern = "gpt-4*"
min_tier = 3
```

```bash
# Backends below tier 3 will be excluded for gpt-4* requests
# If no backend meets the tier requirement, an actionable 503 is returned

curl -s http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-4", "messages": [{"role": "user", "content": "test"}]}' \
  -w "\nHTTP Status: %{http_code}\n"

# Expected: 503 with rejection_reasons if no tier-3+ backend available
```

### 13.5 Actionable 503 Responses

When no backend can serve a request, the 503 includes reasons from each reconciler.

```bash
# With all cloud backends excluded (privacy + no local fallback):
curl -s http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "nonexistent-model", "messages": [{"role": "user", "content": "test"}]}' | jq .

# Expected:
# {
#   "error": {
#     "message": "No backends available...",
#     "type": "service_unavailable",
#     "context": {
#       "rejection_reasons": [
#         {"agent_id": "...", "reconciler": "privacy", "reason": "...", "suggested_action": "..."}
#       ]
#     }
#   }
# }
```

### 13.6 Pipeline Performance

The entire reconciler pipeline must execute in under 1ms.

```bash
# Enable debug logging to see pipeline timing
RUST_LOG=nexus=debug nexus serve &

# Send requests and check logs for pipeline duration
curl -s http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "llama3:8b", "messages": [{"role": "user", "content": "test"}]}'

# Look for log entries like:
# "ReconcilerPipeline: completed in 0.15ms"
```

### Automated Tests

```bash
# Unit tests (reconciler modules)
cargo test routing::reconciler::   # 88+ reconciler tests

# Integration test (full pipeline)
cargo test --test reconciler_pipeline_test   # 6 pipeline tests

# All tests
cargo test   # 590+ tests
```

---

## E2E Test Script

An automated smoke test is available at `scripts/e2e-test.sh`. It validates all core functionality in one run.

### Run It

```bash
# Requires: Ollama running with at least one model
./scripts/e2e-test.sh
```

The script starts a Nexus server, runs through health checks, model listing, chat completions, metrics, stats, history, and dashboard endpoints, then cleans up.

See [scripts/e2e-test.sh](../scripts/e2e-test.sh) for the full source.

---

## Troubleshooting

### Server stops when running in background

```bash
# Option 1: Use two terminals (recommended)
# Terminal 1: nexus serve
# Terminal 2: curl commands

# Option 2: Detach from terminal
nohup nexus serve > nexus.log 2>&1 &

# Option 3: Use tmux
tmux new-session -d -s nexus 'nexus serve'
```

### Port already in use

```bash
lsof -i :8000
nexus serve --port 8001  # use a different port
```

### Backend shows Unhealthy

```bash
# Verify the backend is running
curl http://localhost:11434/api/tags  # Ollama
curl http://localhost:1234/v1/models  # LM Studio

# Check Nexus logs for details
RUST_LOG=debug nexus serve
```

### No models found

```bash
ollama list                   # verify models exist
ollama pull llama3.2:latest   # download one if needed
```

### Streaming output is buffered

```bash
# Use --no-buffer flag
curl --no-buffer -s http://localhost:8000/v1/chat/completions ...
```

### Dashboard not loading

```bash
# Verify the root endpoint works
curl -s http://localhost:8000/ | head -5
# Should return HTML: <!DOCTYPE html>...

# Check that assets load
curl -sI http://localhost:8000/assets/dashboard.js | head -1
# HTTP/1.1 200 OK
```

---

## Cleanup

```bash
# Stop Nexus server (if running in background)
kill $SERVER_PID 2>/dev/null

# Remove test config files
rm -f nexus.toml /tmp/nexus-test.toml /tmp/my-nexus.toml

# Remove shell completions
rm -f /tmp/nexus.{bash,zsh,fish}
```

---

## Feature Summary

| Feature | Key Tests | Pass Criteria |
|---------|-----------|---------------|
| F01: API Gateway | Chat completions, streaming, models, errors | OpenAI-compatible responses |
| F02: Registry | Add/remove/list backends | Backends tracked correctly |
| F03: Health | Health status, failure detection | Accurate status reporting |
| F04: CLI & Config | Config init, env vars, CLI overrides | Correct precedence |
| F05: mDNS | Auto-discovery, graceful fallback | Backends discovered automatically |
| F06: Router | Smart scoring, strategies | Requests routed to best backend |
| F07: Aliases | Name mapping, chaining | Aliases resolve transparently |
| F08: Fallbacks | Automatic failover | `x-nexus-fallback-model` header returned |
| F09: Metrics | `/metrics`, `/v1/stats`, `/v1/history` | Prometheus-compatible output |
| F10: Dashboard | Web UI, WebSocket, dark mode | Live updates in browser |
| F11: Logging | JSON format, component levels, correlation IDs | Structured, queryable logs |
| F12: Cloud Backends | Cloud config, transparent headers, cost, 503s | X-Nexus-* headers on all responses |
| Control Plane | Privacy zones, budget limits, tier enforcement, pipeline | < 1ms pipeline, actionable 503s |

**Automated test suite**: `cargo test` — **590+ tests**

For the full E2E smoke test: `./scripts/e2e-test.sh`
