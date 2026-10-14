# WireLLM Recommended Setup & Provider Topology Guide ⚡🦀

This document provides a continuously maintained reference of **always-free providers**, **budget high-throughput options**, and **model capability ratings** optimized for autonomous coding agents (OpenCode, Claude Code, Cursor, Windsurf, Antigravity).

---

## 1. Quick Agent Onboarding Prompt (MCP)

Copy and send this instruction to your AI coding agent:

```text
Connect to the WireLLM MCP server at <MCP_ENDPOINT> (e.g. http://localhost:8765/mcp/sse or https://llm.example.com/mcp/sse).
NOTE: If WireLLM is running remotely in the cloud and API token access is enabled, pass your token in the Authorization header: 'Bearer <TOKEN>'.

1. Inspect the available tools with 'wirellm_get_setup_recommendations'.
2. Ask me for API keys for any providers I have (e.g. NVIDIA NIM, Groq, DeepSeek, OpenRouter).
3. Use 'wirellm_add_provider' to register available providers with their recommended rate limits and burst spacing.
4. Use 'wirellm_update_virtual_model' to set up the recommended fallback ladders for 'coder', 'planner', and 'orchestrator'.
5. Verify everything is healthy with 'wirellm_get_status'.
```

---

## 2. Always-Free & Community Tier Providers

| Provider | Base URL | Supported Free / Trial Models | Free Tier Limits | Model Ratings (Code / Reason / Speed) | Notes |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **NVIDIA NIM** | `https://integrate.api.nvidia.com/v1` | `nvidia/nemotron-3-ultra-550b-a55b`<br>`nvidia/nemotron-3.5-lightning-30b-a3b`<br>`meta/llama-3.3-70b-instruct`<br>`deepseek-ai/deepseek-v3.1-flash` | ~1,000 free API credits on signup | Code: **9.6 / 10**<br>Reason: **9.4 / 10**<br>Speed: **9.2 / 10** | Incredible reasoning & coding performance. Highly recommended as primary. |
| **GroqCloud** | `https://api.groq.com/openai/v1` | `openai/gpt-oss-120b`<br>`openai/gpt-oss-20b`<br>`qwen/qwen3.8-27b`<br>`llama-3.3-70b-versatile` | 30 RPM / 14,400 RPD free tier | Code: **9.1 / 10**<br>Reason: **8.9 / 10**<br>Speed: **10 / 10** (500+ tps) | Fastest LPUs on Earth. Perfect for low-latency code generation & tool routing. |
| **OpenRouter (Free)** | `https://openrouter.ai/api/v1` | `openrouter/free`<br>`cohere/north-mini-code:free`<br>`nvidia/nemotron-3-ultra-550b-a55b:free`<br>`step-5-preview-free` | 20 RPM free community routing | Code: **8.5 / 10**<br>Reason: **8.4 / 10**<br>Speed: **7.8 / 10** | Zero cost. Excellent fallback when rate limits on direct providers are reached. |
| **Google Gemini Studio** | `https://generativelanguage.googleapis.com/v1beta/openai/` | `gemini-2.5-flash`<br>`gemini-2.0-flash` | 15 RPM free tier | Code: **9.2 / 10**<br>Reason: **9.3 / 10**<br>Speed: **9.5 / 10** | 1M token context window. Ideal for full-codebase repository analysis. |
| **Cerebras** | `https://api.cerebras.ai/v1` | `llama3.1-70b`<br>`llama3.1-8b` | 30 RPM free tier | Code: **8.8 / 10**<br>Reason: **8.6 / 10**<br>Speed: **10 / 10** (1800+ tps) | Wafer-scale speed. Instantaneous code autocomplete. |

---

## 3. Ultra-Cheap High-Value Providers

