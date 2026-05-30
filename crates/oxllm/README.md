# `oxllm` 🦀 (Oxide LLM Proxy Binary)

[![Crates.io](https://img.shields.io/crates/v/oxllm.svg)](https://crates.io/crates/oxllm)
[![Docs.rs](https://docs.rs/oxllm/badge.svg)](https://docs.rs/oxllm)
[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

`oxllm` is the binary gateway application for **Oxide LLM Proxy** — an ultra-minimalist, high-resilience adaptive routing LLM gateway written in Rust.

It exposes a single OpenAI-compatible HTTP interface (`POST /v1/chat/completions`, `POST /v1/embeddings`, `GET /v1/models`), proxying requests to a tiered fallback pool of LLM providers with automatic rate-limit detection, circuit breakers, and SIGHUP hot-reloading.

---

## 🚀 Binary Features

- **OpenAI-Compatible Interface**: Out-of-the-box compatibility with existing OpenAI client SDKs (Python, JS, curl, LangChain).
- **SSE Stream Forwarding**: Real-time token chunk streaming for chat completions using mapped asynchronous byte streams.
- **Granular Localhost Administrative Isolation**: Security isolation for administrative routes like `/status` and `/health`, returning `403 Forbidden` to external network callers.
- **Graceful POSIX Shutdown**: Listens for `SIGINT`/`SIGTERM` to safely drain open client connections and SSE event streams before terminating.
- **POSIX Signal Hot-Reloading**: Spawns a background Unix `SIGHUP` listener utilizing `tokio::sync::watch` to hot-swap active `AppState` memory pools on the fly without dropping connections.
- **Ultra-low Memory Footprint**: Less than **25 MB RAM** at idle and less than **40 MB RAM** under peak concurrency.

## 📦 Installation

You can install the `oxllm` binary using either **Homebrew** (recommended for pre-compiled speed) or **Cargo**:

### 1. Via Homebrew (Pre-compiled)
Install the pre-compiled binary instantly using your Homebrew formula tap:
```bash
brew tap planetf1/homebrew-tap
brew install oxllm
```

### 2. Via Cargo (Compiled from source)
Install the binary directly from crates.io by compiling it on your machine:
```bash
cargo install oxllm
```

---

## 🛠️ CLI Subcommands

Manage the daemon using simple, standard CLI commands:

```bash
# Starts the gateway server in the foreground (binds host:port from config)
oxllm serve --config config.toml

# Parses and validates configuration syntax and cross-references virtual models
oxllm validate --config config.toml

# Queries the running daemon locally and prints a beautiful ASCII status table
oxllm status

# Triggers a SIGHUP config hot-reload on the active oxllm process
oxllm reload
```

---

## 🌐 Endpoints

| Method | Path | Description | Access |
| :--- | :--- | :--- | :--- |
| **POST** | `/v1/chat/completions` | Standard chat completions (supports `stream: true/false`). | Public |
| **POST** | `/v1/embeddings` | Standard text embeddings with auto-retry failovers. | Public |
| **GET** | `/v1/models` | List of currently healthy virtual models. | Public |
| **GET** | `/status` | Complete ASCII table status of provider pool metrics. | **Localhost-Only** |
| **GET** | `/health` | Gateway live healthcheck indicator. | **Localhost-Only** |

---

## 📄 License

Licensed under the Apache License, Version 2.0. See [LICENSE](https://github.com/planetf1/oxllm/blob/main/LICENSE) for details.
