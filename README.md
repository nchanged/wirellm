# `oxllm` 🦀 (Oxide LLM Proxy)

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/Rust-1.85.1%2B-orange.svg)](https://www.rust-lang.org/)
[![CI](https://github.com/planetf1/oxllm/actions/workflows/ci.yml/badge.svg)](https://github.com/planetf1/oxllm/actions/workflows/ci.yml)

`oxllm` (Oxide LLM Proxy) is an ultra-minimalist, high-resilience adaptive routing LLM gateway written in Rust. It exposes an OpenAI-compatible interface, proxying requests to a tiered fallback pool of LLM providers with automatic rate-limit detection, circuit breakers, and failover.

Built to operate entirely in memory with zero local disk persistence, `oxllm` is optimized for resource-constrained edge devices (like OpenWrt routers), developer workstations, and background daemons.

---

## 🚀 Key Features

* **Zero-Disk Dependency**: No SQLite, local caching, or file write operations during routing. State is strictly in memory.
* **Under &lt;2ms Routing Overhead**: Designed using lock-free concurrency to avoid thread contention on your CPU's hot path.
* **Adaptive Circuit Breaker**: Strict `HalfOpen` state machine with lock-free `probe_in_flight` atomic check-and-set locks. Rate limits and server errors trip per-provider circuits with exponential backoff. Idle-based penalty decay automatically rehabilitates providers after periods of inactivity.
* **Tiered Failover**: Configure fallback chains across multiple providers and models. If the primary provider returns a 429 or 5xx, the proxy transparently tries the next in the chain.
* **Hot Config Reloading**: Unix `SIGHUP` signal listener that parses updated `config.toml` and hot-swaps the active provider pool via `tokio::sync::watch` without dropping connections.
* **Local Stats Dashboard**: Every provider tracks request count, success count, and token volumes via lock-free atomics. Query via `oxllm status` or `curl /status` — no external collector needed.
* **OOM-Proof Telemetry**: Bounded OTel event channel (1024 cap) with non-blocking `try_send` drops. If `otelite` is offline, telemetry degrades gracefully and the proxy keeps running.
* **W3C Trace Context Propagation**: Extracts and injects `traceparent` headers for continuous trace spans in collectors like [Otelite](https://github.com/planetf1/otelite).
* **Unix-Style Environment Expansion**: Shell-style `${VAR}` environment variable replacement in TOML config values.
* **Musl Cross-Compilation**: Pure-Rust `rustls-tls` stack avoids native OpenSSL linking issues on edge routers.

---

## 📦 Project Layout

```
oxllm/
├── Cargo.toml              # Workspace root
├── config.toml             # Example multi-tier provider config
├── config-local-test.toml  # Local-only Ollama config for testing
├── crates/
│   ├── oxllm-core/         # Core: config parsing, circuit breaker, router, telemetry
│   └── oxllm/              # CLI: Axum server, routes, signal handling
├── docs/                   # Architecture & design docs
├── .github/workflows/      # CI, security scans, release automation
└── dist-workspace.toml     # cargo-dist release config
```

---

## 🛠️ Quick Start

### Prerequisites

- Rust 1.85.1+
- At least one LLM provider API key, **or** [Ollama](https://ollama.com) running locally

### Install & Run

```bash
# Clone and build
git clone https://github.com/planetf1/oxllm.git
cd oxllm
cargo build --release

# Copy the example config and edit with your API keys
cp config.toml my-config.toml
# Set env vars for your keys:
export GOOGLE_AI_KEY="..."
export GROQ_KEY="..."
export OPENROUTER_KEY="..."

# Start the proxy
cargo run -- serve --config my-config.toml
```

Or install via Homebrew (once a release is published):
```bash
brew tap planetf1/homebrew-tap
brew install oxllm
oxllm serve --config /etc/oxllm/config.toml
```

### Test it

```bash
# Chat completion
curl -X POST http://127.0.0.1:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "smart", "messages": [{"role": "user", "content": "Hello"}]}'

# Embeddings
curl -X POST http://127.0.0.1:8080/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{"model": "basic", "input": "hello world"}'

# List models
curl http://127.0.0.1:8080/v1/models

# Health check
curl http://127.0.0.1:8080/health
```

---

## ⚙️ Configuration

### Multi-Tier Example (Cloud Providers)

The `config.toml` in this repo shows a 3-tier setup with 7 providers and 3 virtual models:

- **`smart`** — starts with powerful models (Gemini Pro, Llama Maverick, Grok) and cascades to basic if rate-limited
- **`basic`** — uses free/cheap models with generous rate limits (Gemini Flash, Llama Scout, DeepSeek R1)
- **Local fallback** — Ollama with a tiny model, always available, zero cost

```toml
[server]
host = "127.0.0.1"
port = 8080
otel_endpoint = "http://127.0.0.1:4318"
upstream_timeout_secs = 30

[[providers]]
name = "google-strong"
enabled = true
base_url = "https://generativelanguage.googleapis.com/v1beta/openai/"
api_key = "${GOOGLE_AI_KEY}"
models = ["gemini-2.5-pro"]

[[providers]]
name = "groq-basic"
enabled = true
base_url = "https://api.groq.com/openai/v1/"
api_key = "${GROQ_KEY}"
models = ["llama-4-scout"]

[virtual_models]
smart = [
  { provider = "google-strong", model = "gemini-2.5-pro" },
  { provider = "groq-basic",    model = "llama-4-scout" },
]
```

> **`base_url` convention**: Must end with a trailing slash. oxllm appends `chat/completions` and `embeddings` relative to this base.
>
> **Telemetry**: If `otel_endpoint` is unreachable, oxllm logs a warning and starts normally. Local request counters on `/status` always work regardless.

### Local-Only Config (Ollama)

See [`config-local-test.toml`](config-local-test.toml) for a zero-dependency local setup.

---

## 📟 CLI Subcommands

```bash
# Start the proxy server
oxllm serve --config config.toml

# Validate config syntax and provider cross-references
oxllm validate --config config.toml

# Query provider status and counters from running daemon
oxllm status

# Trigger config hot-reload (SIGHUP)
oxllm reload
```

The `status` command shows:
```
+--------------------+--------------------------------+----------+---------------+----------+----------+-------------+--------------+
| Provider Name      | Circuit Breaker State          | Failures | Rate Limited? | Requests | Success  | Tokens In   | Tokens Out   |
+--------------------+--------------------------------+----------+---------------+----------+----------+-------------+--------------+
| google-strong      | Closed (Healthy)               | 0        | No            | 47       | 45       | 14200       | 3200         |
| groq-strong        | Closed (Healthy)               | 0        | No            | 3        | 2        | 850         | 190          |
```

Uptime and total request count are shown at the top.

---

## 🔬 API Endpoints

| Method | Path | Description | Auth |
|--------|------|-------------|------|
| `GET` | `/health` | Health check (localhost-only) | Loopback |
| `GET` | `/status` | Provider circuit states + counters (localhost-only) | Loopback |
| `GET` | `/v1/models` | List available virtual models | None |
| `POST` | `/v1/chat/completions` | Chat completion (JSON or SSE streaming) | None |
| `POST` | `/v1/embeddings` | Text embeddings | None |

All admin endpoints (`/health`, `/status`) are restricted to localhost (127.0.0.1) — external callers receive `403 Forbidden`.

---

## 🔬 Developer Guides

* **[Architecture & Design](docs/architecture.md)** — concurrency model, circuit breaker rules, telemetry pipeline
* **[Implementation Plan](implementation_plan.md)** — development roadmap and phase breakdown

## 📄 License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.
