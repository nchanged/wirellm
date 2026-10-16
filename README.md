# WireLLM ⚡🦀
### The Ultra-Fast, Low-Memory Autonomous LLM Gateway & Native MCP Server

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![RAM](https://img.shields.io/badge/Memory_Footprint-~20_MB-brightgreen.svg)]()
[![Routing Overhead](https://img.shields.io/badge/Routing_Overhead-<2ms-brightgreen.svg)]()

**WireLLM** is an ultra-minimalist, lightning-fast adaptive routing LLM gateway and native **Model Context Protocol (MCP)** server written in 100% pure Rust. It acts as an invisible, high-resilience wire between your AI developer tools (OpenCode, Claude Code, Cursor, Windsurf, Antigravity) and a resilient pool of LLM providers.

> *WireLLM was inspired by the architecture of `oxllm` and re-engineered with SQLite WAL state persistence, AES-256-GCM encrypted credential vaulting, a full-featured real-time React dashboard embedded into the binary, and first-class Model Context Protocol (MCP) tooling.*

---

## 🎯 Why WireLLM Exists

### 1. Minimal Impact on RAM (~20 MB vs. 500 MB+)
Most LLM routing gateways (LiteLLM, One-API, Portkey, LibreChat) are heavyweight Python or Node.js services. They consume **300 MB to 1 GB+ of RAM**, require Docker containers, have multi-second startup times, and introduce 20–60 ms of garbage collection and runtime overhead per request.

**WireLLM is built in pure Rust**:
* **~20 MB idle RAM footprint** (up to 95% memory savings compared to Python/Node proxies).
* **Instant startup** in under 15 milliseconds.
* **<2ms internal routing overhead** with lock-free atomic counters and non-blocking channels.
* Can run seamlessly in the background on edge routers, modest VPSs, laptops, and developer workstations without noticeably consuming resources.

### 2. Native Model Context Protocol (MCP) Focus
WireLLM doesn't just route LLM calls—it natively implements an MCP server (`/mcp/sse`) with JSON-RPC 2.0 tools. 

This means your AI coding agents can **manage their own model infrastructure**:
* Agents can check provider health and circuit states.
* Agents can add, modify, or remove API keys dynamically.
* Agents can reset tripped circuit breakers or tune virtual model routing chains autonomously.

### 3. "Let Your Agents Set It Up For You"
Why spend 20 minutes manually configuring config files, base URLs, and environment variables? With WireLLM, you can simply hand an agent a single prompt (see [Agent Setup Instructions](#-let-your-agents-set-it-up-for-you)), and the agent will inspect the running WireLLM instance, configure providers, and set up virtual model chains through the MCP interface or REST API.

### 4. Predictive Pacing & Seamless Burst Cascading
Autonomous agents (OpenCode, Cline, Claude Code, Cursor) frequently fire rapid bursts of requests during autonomous multi-step loops (planning, tool calls, code review) that instantly trigger `429 Too Many Requests` or harsh rate-limit locks on upstream providers.

WireLLM eliminates this **proactively on the wire before the upstream provider is ever contacted**:
* **Predictive Burst Spacing (`min_interval_ms`)**: Enforces required inter-request cooldown intervals (e.g., 2,500ms between calls for free/community tiers) locally. When a rapid request arrives within the cooldown window, WireLLM **never sleeps or blocks the client**—it instantly skips the constrained target in `<0.1ms` and cascades down the priority ladder to the next standby provider.
* **Sliding-Window RPM Accounting (`max_rpm`)**: Tracks requests across an in-memory 60-second sliding window. When a provider reaches its RPM ceiling, subsequent queries are preemptively diverted to standby fallbacks, preventing upstream 429 lockouts entirely.
* **In-Flight Concurrency Throttling (`max_concurrency`)**: Guards against multi-agent parallel bursts. If an agent executes concurrent tool calls, WireLLM fans them out across available ladder targets simultaneously.
* **Zero Client-Side Retries or Delays**: Coding agents never sleep, crash, or see 429 errors. They receive immediate streaming tokens while WireLLM silently manages upstream pacing, rate limits, and fallback execution.
* **Autonomous Self-Healing**: As soon as a provider's cooldown timer elapses or the 60s sliding window frees up slots, WireLLM automatically restores it to primary status without human intervention.
* **Virtual Role Fallback Ladders**: Group models into roles like `coder`, `planner`, `orchestrator`, and `code-reviewer` with deterministic cascade chains across cloud providers (DeepSeek, Groq, NVIDIA NIM, OpenAI, Anthropic, Cerebras, Sambanova, OpenRouter, local Ollama).

### 5. Encrypted Vault (Zero Secret Leaks)
* API keys can be stored encrypted with **AES-256-GCM** using local machine salt (`0600` permissions) directly in `~/.wirellm/`.
* Alternatively, use dynamic shell environment variable templates like `${DEEPSEEK_API_KEY}` evaluated at runtime.
* Keys are never exposed in logs, telemetry, or client responses.

---

## 🏗️ Architecture & Component Overview

```
                      +------------------------------------------+
                      |   AI Developer Tools & Coding Agents     |
                      |  (OpenCode, Claude Code, Cursor, SDKs)   |
                      +--------------------+---------------------+
                                           |
                    +----------------------+----------------------+
                    | OpenAI HTTP API      | Model Context Protocol|
                    | POST /v1/chat/completions | GET/POST /mcp/*  |
                    v                      v                       v
      +-----------------------------------------------------------------+
      |                           WireLLM                               |
      |  +-----------------------------------------------------------+  |
      |  |                Adaptive Dynamic Router                    |  |
      |  |   * Circuit Breaker (Closed / HalfOpen / Open)            |  |
      |  |   * Proactive RPM & Burst Window Rate Limiter             |  |
      |  |   * In-Flight Atomic Concurrency Limiter                  |  |
      |  +-----------------------------+-----------------------------+  |
      |                                |                                |
      |  +-------------------------+   |   +-------------------------+  |
      |  |      SQLite + WAL       |   |   |   Embedded Dashboard    |  |
      |  |  * Providers & Models   |   |   |   * Real-time React SPA |  |
      |  |  * Daily Token Stats    |   |   |   * Single binary embed |  |
      |  +-------------------------+   |   +-------------------------+  |
      |  |    AES-256-GCM Vault    |   |   |   OpenTelemetry (OTel)  |  |
      |  |  * Salted local keys    |   |   |   * Bounded OTLP traces |  |
      |  +-------------------------+   |   +-------------------------+  |
      +--------------------------------+--------------------------------+
                                       |
          +----------------------------+----------------------------+
          |                            |                            |
          v                            v                            v
   +--------------+             +--------------+             +--------------+
   |   DeepSeek   |             |  Groq / NIM  |             | Local Ollama |
   |  (Primary)   |             |  (Fallback)  |             |  (Offline)   |
   +--------------+             +--------------+             +--------------+
```

---

## ⚡ Quick Start

### 1. Build and Install

```bash
git clone https://github.com/nchanged/wirellm.git
cd wirellm

# Build release binary
cargo build --release

# Install to ~/.local/bin
install -m 755 target/release/wirellm ~/.local/bin/wirellm
```

### 2. Service Management

WireLLM comes with an embedded daemon manager:

```bash
# Start background daemon on default port 8765
wirellm start

# Check status, provider circuits, and request metrics
wirellm status

# View live streaming server logs
wirellm logs

# Restart service after configuration changes
wirellm restart

# Stop service
wirellm stop
```

You can also run WireLLM interactively in the foreground:
```bash
wirellm serve --port 8765 -v
```

### 3. Access Dashboard & Endpoints

Once started:
* **Web UI Dashboard**: `http://localhost:8765/` (or `/dashboard`, `/ui`)
* **OpenAI Base URL**: `http://localhost:8765/v1`
* **MCP SSE Endpoint**: `http://localhost:8765/mcp/sse`
* **MCP Messages RPC**: `http://localhost:8765/mcp/messages`
* **Health Check**: `http://localhost:8765/health`

---

## 🤖 "Let Your Agents Set It Up For You"

You can give your AI agent (Claude Code, Cursor, OpenCode, Antigravity) the instructions below to let it inspect, configure, and manage WireLLM autonomously.

### Copy-Paste Prompt for Your Agent

```markdown
You have access to WireLLM running locally at http://127.0.0.1:8765.

WireLLM exposes:
1. Native MCP Tools over SSE at http://127.0.0.1:8765/mcp/sse:
   - `wirellm_get_status`: Inspect active providers, circuit states, and token stats.
   - `wirellm_list_providers`: List configured upstream LLM endpoints.
   - `wirellm_add_provider`: Add/update an LLM provider with API keys, RPM limits, and models.
   - `wirellm_reset_circuit`: Reset tripped circuit breakers to Closed (Healthy).
   - `wirellm_list_virtual_models`: List aliases like `coder`, `planner`, `orchestrator`.
   - `wirellm_update_virtual_model`: Tune provider fallback chains for each virtual model.
   - `wirellm_get_token_stats`: View historical usage and latency.
   - `wirellm_get_routing_policy`: Read the live timeout, overload-breathing, and circuit-breaker cooldown settings.
   - `wirellm_set_routing_policy`: Tune & persist those settings (applied live, no restart, no Rust edits).
2. REST Admin API:
   - GET http://127.0.0.1:8765/api/status
   - GET/POST http://127.0.0.1:8765/api/routing-policy
   - POST http://127.0.0.1:8765/api/providers
   - POST http://127.0.0.1:8765/api/virtual-models
   - POST http://127.0.0.1:8765/api/providers/:name/reset

Please check WireLLM status, ensure our required providers and virtual models are configured, and route requests to `http://127.0.0.1:8765/v1` using `wirellm/coder`, `wirellm/planner`, or `wirellm/orchestrator`.
```

---

## 🔌 Tool Integrations

### OpenCode Setup (`~/.config/opencode/opencode.jsonc`)

Add the `wirellm` provider to your OpenCode configuration:

```jsonc
{
  "provider": {
    "wirellm": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "WireLLM Proxy (Local)",
      "options": {
        "baseURL": "http://127.0.0.1:8765/v1",
        "apiKey": "sk-wirellm-local"
      },
      "models": {
        "orchestrator": {
          "id": "orchestrator",
          "name": "WireLLM: Orchestrator",
          "reasoning": true,
          "tool_call": true
        },
        "coder": {
          "id": "coder",
          "name": "WireLLM: Coder",
          "reasoning": true,
          "tool_call": true
        },
        "planner": {
          "id": "planner",
          "name": "WireLLM: Planner",
          "reasoning": true,
          "tool_call": true
        },
        "code-reviewer": {
          "id": "code-reviewer",
          "name": "WireLLM: Code Reviewer",
          "reasoning": true,
          "tool_call": true
        }
      }
    }
  }
}
```

### Claude Code / Cursor MCP Setup (`mcp.json`)

To let Claude Code or Cursor control WireLLM via MCP:

```json
{
  "mcpServers": {
    "wirellm": {
      "url": "http://127.0.0.1:8765/mcp/sse",
      "transport": "sse"
    }
  }
}
```

---

## ⚙️ Configuration Files & Vault

WireLLM stores configuration and data in `~/.wirellm/`:

| File | Purpose | Security |
|---|---|---|
| `~/.wirellm/config.yaml` | Static provider and virtual model defaults | User editable |
| `~/.wirellm/wirellm.db` | SQLite database in WAL mode (providers, models, token metrics) | User readable |
| `~/.wirellm/.vault_salt` | Unique per-machine salt for AES-256 encryption | Mode `0600` |
| `~/.wirellm/.env` | Optional local environment variables for API keys | Mode `0600` |
| `~/.wirellm/wirellm.pid` | Process ID tracking for service management | Internal |
| `~/.wirellm/wirellm.log` | Rolling server and proxy event log | Internal |

---

## 📈 Performance & Benchmarks

Run the built-in routing loop benchmark:
```bash
cargo test -p wirellm-core --test performance -- --nocapture
```

* **Routing Decision Latency**: **< 15 microseconds** per request through atomic state evaluation.
* **Memory Footprint**: **~20 MB RSS** under multi-threaded load.
* **Concurrency**: Pure lock-free atomics and `tokio::sync::RwLock` across provider pools.

---

## 👥 Authors & Acknowledgments

* **Author**: [nchanged](https://github.com/nchanged)
* **Inspiration**: Inspired by [`oxllm`](https://github.com/planetf1/oxllm) by PlanetF1.

---

## 📄 License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.
