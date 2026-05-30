# `oxllm` 🦀 (Oxide LLM Proxy)

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/Rust-1.85.1%2B-orange.svg)](https://www.rust-lang.org/)
[![CI](https://github.com/planetf1/oxllm/actions/workflows/ci.yml/badge.svg)](https://github.com/planetf1/oxllm/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/oxllm.svg)](https://crates.io/crates/oxllm)

`oxllm` (Oxide LLM Proxy) is an ultra-minimalist, high-resilience adaptive routing LLM gateway written in Rust. It exposes an OpenAI-compatible interface, proxying requests to a tiered fallback pool of LLM providers with automatic rate-limit detection, circuit breakers, and failover.

Built to operate entirely in memory with zero local disk persistence, `oxllm` is optimized for resource-constrained edge devices (like OpenWrt routers), developer workstations, and background daemons. The **stripped release binary is ~2.6 MB** and idle RAM usage is **~14 MB**.

---

## 🚀 Key Features

* **Zero-Disk Dependency**: No SQLite, local caching, or file write operations during routing. State is strictly in memory.
* **&lt;2ms Routing Overhead**: Lock-free concurrency across routing loop, counters, and probe permits. Verified by CI benchmark.
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
├── config.toml             # Multi-tier cloud provider config (6 providers)
├── config-local-test.toml  # Local-only Ollama config for testing
├── crates/
│   ├── oxllm-core/         # Core: config parsing, circuit breaker, router, telemetry
│   └── oxllm/              # CLI: Axum server, routes, signal handling, admin API
├── docs/
│   ├── architecture.md     # Concurrency model, circuit breaker rules, telemetry
│   └── providers.md        # Free-tier provider guide (snapshot: 2026-05-30)
├── .github/workflows/      # CI, security, release, crates.io publish
└── dist-workspace.toml     # cargo-dist release config
```

---

## 🛠️ Installation

### 1. Homebrew (easiest — pre-compiled binary)

```bash
brew tap planetf1/homebrew-tap
brew install oxllm
```

Pre-compiled for macOS and Linux (aarch64 + x86_64). No Rust toolchain needed. Binary size: ~2.6 MB stripped.

### 2. Cargo (compiled from source)

```bash
cargo install oxllm
```

Builds from [crates.io](https://crates.io/crates/oxllm). Requires Rust 1.85.1+.

### 3. From source (latest main)

```bash
git clone https://github.com/planetf1/oxllm.git
cd oxllm
cargo build --release
./target/release/oxllm serve --config config-local-test.toml
```

### Default Config Location

`oxllm serve` looks for config in this order:
1. `--config <path>` if provided
2. `~/.config/oxllm/config.toml` (XDG base directory)
3. `./config.toml` (current directory, for development)

```bash
# Quick start with local Ollama (no API keys needed):
cp config-local-test.toml ~/.config/oxllm/config.toml
oxllm serve

# Or with cloud providers (set env vars first):
export GROQ_API_KEY="gsk_..."
export GOOGLE_API_KEY="AIza..."
cp config.toml ~/.config/oxllm/config.toml
oxllm serve
```


## 🚀 Quick Start

You can run oxllm with **local models** (Ollama, zero API keys) or **cloud providers** (free tier).
Choose the path that works for you:

### Option A: Local Ollama (zero API keys)

```bash
# Install Ollama
brew install ollama
ollama pull granite4:micro

# Start oxllm with the included local config
oxllm serve --config config-local-test.toml

# Test it
curl http://127.0.0.1:8080/v1/chat/completions \
  -X POST -H "Content-Type: application/json" \
  -d '{"model": "default", "messages": [{"role": "user", "content": "Hello"}]}'
```

### Option B: Cloud Providers (free tier)

```bash
# Set your API keys
export GROQ_API_KEY="gsk_..."
export GOOGLE_API_KEY="AIza..."
export SAMBANOVA_API_KEY="..."
export OPENROUTER_API_KEY="sk-or-..."

# Start oxllm with the multi-tier config
oxllm serve --config config.toml

# Test the smart tier (strongest available model)
curl -X POST http://127.0.0.1:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "smart", "messages": [{"role": "user", "content": "Hello"}]}'
```

In either case, the circuit breaker handles failover automatically.
If a provider returns a 429 or is unreachable, oxllm transparently tries the next
provider in the chain.

## 🚀 Quick Start (Multi-Tier Cloud)

The repo's `config.toml` uses **6 free-tier providers** with **2 tiers** of virtual models:

- **`smart`** — Groq Llama 3.3 70B & SambaNova Llama 4 Maverick, cascading to basic
- **`basic`** — Groq Llama 4 Scout, Google Gemini Flash, SambaNova DeepSeek, OpenRouter

```bash
# Set your API keys (full list in docs/providers.md)
export GROQ_API_KEY="gsk_..."
export GOOGLE_API_KEY="AIza..."
export SAMBANOVA_API_KEY="..."
export OPENROUTER_API_KEY="sk-or-..."

# Start with cloud providers
oxllm serve --config config.toml

# Use the smart model (strongest available)
curl -X POST http://127.0.0.1:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "smart", "messages": [{"role": "user", "content": "Hello"}]}'

# Use the basic model (fast, cheap, high rate limits)
curl -X POST http://127.0.0.1:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "basic", "messages": [{"role": "user", "content": "Hello"}]}'
```

---

## ⚙️ Configuration

### Server Options

| Field | Default | Description |
|---|---|---|
| `host` | `"127.0.0.1"` | Bind address (not used when `bind_family` is `ipv6`/`dual`) |
| `port` | `8080` | Listen port |
| `otel_endpoint` | — | OTLP HTTP endpoint (e.g. `http://127.0.0.1:4318`). If unreachable, proxy starts without telemetry. |
| `upstream_timeout_secs` | `5` | Upstream request timeout in seconds |
| `bind_family` | `"ipv4"` | Address family: `"ipv4"`, `"ipv6"`, or `"dual"` (both) |

### Provider Definition

Each provider requires `name`, `enabled`, `base_url` (with trailing `/v1/`), `api_key` (or `${VAR}` env reference), and `models` list.

### Virtual Models (Fallback Chains)

Virtual models define the routing order. If a provider returns 429 or 5xx, the proxy transparently tries the next:

```toml
[virtual_models]
smart = [
  { provider = "groq-strong",  model = "llama-3.3-70b-versatile" },
  { provider = "groq-basic",   model = "meta-llama/llama-4-scout-17b-16e-instruct" },
  { provider = "ollama-fallback", model = "granite4:micro" },
]
```

### How the Routing Algorithm Works

1. When a request arrives, the proxy iterates the virtual model's provider list in order.
2. For each provider, it checks: **circuit breaker state** (Closed? Open? HalfOpen?), **rate-limit window** (cooling down?), **manual override** (admin-disabled?).
3. The first healthy provider is selected for the request.
4. On success: circuit resets to Closed, failure count drops to 0.
5. On 429 (rate limit): sets a cooldown timer based on `retry-after` header (default 30s). After 3 failures, circuit opens.
6. On 5xx: increments failure counter. After 3 failures, circuit opens for **60 × 2^(failures-3)** seconds.
7. **HalfOpen probes**: After cooldown expires, a single probe request is allowed. Only one concurrent probe — others bypass via atomic `compare_exchange`.
8. **Idle decay**: Every 5 minutes without a request, failure count decreases by 1. Below 3 failures, Open circuits automatically rehabilitate to Closed.

### Example Configs

- `config.toml` — 6 cloud providers across 2 tiers (smart + basic)
- `config-local-test.toml` — local Ollama only, zero API keys

---

## 📟 CLI Subcommands

```bash
# Start the proxy
oxllm serve                          # default: ~/.config/oxllm/config.toml
oxllm serve -v                       # verbose: per-request routing info
oxllm serve -vv                      # trace: full request/response dump

# Validate config syntax
oxllm validate                       # checks env vars, provider cross-refs

# Live dashboard (no external collector needed)
oxllm status                         # virtual model routing table + per-provider counters

# Manage providers at runtime
oxllm provider list                  # condensed provider status table
oxllm provider offline <name>        # take a provider out of rotation
oxllm provider online <name>         # re-enable a disabled provider
oxllm provider reset <name>          # clear circuit breaker, failures, rate limit

# Config hot-reload (SIGHUP)
oxllm reload

# Graceful stop (drains in-flight SSE streams)
oxllm stop
```

### Example `oxllm status` Output

```
Uptime: 1m 18s  |  Total Requests: 2

Virtual Model: smart
---------------------------------------------------------------------
| Provider             | Model                                 | Circuit        | Req | Suc |
---------------------------------------------------------------------
| ✓ groq-strong        | llama-3.3-70b-versatile               | Closed (Healthy)|   1 |   2 |
| ✓ sambanova-strong   | Llama-4-Maverick-17B-128E-Instruct    | Closed (Healthy)|   0 |   0 |
---------------------------------------------------------------------

Virtual Model: basic
---------------------------------------------------------------------
| Provider             | Model                                 | Circuit        | Req | Suc |
---------------------------------------------------------------------
| ✓ groq-basic         | meta-llama/llama-4-scout-17b-16e-... | Closed (Healthy)|   1 |   2 |
| ✓ ollama-fallback    | granite4:micro                       | Closed (Healthy)|   0 |   0 |
---------------------------------------------------------------------

Use 'oxllm provider offline <name>' to take a provider out of rotation.
Use 'oxllm provider reset <name>' to clear circuit breaker state.
For full per-provider counters, run 'oxllm status' without piping.
```

All admin endpoints (`/health`, `/status`, `/reload`, `/admin/*`) are restricted to localhost — external callers receive `403 Forbidden`.

---

## 📄 License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.
