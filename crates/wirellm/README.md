# `wirellm` ⚡🦀 (WireLLM Gateway Binary)

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)

`wirellm` is the CLI binary and server daemon for **WireLLM** — an ultra-minimalist, high-resilience adaptive routing LLM gateway and native Model Context Protocol (MCP) server written in Rust.

It exposes OpenAI-compatible HTTP endpoints (`/v1/chat/completions`, `/v1/models`, `/v1/embeddings`), native MCP SSE tooling (`/mcp/sse`), and an embedded real-time SPA dashboard.

---

## 🚀 Key Features

- **Sub-Millisecond Overhead**: Pure async Rust with lock-free atomic counters.
- **Ultra-low Memory Footprint**: ~20 MB RAM at idle.
- **Built-in Daemon Service**: Manage with `wirellm start`, `wirellm status`, `wirellm restart`, `wirellm logs`, and `wirellm stop`.
- **Native MCP Server**: Real-time Model Context Protocol SSE support for autonomous agent inspection and configuration.
- **Encrypted Credential Vault**: AES-256-GCM local storage with salted keys.
- **Embedded Web Dashboard**: Modern React SPA served directly from memory.

---

## 🛠️ CLI Subcommands

```bash
# Start background daemon
wirellm start --port 8765

# Foreground server with verbose logging
wirellm serve --port 8765 -v

# Query status and view provider circuit states
wirellm status

# Tail server logs
wirellm logs --follow

# Restart or reload service
wirellm restart
wirellm reload

# Stop service
wirellm stop
```

---

## 📄 License

Licensed under the Apache License, Version 2.0. See [LICENSE](../../LICENSE) for details.
