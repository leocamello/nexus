# Frequently Asked Questions

## General

### What is Nexus?

Nexus is a distributed LLM orchestrator — the control plane for heterogeneous LLM inference. It unifies local and cloud inference backends (Ollama, vLLM, llama.cpp, exo, LM Studio, OpenAI) behind a single OpenAI-compatible API gateway with mDNS auto-discovery, intelligent capability-aware routing, and privacy-zone enforcement.

### What is Nexus NOT?

- **Not an inference engine** — it routes to backends, doesn't run models
- **Not a GPU scheduler** — backends manage their own VRAM/compute
- **Not for training** — inference routing only
- **Not a model manager** — no model downloads, conversions, or storage
- **Not a session manager** — no KV-cache, no conversation state; clients own their history (OpenAI API contract)
- **Not distributed inference** — that's exo's job; Nexus routes TO exo

### How is Nexus different from LiteLLM?

LiteLLM is cloud-focused (Python, routes to cloud APIs). Nexus is local-first (Rust, single binary, mDNS auto-discovery) that can overflow to cloud when needed. Nexus also provides privacy zones and capability-aware routing that LiteLLM doesn't offer.

## Setup

### Do I need to configure anything?

No. If you have Ollama running locally, just run `nexus serve` and it auto-discovers your backends via mDNS. Configuration is optional for advanced features (cloud backends, privacy zones, budgets).

### What backends does Nexus support?

Ollama, LM Studio, vLLM, llama.cpp server, exo, and OpenAI. Ollama and exo support mDNS auto-discovery; others use static TOML configuration.

### Can I use Nexus with Claude Code / Continue.dev?

Yes. Point your AI coding tool to `http://localhost:8000` as the API base URL. Nexus is fully OpenAI-compatible.

## Architecture

### Is Nexus stateless?

Yes. Nexus routes individual requests — it doesn't maintain conversation history or KV-cache state. Clients own their conversation context, which is the standard OpenAI API contract.

### Does Nexus modify API responses?

Never. Nexus is strictly OpenAI-compatible. Routing metadata is exposed via `X-Nexus-*` response headers only — the JSON body is passed through untouched.

### How fast is routing?

The full reconciler pipeline (privacy, budget, tier, quality, scheduling) executes in under 100µs for typical deployments. Total request overhead is < 5ms.

## Privacy & Security

### How do privacy zones work?

Backends declare a zone ("restricted" or "open") in their TOML config. The PrivacyReconciler structurally prevents requests from reaching cloud backends when privacy-sensitive traffic policies are active. This is enforced at the routing layer, not by client headers.

### Are API keys stored safely?

API keys are never stored in config files. You specify `api_key_env = "OPENAI_API_KEY"` which tells Nexus to read the key from that environment variable at startup.

### What happens if a cloud backend's API key is missing?

Nexus logs a warning and skips that backend. The server continues running with the remaining backends (zero-config principle).

## Cost Management

### How does budget management work?

Configure `[routing.budget]` in your TOML with a monthly USD limit. Nexus estimates token costs using tiered tokenization (exact for OpenAI via tiktoken, heuristic for others) and enforces soft/hard spending limits.

### What happens when the budget is exceeded?

Depends on `hard_limit_action`: `block_cloud` stops routing to cloud backends (local still works), `block_all` rejects all requests with a 503 and actionable error context.

## See Also

- [Getting Started](getting-started.md) — setup and installation
- [REST API Reference](api/rest.md) — endpoint details
- [Architecture](architecture.md) — internals and design decisions
