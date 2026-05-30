# `oxllm` 🦀 (Oxide LLM Proxy)

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/Rust-1.85.1%2B-orange.svg)](https://www.rust-lang.org/)
[![CI](https://github.com/planetf1/oxllm/actions/workflows/ci.yml/badge.svg)](https://github.com/planetf1/oxllm/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/oxllm.svg)](https://crates.io/crates/oxllm)

`oxllm` (Oxide LLM Proxy) is an ultra-minimalist, high-resilience adaptive routing LLM gateway written in Rust. It exposes an OpenAI-compatible interface, proxying requests to a tiered fallback pool of LLM providers with automatic rate-limit detection, circuit breakers, and failover.

Built to operate entirely in memory with zero local disk persistence, `oxllm` is optimized for resource-constrained edge devices (like OpenWrt routers), developer workstations, and background daemons.

---

## 🚀 Key Features

* **Zero-Disk Dependency**: No SQLite, local caching, or file write operations during routing. State is strictly in memory.
* **Under &lt;2ms Routing Overhead**: Lock-free concurrency across routing loop, counters, and probe permits.
* **Adaptive Circuit Breaker**: Strict `HalfOpen` state machine with lock-free `probe_in_flight` atomic check-and-set. Rate limits and server errors trip per-provider circuits with exponential backoff. Idle-based penalty decay automatically rehabilitates providers.
* **Tiered Failover**: Configure fallback chains across multiple providers. If the primary returns 429 or 5xx, the proxy transparently cascades to the next.
* **Hot Config Reloading**: `SIGHUP` signal or `POST /reload` HTTP endpoint — parses updated `config.toml` and hot-swaps the provider pool via `tokio::sync::watch` without dropping connections.
* **Local Stats Dashboard**: Every provider tracks request count, success count, token volumes, and last request time via lock-free atomics. Query via `oxllm status` or `curl /status` — no external collector needed.
* **OOM-Proof Telemetry**: Bounded OTel event channel (1024 cap) with non-blocking `try_send` drops. If `otelite` is offline, telemetry degrades gracefully and the proxy keeps running.
* **W3C Trace Context Propagation**: Extracts and injects `traceparent` headers for continuous trace spans.
* **Dual-Stack IPv4/IPv6**: Configurable via `bind_family`: `"ipv4"` (default), `"ipv6"`, or `"dual"` for both.
* **Unix-Style Environment Expansion**: Shell-style `${VAR}` replacement in TOML config values.
* **Musl Cross-Compilation**: Pure-Rust `rustls-tls` stack avoids native OpenSSL linking on edge routers.

---

## 📦 Project Layout

```
oxllm/
├── Cargo.toml              # Workspace root
├── config.toml             # Multi-tier cloud provider config
├── config-local-test.toml  # Local-only Ollama config for testing
├── crates/
│   ├── oxllm-core/         # Core: config, circuit breaker, router, telemetry
│   └── oxllm/              # CLI: Axum server, routes, signal handling
├── docs/                   # Architecture & design docs
├── .github/workflows/      # CI, security, release, publish workflows
└── dist-workspace.toml     # cargo-dist release config
```

---

## 🛠️ Installation

### 1. Homebrew (easiest — pre-compiled binary)

```bash
brew tap planetf1/homebrew-tap
brew install oxllm
```

No Rust toolchain needed. Pre-compiled for macOS and Linux (aarch64 + x86_64).

### 2. Cargo (compiled from source)

```bash
cargo install oxllm
```

Builds from crates.io. Requires Rust 1.85.1+.

### 3. From source (latest main)

```bash
git clone https://github.com/planetf1/oxllm.git
cd oxllm
cargo build --release
cp config-local-test.toml my-config.toml
./target/release/oxllm serve --config my-config.toml
```

---

## 🚀 Quick Start

### Prerequisites

- Either a local [Ollama](https://ollama.com) instance (free, zero API keys), or cloud provider keys.
- If using Ollama: `ollama pull granite4:micro` for a tiny test model.

### 1. Create a config

```toml
[server]
host = "127.0.0.1"
port = 8080
otel_endpoint = "http://127.0.0.1:4318"
upstream_timeout_secs = 30

[[providers]]
name = "local-ollama"
enabled = true
base_url = "http://localhost:11434/v1/"
api_key = "ollama"
models = ["granite4:micro"]

[virtual_models]
default = [
  { provider = "local-ollama", model = "granite4:micro" },
]
```

### 2. Start the proxy

```bash
oxllm serve --config my-config.toml
```

### 3. Test it

```bash
# Chat
curl -X POST http://127.0.0.1:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "default", "messages": [{"role": "user", "content": "Hello"}]}'

# Embeddings
curl -X POST http://127.0.0.1:8080/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{"model": "default", "input": "hello world"}'

# Models, health, status
curl http://127.0.0.1:8080/v1/models
curl http://127.0.0.1:8080/health
curl http://127.0.0.1:8080/status
```

---

## ⚙️ Configuration

### Server Options

| Field | Default | Description |
|---|---|---|
| `host` | `127.0.0.1` | Bind address (not used when `bind_family` is `ipv6`/`dual`) |
| `port` | `8080` | Listen port |
| `otel_endpoint` | — | OTLP HTTP endpoint (e.g. `http://127.0.0.1:4318`). If unreachable, proxy starts without telemetry. |
| `upstream_timeout_secs` | `5` | Upstream request timeout |
| `bind_family` | `"ipv4"` | Address family: `"ipv4"`, `"ipv6"`, or `"dual"` (both families) |

### Provider Definition

Each provider requires a `name`, `enabled`, `base_url` (with trailing `/v1/`), `api_key` (or `${VAR}` env reference), and `models` list.

### Virtual Models

Virtual models define fallback chains. List providers in priority order — if one returns 429 or 5xx, the proxy tries the next:

```toml
[virtual_models]
smart = [
  { provider = "google-strong", model = "gemini-2.5-pro" },
  { provider = "groq-strong",   model = "llama-4-maverick" },
  { provider = "groq-basic",    model = "llama-4-scout" },
]
```

### Example Configs

- `config.toml` — multi-tier cloud config with 7 providers and 3 virtual models
- `config-local-test.toml` — local-only Ollama, zero API keys needed

---

## 📟 CLI Subcommands

```bash
# Start the proxy (use -v for per-request routing, -vv for trace)
oxllm serve --config config.toml
oxllm serve --config config.toml -v
oxllm serve --config config.toml -vv

# Validate config syntax and provider cross-references
oxllm validate --config config.toml

# Query provider status, counters, and virtual model routing table
oxllm status

# Gracefully stop the running daemon (sends SIGTERM)
oxllm stop

# Trigger config hot-reload via SIGHUP
oxllm reload
```

### `oxllm status` output

```
Uptime: 12m 34s  |  Total Requests: 47

Virtual Model: smart
+----------------------+--------------------------+-------------------------------+----------+----------+
| Provider             | Model                    | Circuit                       | Requests | Success  |
+----------------------+--------------------------+-------------------------------+----------+----------+
| google-strong        | gemini-2.5-pro           | Closed (Healthy)              |       45 |       43 |
| groq-strong          | llama-4-maverick         | Open (30s cooldown)           |        2 |        0 |
| google-basic         | gemini-2.5-flash         | Closed (Healthy)              |        2 |        2 |
+----------------------+--------------------------+-------------------------------+----------+----------+

Virtual Model: basic
+----------------------+--------------------------+-------------------------------+----------+----------+
| Provider             | Model                    | Circuit                       | Requests | Success  |
+----------------------+--------------------------+-------------------------------+----------+----------+
| groq-basic           | llama-4-scout            | Closed (Healthy)              |       12 |       12 |
| google-basic         | gemini-2.5-flash         | Closed (Healthy)              |        0 |        0 |
+----------------------+--------------------------+-------------------------------+----------+----------+

Per-Provider Details:
+------------------+-----------------------+----------+---------------+----------+-----------+--------------+---------------+-------------+
| Provider Name    | Circuit               | Failures | Rate Limited? | Requests | Successes | Tokens Input | Tokens Output | Last Req    |
+------------------+-----------------------+----------+---------------+----------+-----------+--------------+---------------+-------------+
| google-strong    | Closed (Healthy)      |        0 | No            |       45 |        43 |      14200   |         3200  | Just now    |
| groq-strong      | Open (30s cooldown)   |        3 | No            |        2 |         0 |        850   |          0   | 5m ago      |
| google-basic     | Closed (Healthy)      |        0 | No            |        2 |        2 |        600   |          150  | 3m ago      |
| groq-basic       | Closed (Healthy)      |        0 | No            |       12 |        12 |       3600   |          900  | 1m ago      |
+------------------+-----------------------+----------+---------------+----------+-----------+--------------+---------------+-------------+
```

---

## 🔬 API Endpoints

| Method | Path | Description | Auth |
|--------|------|-------------|------|
| `GET` | `/health` | Health check | Loopback |
| `GET` | `/status` | Provider + virtual model stats (counters, circuit state, last request time) | Loopback |
| `POST` | `/reload` | Trigger config hot-reload | Loopback |
| `GET` | `/v1/models` | List available virtual models | None |
| `POST` | `/v1/chat/completions` | Chat completion (JSON or SSE streaming) | None |
| `POST` | `/v1/embeddings` | Text embeddings | None |

All admin endpoints (`/health`, `/status`, `/reload`) are restricted to localhost (127.0.0.1) — external callers receive `403 Forbidden`.

---

## 🔬 Developer Guides

* **[Architecture & Design](docs/architecture.md)** — concurrency model, circuit breaker rules, telemetry pipeline
* **[Implementation Plan](implementation_plan.md)** — development roadmap and phase breakdown

## 📄 License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.
