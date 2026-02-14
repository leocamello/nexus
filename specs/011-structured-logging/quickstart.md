# Quickstart: Structured Request Logging

**Feature**: F11 Structured Request Logging  
**Target Audience**: Platform operators, DevOps engineers  
**Prerequisites**: Nexus v0.1.0+, understanding of structured logging concepts

## Overview

This guide shows you how to enable, configure, and query structured request logs in Nexus. Every request passing through Nexus produces a structured log entry with essential metadata, making it easy to monitor system health, diagnose issues, and understand usage patterns.

## Table of Contents

1. [Quick Start (5 minutes)](#quick-start)
2. [Configuration](#configuration)
3. [Log Output Examples](#log-output-examples)
4. [Querying Logs](#querying-logs)
5. [Integration with Log Aggregators](#integration-with-log-aggregators)
6. [Troubleshooting](#troubleshooting)

---

## Quick Start

### 1. Enable JSON Logging (Default: Pretty)

Edit your `nexus.toml`:

```toml
[logging]
level = "info"
format = "json"  # Switch from "pretty" to "json"
```

Restart Nexus:

```bash
nexus serve --config nexus.toml
```

### 2. Send a Test Request

```bash
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3:70b",
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": false
  }'
```

### 3. View Structured Logs

Check your terminal/console output for JSON log entries:

```json
{
  "timestamp": "2024-01-15T14:32:01.234Z",
  "level": "INFO",
  "target": "nexus::api::completions",
  "request_id": "550e8400-e29b-41d4-a716-446655440000",
  "model": "llama3:70b",
  "backend": "ollama-local",
  "backend_type": "local",
  "status": "success",
  "status_code": 200,
  "latency_ms": 1234,
  "tokens_prompt": 15,
  "tokens_completion": 8,
  "tokens_total": 23,
  "stream": false,
  "route_reason": "highest_score:0.95",
  "retry_count": 0
}
```

**Key Fields**:
- `request_id`: Unique ID that persists across retries/failovers
- `latency_ms`: Total request duration
- `tokens_*`: Token usage for cost tracking
- `route_reason`: Why this backend was selected

---

## Configuration

### Basic Configuration

```toml
[logging]
level = "info"       # Global log level: trace, debug, info, warn, error
format = "json"      # Output format: "json" or "pretty"
```

### Component-Level Filtering

Reduce log noise by setting different log levels for different Nexus components:

```toml
[logging]
level = "warn"  # Default: only warnings and errors
format = "json"

[logging.component_levels]
routing = "debug"     # Detailed routing decisions
api = "info"          # API request logs at info level
health = "warn"       # Health checks only on warnings
discovery = "info"    # Backend discovery events
```

This reduces log volume by 60-80% in production while keeping detailed logs for critical components.

### Debug Content Logging (Not Recommended for Production)

**⚠️ WARNING**: Enabling this option logs request message content, which may contain sensitive user data.

```toml
[logging]
level = "debug"
format = "json"
enable_content_logging = true  # Opt-in flag for debugging
```

When enabled, logs will include `prompt_preview` field (first 100 chars of user messages). Nexus will print a warning on startup:

```
⚠️  WARNING: Content logging enabled. Request/response data will be logged.
```

**Privacy Reminder**: By default, Nexus NEVER logs message content (FR-008). Only enable this for local development/debugging.

### Environment Variable Override

Override configuration without editing TOML files:

```bash
# Set global log level
export RUST_LOG=info

# Component-specific levels
export RUST_LOG=nexus::routing=debug,nexus::api=info,warn

# Enable JSON format
nexus serve --log-format json
```

---

## Log Output Examples

### Success (First Attempt)

```json
{
  "timestamp": "2024-01-15T14:32:01.234Z",
  "level": "INFO",
  "target": "nexus::api::completions",
  "request_id": "550e8400-e29b-41d4-a716-446655440000",
  "model": "gpt-4",
  "actual_model": "llama3:70b",
  "backend": "ollama-local",
  "backend_type": "local",
  "status": "success",
  "status_code": 200,
  "latency_ms": 1234,
  "tokens_prompt": 150,
  "tokens_completion": 85,
  "tokens_total": 235,
  "stream": false,
  "route_reason": "highest_score:0.95",
  "retry_count": 0,
  "fallback_chain": ""
}
```

### Retry with Fallback

```json
{
  "timestamp": "2024-01-15T14:32:05.678Z",
  "level": "WARN",
  "target": "nexus::api::completions",
  "request_id": "550e8400-e29b-41d4-a716-446655440000",
  "model": "gpt-4",
  "backend": "vllm-remote",
  "backend_type": "cloud",
  "status": "success",
  "status_code": 200,
  "latency_ms": 5432,
  "tokens_prompt": 150,
  "tokens_completion": 85,
  "tokens_total": 235,
  "stream": false,
  "route_reason": "fallback:primary_unhealthy",
  "retry_count": 2,
  "fallback_chain": "ollama-local,vllm-remote",
  "error_message": "Primary backend timeout after 30s"
}
```

**Note**: Same `request_id` as first attempt - this is the same logical request.

### Error (Model Not Found)

```json
{
  "timestamp": "2024-01-15T14:32:10.123Z",
  "level": "ERROR",
  "target": "nexus::api::completions",
  "request_id": "550e8400-e29b-41d4-a716-446655440001",
  "model": "unknown-model",
  "backend": "none",
  "status": "error",
  "status_code": 404,
  "latency_ms": 12,
  "stream": false,
  "retry_count": 0,
  "fallback_chain": "",
  "error_message": "Model 'unknown-model' not found. Available: llama3:70b, gpt-3.5-turbo"
}
```

---

## Querying Logs

### Using jq (JSON Processor)

#### Find all failed requests

```bash
nexus serve --log-format json 2>&1 | \
  jq 'select(.status == "error" or .status == "exhausted")'
```

#### Show requests by specific model

```bash
jq 'select(.model == "gpt-4")' logs/nexus.log
```

#### Calculate average latency per backend

```bash
jq -s 'group_by(.backend) | 
       map({backend: .[0].backend, avg_latency: (map(.latency_ms) | add / length)})' \
       logs/nexus.log
```

#### Trace a specific request by correlation ID

```bash
jq 'select(.request_id == "550e8400-e29b-41d4-a716-446655440000")' logs/nexus.log
```

This shows all retry attempts for a single logical request.

### Using grep (Quick Filtering)

```bash
# Find all requests with retries
grep '"retry_count":[1-9]' logs/nexus.log

# Find high-latency requests (>5 seconds)
grep -E '"latency_ms":[5-9][0-9]{3}' logs/nexus.log

# Find specific backend usage
grep '"backend":"ollama-local"' logs/nexus.log
```

---

## Integration with Log Aggregators

### Elasticsearch (ELK Stack)

**1. Install Filebeat**:

```yaml
# filebeat.yml
filebeat.inputs:
  - type: log
    enabled: true
    paths:
      - /var/log/nexus/*.log
    json.keys_under_root: true
    json.add_error_key: true

output.elasticsearch:
  hosts: ["localhost:9200"]
  index: "nexus-logs-%{+yyyy.MM.dd}"

setup.template.name: "nexus-logs"
setup.template.pattern: "nexus-logs-*"
```

**2. Start Filebeat**:

```bash
filebeat -e -c filebeat.yml
```

**3. Query in Kibana**:

```
status:error AND latency_ms:>5000
```

### Grafana Loki

**1. Install Promtail**:

```yaml
# promtail-config.yaml
server:
  http_listen_port: 9080

clients:
  - url: http://localhost:3100/loki/api/v1/push

scrape_configs:
  - job_name: nexus
    static_configs:
      - targets:
          - localhost
        labels:
          job: nexus
          __path__: /var/log/nexus/*.log
    pipeline_stages:
      - json:
          expressions:
            level: level
            request_id: request_id
            model: model
            backend: backend
```

**2. Start Promtail**:

```bash
promtail -config.file=promtail-config.yaml
```

**3. Query in Grafana**:

```logql
{job="nexus"} | json | latency_ms > 5000
```

### CloudWatch (AWS)

**1. Install CloudWatch Agent**:

```json
{
  "logs": {
    "logs_collected": {
      "files": {
        "collect_list": [
          {
            "file_path": "/var/log/nexus/nexus.log",
            "log_group_name": "/aws/nexus",
            "log_stream_name": "{instance_id}",
            "timestamp_format": "%Y-%m-%dT%H:%M:%S%.f%z"
          }
        ]
      }
    }
  }
}
```

**2. Query in CloudWatch Insights**:

```
fields @timestamp, request_id, model, backend, latency_ms
| filter status = "error"
| sort latency_ms desc
| limit 20
```

---

## Troubleshooting

### No Log Output

**Problem**: Nexus is running but no logs appear.

**Solution**:
1. Check log level is not set too high: `level = "info"` (not "error" or "off")
2. Verify logging is not disabled: `RUST_LOG` environment variable may override config
3. Check if logs are being redirected: `nexus serve 2>&1 | tee nexus.log`

### Logs Missing Fields

**Problem**: Log entries don't contain `tokens_prompt` or `backend_type`.

**Solution**: These are conditional fields:
- `tokens_*`: Only present when backend returns usage information
- `backend`: Only present when routing succeeds
- `actual_model`: Only present when different from requested model

This is expected behavior per FR-013 (sentinel values for N/A fields).

### High Logging Overhead

**Problem**: Logging adds >5ms latency per request.

**Solution**:
1. Use component-level filtering to reduce log volume:
   ```toml
   [logging]
   level = "warn"  # Only errors
   [logging.component_levels]
   routing = "debug"  # Exceptions for specific components
   ```
2. Disable debug content logging (should be off by default)
3. Consider sampling (log 1% of requests) - custom implementation needed

### Request Correlation Not Working

**Problem**: Can't trace a request through retry chain.

**Solution**: Filter by `request_id`:
```bash
jq 'select(.request_id == "YOUR-UUID-HERE")' logs/nexus.log
```

All retry/fallback attempts share the same `request_id` (FR-002).

### JSON Parsing Errors in Aggregator

**Problem**: Log entries not parsed correctly by Elasticsearch/Loki.

**Solution**:
1. Verify `format = "json"` in config (not "pretty")
2. Check logs are not mixed with non-JSON output (stderr vs stdout)
3. Ensure no ANSI color codes in JSON output (automatically disabled for JSON format)

---

## Performance Best Practices

1. **Use Metrics for High-Frequency Tracking**: Nexus already exports Prometheus metrics for counters/histograms. Use logs for exceptional events only.

2. **Component Filtering**: Set global level to `warn`, enable `debug` only for components under investigation:
   ```toml
   level = "warn"
   [logging.component_levels]
   routing = "debug"  # Temporarily for debugging
   ```

3. **Pipe to Log Aggregator**: Don't write logs to disk directly. Pipe stdout to aggregator or syslog:
   ```bash
   nexus serve | vector --config vector.toml
   ```

4. **Sampling (Custom)**: For ultra-high throughput (>50k RPS), implement custom sampling in code (not yet available).

---

## Next Steps

- **Metrics**: See Prometheus metrics at `http://localhost:8000/metrics`
- **Dashboard**: View request history at `http://localhost:8000/dashboard`
- **Advanced Routing**: Configure fallback chains and model aliases in `nexus.toml`

For complete field reference, see [data-model.md](./data-model.md).  
For JSON schema validation, see [contracts/log-schema.json](./contracts/log-schema.json).

---

**Last Updated**: 2025-02-14  
**Version**: 1.0 (Initial implementation)