| Provider | Base URL | Recommended Models | Pricing | Model Ratings | Notes |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **DeepSeek Official** | `https://api.deepseek.com/v1` | `deepseek-chat` (V3)<br>`deepseek-reasoner` (R1) | \$0.14 / 1M input (cache hit: \$0.014)<br>\$0.55 / 1M output | Code: **9.8 / 10**<br>Reason: **9.9 / 10**<br>Speed: **8.5 / 10** | State-of-the-art coding and math reasoning. Exceptional quality per dollar. |
| **OpenRouter Direct** | `https://openrouter.ai/api/v1` | `deepseek/deepseek-chat`<br>`anthropic/claude-3.5-sonnet`<br>`meta/llama-3.3-70b-instruct` | Pay-per-token with unified balance | Code: **9.7 / 10**<br>Reason: **9.6 / 10**<br>Speed: **8.8 / 10** | Single API key gives access to over 200 models. |

---

## 4. Recommended Pacing & Rate Limit Settings

To prevent `429 Too Many Requests` errors, configure WireLLM's proactive rate limiters:

```yaml
# NVIDIA NIM
rpm: 40
min_interval_ms: 1200      # 1.2s burst spacing
max_concurrency: 3

# GroqCloud
rpm: 30
min_interval_ms: 1500      # 1.5s burst spacing
max_concurrency: 2

# OpenRouter Free
rpm: 20
min_interval_ms: 2000      # 2.0s burst spacing
max_concurrency: 2

# DeepSeek Official
rpm: 60
min_interval_ms: 500       # 500ms burst spacing
max_concurrency: 4
```

---

## 5. Recommended Virtual Model Fallback Ladders

WireLLM routes virtual model aliases (`coder`, `planner`, `orchestrator`) down deterministic priority cascades:

### Ladder A: `coder` (Code Generation & Multi-File Refactoring)
1. **Primary**: `nvidia` $\rightarrow$ `nvidia/nemotron-3-ultra-550b-a55b` (or `nvidia/nemotron-3.5-lightning-30b-a3b`)
2. **First Fallback**: `groq` $\rightarrow$ `openai/gpt-oss-120b` (or `qwen/qwen3.8-27b`)
3. **Second Fallback**: `deepseek` $\rightarrow$ `deepseek-chat`
4. **Third Fallback**: `openrouter` $\rightarrow$ `cohere/north-mini-code:free`

### Ladder B: `planner` (Architecture & Deep Reasoning)
1. **Primary**: `deepseek` $\rightarrow$ `deepseek-reasoner` (R1 chain-of-thought)
2. **First Fallback**: `nvidia` $\rightarrow$ `nvidia/nemotron-3-ultra-550b-a55b`
3. **Second Fallback**: `groq` $\rightarrow$ `qwen/qwen3.8-27b`

### Ladder C: `orchestrator` (Fast Tool Dispatch & Summarization)
1. **Primary**: `groq` $\rightarrow$ `openai/gpt-oss-120b` (instant low latency)
2. **First Fallback**: `nvidia` $\rightarrow$ `nvidia/nemotron-3.5-lightning-30b-a3b`
3. **Second Fallback**: `deepseek` $\rightarrow$ `deepseek-chat`
4. **Third Fallback**: `openrouter` $\rightarrow$ `openrouter/free`

---

## 6. MCP JSON-RPC Setup Snippets

If calling WireLLM's MCP tools programmatically:

### Register NVIDIA NIM Provider:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "wirellm_add_provider",
    "arguments": {
      "name": "nvidia",
      "base_url": "https://integrate.api.nvidia.com/v1",
      "api_key_template": "${NVIDIA_API_KEY}",
      "models": [
        "nvidia/nemotron-3-ultra-550b-a55b",
        "nvidia/nemotron-3.5-lightning-30b-a3b",
        "meta/llama-3.3-70b-instruct"
      ],
      "rpm": 40,
      "min_interval_ms": 1200,
      "max_concurrency": 3,
      "smartness_score": 95
    }
  }
}
```

### Register `coder` Virtual Model Ladder:
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "wirellm_update_virtual_model",
    "arguments": {
      "name": "coder",
      "description": "High-performance coding chain with auto rate-limit failover",
      "targets": [
        { "provider": "nvidia", "model": "nvidia/nemotron-3-ultra-550b-a55b" },
        { "provider": "groq", "model": "openai/gpt-oss-120b" },
        { "provider": "deepseek", "model": "deepseek-chat" },
        { "provider": "openrouter", "model": "cohere/north-mini-code:free" }
      ]
    }
  }
}
```
