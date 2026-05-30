# `oxllm` 🦀 (Oxide LLM Proxy)

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/Rust-1.85.1%2B-orange.svg)](https://www.rust-lang.org/)
[![cargo-dist](https://img.shields.io/badge/packaged%20with-cargo--dist-000.svg?style=flat&logo=rust)](https://github.com/axodotl/cargo-dist)

`oxllm` (Oxide LLM Proxy) is an ultra-minimalist, high-resilience adaptive routing LLM gateway written in Rust. It exposes an OpenAI-compatible interface, proxying requests to a tiered fallback pool of free-tier LLM providers. 

Built to operate entirely in memory with zero local disk persistence, `oxllm` is highly optimized for resource-constrained edge devices (like OpenWrt routers), developer workstations, and background daemons.

---

## 🚀 Key Features

* **Zero-Disk Dependency**: No SQLite, local caching, or file write operations during routing. State is strictly in memory.
* **Under <2ms Routing Overhead**: Designed using lock-free concurrency to avoid thread contention on your CPU's hot path.
* **Thundering Herd Defense**: Strict `HalfOpen` state machine with lock-free `probe_in_flight` atomic check-and-set locks. Concurrent requests automatically bypass a probing provider to prevent avalanche failures.
* **Unix-Style Environment Expansion**: Native shell-style `${VAR}` environment variable replacement parsed safely inside TOML keys.
* **OOM-Proof Telemetry**: Bounded OTel JSON event channel (`1024` cap) with non-blocking `try_send` drops to protect system memory if the local collector goes offline.
* **W3C Trace Context Propagation**: Seamlessly extracts and injects W3C `traceparent` headers, automatically generating new root contexts to render beautiful, continuous trace spans in collectors like [Otelite](https://github.com/planetf1/otelite).
* **Hot Config Reloading**: Unix `SIGHUP` signal listener that parses updated `config.toml` files and hot-swaps active memory pools seamlessly via `tokio::sync::watch` without dropping connections.
* **Musl Cross-Compilation Out-Of-The-Box**: Uses a pure-Rust `rustls-tls` stack to avoid native OpenSSL dynamic linking headaches on edge routers.

---

## 📦 Project Layout

`oxllm` is structured as a modular Cargo workspace:

* **[`crates/oxllm-core`](file:///crates/oxllm-core)**: Pure domain structures, TOML configuration parsing, standard `OxllmError` types, environment variable expanding, standard OTel batch telemetry processors, and the stateless `RoutingStrategy` trait.
* **[`crates/oxllm`](file:///crates/oxllm)**: Axum 0.8 HTTP routing, POSIX signals handlers, graceful SIGTERM shutdowns, and `clap` CLI binaries.
* **[`docs/`](file:///docs)**: Technical architectures and specs.

---

## 🛠️ Installation & Usage

### Installation Options

You can install `oxllm` using either **Homebrew** (recommended for pre-compiled speed) or **Cargo**:

#### 1. Via Homebrew (Pre-compiled)
Install the pre-compiled binary instantly using your Homebrew formula tap:
```bash
brew tap planetf1/homebrew-tap
brew install oxllm
```

#### 2. Via Cargo (Compiled from source)
Install the binary directly from crates.io by compiling it on your machine:
```bash
cargo install oxllm
```

### Configuration (`config.toml`)
Define your upstream providers and priority virtual model mappings in a single local config file:

> **`base_url` convention**: the value must end with a trailing slash and include the full API path prefix (e.g. `/v1/`). `oxllm` appends `chat/completions` and `embeddings` relative to this base. If your provider already exposes `/v1/`, set `base_url = "https://api.example.com/v1/"`.

> **Telemetry is optional**: if `otel_endpoint` is unreachable (e.g. no local collector running), `oxllm` logs a warning and starts normally without exporting telemetry.

**Cloud providers (OpenAI-compatible):**
```toml
[server]
host = "127.0.0.1"
port = 8080
otel_endpoint = "http://127.0.0.1:4318"
upstream_timeout_secs = 5

[[providers]]
name = "google-ai-studio"
enabled = true
base_url = "https://generativelanguage.googleapis.com/v1beta/openai/"
api_key = "${AI_STUDIO_KEY}"
models = ["gemini-2.5-flash"]

[[providers]]
name = "groq"
enabled = true
base_url = "https://api.groq.com/openai/v1/"
api_key = "${GROQ_KEY}"
models = ["llama-4-scout", "deepseek-r1-distill"]

[virtual_models]
complex-free = [
  { provider = "groq", model = "deepseek-r1-distill" },
  { provider = "google-ai-studio", model = "gemini-2.5-flash" }
]
```

**Local Ollama (fully self-contained, no API keys needed):**
```toml
[server]
host = "127.0.0.1"
port = 8080
otel_endpoint = "http://127.0.0.1:4318"
upstream_timeout_secs = 60

[[providers]]
name = "ollama-primary"
enabled = true
base_url = "http://localhost:11434/v1/"
api_key = "ollama"
models = ["granite4:micro"]

[[providers]]
name = "ollama-fallback"
enabled = true
base_url = "http://localhost:11434/v1/"
api_key = "ollama"
models = ["granite4.1:3b"]

[virtual_models]
default = [
  { provider = "ollama-primary",  model = "granite4:micro" },
  { provider = "ollama-fallback", model = "granite4.1:3b"  },
]
```

### CLI Subcommands
Manage the daemon easily using clean POSIX CLI commands:

```bash
# Starts the LLM proxy server in the foreground
oxllm serve --config config.toml

# Validates configuration syntax and runs network connection tests
oxllm validate --config config.toml

# Queries the running daemon locally and prints a beautiful status table
oxllm status

# Triggers a SIGHUP config hot-reload on the active oxllm process
oxllm reload
```

---

## 🔬 Developer Guides & Architecture

* For details on the concurrency routing loop, failover logic, and circuit breaker states, see the **[Architecture & Design Specifications](file:///docs/architecture.md)**.
* To review the dev roadmap, check out the **[Technical Implementation Plan](file:///implementation_plan.md)**.

## 📄 License

Licensed under the Apache License, Version 2.0. See [LICENSE](file:///LICENSE) for details.
