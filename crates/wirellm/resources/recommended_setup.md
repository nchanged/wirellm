# WireLLM Recommended Setup & Provider Topology Guide ⚡🦀

This document provides a continuously maintained reference of **always-free providers**, **budget high-throughput options**, and the **proven production topology** for autonomous coding agents (OpenCode, Claude Code, Cursor, Windsurf, Antigravity). Last verified live: **2026-10-18**.

---

## 1. Quick Agent Onboarding Prompt (MCP)

Copy and send this instruction to your AI coding agent:

```text
Connect to the WireLLM MCP server at <MCP_ENDPOINT> (e.g. http://localhost:8765/mcp or https://<RELAY_HOST>/mcp).
NOTE: If WireLLM is running remotely in the cloud and API token access is enabled, pass your token in the Authorization header: 'Bearer <TOKEN>'.

1. Inspect available tools with 'wirellm_get_setup_recommendations'.
2. Check configured vault items with 'wirellm_list_secrets'.
3. If new keys are needed, ask me for provider keys and store them with 'wirellm_set_secret' (AES-256-GCM encrypted). Never paste keys into chat.
4. For any provider, explore live available models using 'wirellm_discover_provider_models'.
5. Use 'wirellm_add_provider' to register providers — ONE MODEL PER PROVIDER (isolates circuit breakers so healthy siblings don't cascade-fail). Key-less providers (e.g. Kilo free pool) are allowed: omit all key arguments and no Authorization header is sent.
6. Use 'wirellm_update_virtual_model' to set up the single 'main' ladder (see §5) plus 'embed' if embeddings are needed. Never wire paid models.
7. Probe every rung with 'wirellm_test_provider' and verify with 'wirellm_get_status' before reporting success.
```

---

## 2. Always-Free Providers (verified live, 2026-10-18)

| Provider | Base URL | Key? | Verified Free Models | Free Tier Limits | Ratings (Code / Reason / Speed) | Notes |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **NVIDIA NIM** | `https://integrate.api.nvidia.com/v1` | yes (`NVIDIA_API_KEY`) | `nvidia/nemotron-3-ultra-550b-a55b` (1M)<br>`nvidia/nemotron-3-super-120b-a12b` (1M)<br>`nvidia/nemotron-3.5-lightning-30b-a3b` (1M)<br>`nvidia/nemotron-3-embed-1b` (embeddings) | 40 RPM / 10,000 RPD per model | Code: **9.6 / 10**<br>Reason: **9.4 / 10**<br>Speed: **9.2 / 10** | Primary workhorse. Register one model per provider. |
| **Google Gemini Studio** | `https://generativelanguage.googleapis.com/v1beta/openai/` | yes (`GEMINI_API_KEY`) | `gemini-2.5-flash` (1M ctx) | 15 RPM / 1,500 RPD | Code: **9.2 / 10**<br>Reason: **9.3 / 10**<br>Speed: **9.5 / 10** | Great mid-ladder cushion for 1M-context requests. |
| **GroqCloud** | `https://api.groq.com/openai/v1` | yes (`GROQ_API_KEY`) | `openai/gpt-oss-120b` (131K)<br>`openai/gpt-oss-20b`<br>`qwen/qwen3.8-27b` | 30 RPM / 1,000 RPD (chat models) | Code: **9.1 / 10**<br>Reason: **8.9 / 10**<br>Speed: **10 / 10** (500+ tps) | Smallest context (131K) — put LAST in ladders. |
| **OpenRouter (Free)** | `https://openrouter.ai/api/v1` | yes (`OPENROUTER_API_KEY`) | ~17 `:free` models: `cohere/north-mini-code:free` (256K), `thinkingmachines/inkling:free` (1M), `nvidia/*:free` family, `qwen/qwen3.8-27b:free`, `dots-3-note-preview:free`, `openrouter/free` auto-router | 20 RPM / 50 RPD per model (1,000 RPD after one-time $10 credit top-up) | Code: **8.5 / 10**<br>Reason: **8.4 / 10**<br>Speed: **7.8 / 10** | `:free` suffix only; free-pool substitution may serve a sibling model. |
| **UnoRouter (Free)** | `https://api.unorouter.com/v1` | yes (`UNOROUTER`) | ~130 `:free` models: `glm-5.3-flash:free` (1M), `glm-5.3:free` (1M), `nemotron-3-ultra-550b-a55b:free` (1M), `nemotron-3-super-120b-a12b:free`, `deepseek-v4-pro:free` (1M), `minimax-m3:free` (1M), `qwen3.8-27b:free`, `jina-embeddings-v4:free` (2048-dim embeddings) | ~1 RPM per model/user; 429 + Retry-After (45s if Discord server tag worn) | Code: **9.0 / 10**<br>Reason: **8.8 / 10**<br>Speed: **6 / 10** | Deep free catalog; free-pool substitution observed. Embed fallback must be dimension-matched (2048). |
| **Kilo AI Gateway (Free pool)** | `https://api.kilo.ai/api/gateway/v1` | **NO KEY NEEDED** | 14 `:free` models: `nvidia/nemotron-3-ultra-550b-a55b:free` (1M), `nvidia/nemotron-3.5-lightning:free` (1M), `thinkingmachines/inkling-small:free` (1M), `nvidia/nemotron-3-super-120b-a12b:free`, `cohere/north-mini-code:free`, `poolside/laguna-s-2.1:free`, `dots-studio/dots-3-note-preview:free`, `kilo-auto/free` router | 200 requests/hour per IP; invalid bearer tokens 401 — send no header (key-less registration) | Code: **9.0 / 10**<br>Reason: **8.8 / 10**<br>Speed: **7 / 10** | Catalog can lag reality (stale rows 404; unlisted models may answer). Custom header `Authorization: "Bearer "` (empty) also works. |

### 2b. Additional free sources (researched 2026-10-18, not yet registered)

| Provider | Free Models | Limits | Notes |
| :--- | :--- | :--- | :--- |
| **Z AI international** (`https://api.z.ai/api/paas/v4`) | `glm-4.7-flash` (200K, reasoning), `glm-4.6v-flash` (vision) | permanent free, 1 concurrent | Always-free GLM rung; accepts overseas signup |
| **Cloudflare Workers AI** | 51 models: `gpt-oss-120b`, `qwen3.8-27b`, `glm-4.7-flash`, `llama-4-scout` | 10K neurons/day shared | Needs account ID + API token |
| **Mistral free mode** (`https://api.mistral.ai/v1`) | Mistral Large 3, Codestral, Small 4 | $10/mo credits, no card, ~1 RPS | May train on prompts unless opted out |
| **LLM7.io** (`https://api.llm7.io/v1`) | `gpt-oss:20b`, DeepSeek-V4-Flash-0731 (400K) | 60 RPM, ~1M tokens/24h (free token) | Anonymous access works |
| **OVHcloud AI Endpoints** | `qwen3.5-397b-a17b`, `gpt-oss-120b` (EU-hosted) | 2 RPM anonymous, **no signup at all** | Zero-setup emergency rung |
| **SambaNova** | DeepSeek, Llama-405B | 20 RPM / 20 RPD / 200K TPD permanent | ⚠️ ToS forbids proxying to third parties |
| **GitHub Models** | 160+ models (GPT-5, o-series, Grok-3) | free daily, but **8K in / 4K out per request cap** | Only useful for tiny tasks |
| **Cerebras** | volatile (~2 models) | ⚠️ 1M-tokens/day tier replaced July 2026 by one-time $5 credit (payment method required) | No longer a clean free tier |

---

## 3. Ultra-Cheap High-Value Providers (PAID — bench only, never wire)

| Provider | Base URL | Recommended Models | Pricing | Notes |
| :--- | :--- | :--- | :--- | :--- |
| **DeepSeek Official** | `https://api.deepseek.com/v1` | `deepseek-chat`, `deepseek-reasoner` | ~$0.14 / 1M in (cache hit $0.014), $0.55 / 1M out | Acceptable last-resort paid rung with the human's own key — but per policy (2026-10-18) **nothing paid is wired into any ladder**; register + bench (enabled: false) only. |

**PAID-MODEL POLICY:** no paid model may appear in any virtual-model ladder. Paid providers are registered but **benched** (enabled: false). Manual benches persist to the DB (commit 77dc605) and survive reloads/restarts.

---

## 4. Recommended Pacing & Rate Limit Settings (proven live values)

```yaml
# NVIDIA NIM (local account)
rpm: 40
min_interval_ms: 2500
max_concurrency: 1

# Cloud relay twins (wire-cloud-*) — same NVIDIA models via second account/IP
rpm: 120
min_interval_ms: 100
max_concurrency: 6

# Gemini
rpm: 15
min_interval_ms: 2000
max_concurrency: 2

# Groq
rpm: 30
min_interval_ms: 1500
max_concurrency: 2

# OpenRouter / OpenCode Zen (free-labeled models)
rpm: 20
min_interval_ms: 2000-3000
max_concurrency: 2

# UnoRouter free rungs
rpm: 1
min_interval_ms: 60000

# Kilo free rungs
rpm: 10
min_interval_ms: 3000
max_concurrency: 2
```

Routing policy (tuned for free-tier bursts, all live, no restart): `stream_header_timeout_secs 22`, `nonstream_header_timeout_secs 300`, `fault_window_secs 180`, `overload_base_cooldown_secs 15`, `overload_max_cooldown_secs 90`, `circuit_failure_threshold 3`, `circuit_base_cooldown_secs 30`, `circuit_max_cooldown_secs 120`, `rate_limit_default_cooldown_secs 10`, `decay_interval_secs 180`, `severed_quarantine_secs 3600`, `severed_quarantine_max_secs 21600`, `severed_quarantine_backoff 2`.

---

## 5. Recommended Virtual Model Ladders (current proven topology)

One primary ladder + support groups. Sort rungs **largest context first** so big requests never bounce on small-context rungs; smallest context (e.g. Groq 131K) goes dead last.

### Ladder: `main` (primary for all agents; 25 rungs proven live)
1. **NVIDIA pool**: `nvidia-nemotron-ultra` → `wire-cloud-ultra` (relay twin) → `nvidia-nemotron-super` → `wire-cloud-super`
2. **UnoRouter nemotron twins** (extra account quota for the same models): `unorouter-nemotron-ultra-free` → `unorouter-nemotron-super-free`
3. **Gemini cushion**: `gemini-flash` (1M ctx, fast)
4. **Lightning pool**: `nvidia-nemotron-lightning` → `wire-cloud-lightning`
5. **LongCat**: `opencode-go-longcat` (1M ctx, free-labeled on OpenCode Go)
6. **Kilo free pool (1M)**: `kilo-nemotron-ultra-free` → `wire-cloud-kilo-ultra` → `kilo-nemotron-lightning-free` → `wire-cloud-kilo-lightning` → `kilo-inkling-small-free`
7. **512K**: `kilo-dots-free`
8. **256K class**: `kilo-nemotron-super-free` → `wire-cloud-kilo-super` → `unorouter-qwen-27b-free` → `kilo-laguna-s-free` → `kilo-cohere-code-free` → `wire-cloud-kilo-cohere` → `openrouter-cohere-code` → `kilo-auto/free`
9. **Smallest last**: `groq-gpt-oss-120b` (131K)

### Ladder: `embed` (OpenViking embeddings; NEVER mix dimensions)
1. `nvidia-embed` → `nvidia/nemotron-3-embed-1b` (2048 dims)
2. `unorouter-jina-v4-free` → `jina-embeddings-v4:free` (2048 dims — dimension-matched)

### Ladder: `openviking` (OpenViking summarizer/background; free first, workhorses as fallback)
`openrouter-free-auto` → uno nemotron rungs → `nvidia-nemotron-lightning` → `opencode-go-longcat`/`opencode-go-step` → `gemini-flash`

### Relay pattern (≈2× free quota — set this up, it's minutes of work)

Run a second WireLLM on your own cloud (Helm chart in `deploy/helm/wirellm`), holding a **second set of the same provider accounts**. Locally register `wire-cloud-*` providers pointing at `https://<relay-host>/v1` with the relay's gateway API key, slotting each relay twin immediately after its local twin.

**NVIDIA with two keys from two different accounts is the highest-value boost**: each NVIDIA account gets its own 40 RPM / 10,000 RPD free tier, so a second account on the relay instantly doubles the pool for the exact models the ladder grinds on (`nemotron-3-ultra`, `nemotron-3-super`, `nemotron-3.5-lightning` — all 1M context). Same works for Gemini and Groq keys.

Kilo needs no key at all — the relay's different IP alone doubles the 200 req/hr free pool.

Setup order:
1. Cloud auth prerequisite (Google OAuth client for the relay, callback URL registered).
2. Deploy the Helm chart via `scripts/publish.sh` (reads gitignored `.env.publish`: `CONTEXT`, `NS`, `IMAGE`, `SITE_URL`, `PLATFORM`).
3. On the relay, register the same providers with the second key set (vault secrets live per-instance), then `wirellm_create_api_key` for the local instance to authenticate with.
4. Human adds that relay key to the **local Vault** (e.g. `WIRE_RELAY_KEY`).
5. Locally, add the `wire-cloud-*` providers (`base_url = https://<relay-host>/v1`, `vault_secret = WIRE_RELAY_KEY`) and mirror the `main` ladder with twins interleaved after each local rung.

Proven live result: ~640 effective RPM / 82 concurrent slots across two instances — enough for multiple concurrent agent projects.

---

## 6. MCP JSON-RPC Setup Snippets

### 1. Store Key in Central Vault:
```json
{
  "jsonrpc": "2.0", "id": 1, "method": "tools/call",
  "params": {
    "name": "wirellm_set_secret",
    "arguments": { "name": "NVIDIA_API_KEY", "value": "nvapi-...", "description": "NVIDIA NIM developer token" }
  }
}
```

### 2. Discover Live Upstream Models:
```json
{
  "jsonrpc": "2.0", "id": 2, "method": "tools/call",
  "params": {
    "name": "wirellm_discover_provider_models",
    "arguments": { "base_url": "https://integrate.api.nvidia.com/v1", "vault_secret": "NVIDIA_API_KEY" }
  }
}
```

### 3. Register Provider — ONE model per provider:
```json
{
  "jsonrpc": "2.0", "id": 3, "method": "tools/call",
  "params": {
    "name": "wirellm_add_provider",
    "arguments": {
      "name": "nvidia-nemotron-ultra",
      "base_url": "https://integrate.api.nvidia.com/v1",
      "vault_secret": "NVIDIA_API_KEY",
      "models": ["nvidia/nemotron-3-ultra-550b-a55b"],
      "rpm": 40, "min_interval_ms": 2500, "max_concurrency": 1
    }
  }
}
```

### 3b. Key-less provider (Kilo free pool — no auth at all):
```json
{
  "jsonrpc": "2.0", "id": 4, "method": "tools/call",
  "params": {
    "name": "wirellm_add_provider",
    "arguments": {
      "name": "kilo-nemotron-ultra-free",
      "base_url": "https://api.kilo.ai/api/gateway/v1",
      "models": ["nvidia/nemotron-3-ultra-550b-a55b:free"],
      "headers": { "Authorization": "Bearer " },
      "rpm": 10, "min_interval_ms": 3000, "max_concurrency": 2
    }
  }
}
```
Omitting all key arguments also works (empty template → no Authorization header sent).

### 4. Register the `main` ladder:
```json
{
  "jsonrpc": "2.0", "id": 5, "method": "tools/call",
  "params": {
    "name": "wirellm_update_virtual_model",
    "arguments": {
      "name": "main",
      "description": "Primary ladder: NVIDIA pool + UnoRouter twins -> gemini -> lightning -> longcat -> Kilo free pool -> 256K class -> groq last",
      "targets": [
        { "provider": "nvidia-nemotron-ultra", "model": "nvidia/nemotron-3-ultra-550b-a55b" },
        { "provider": "wire-cloud-ultra", "model": "nvidia/nemotron-3-ultra-550b-a55b" },
        { "provider": "unorouter-nemotron-ultra-free", "model": "nemotron-3-ultra-550b-a55b:free" },
        { "provider": "gemini-flash", "model": "gemini-2.5-flash" },
        { "provider": "kilo-nemotron-ultra-free", "model": "nvidia/nemotron-3-ultra-550b-a55b:free" },
        { "provider": "openrouter-cohere-code", "model": "cohere/north-mini-code:free" },
        { "provider": "groq-gpt-oss-120b", "model": "openai/gpt-oss-120b" }
      ]
    }
  }
}
```

### 5. Bench a paid provider (persisted, survives restarts):
```json
{
  "jsonrpc": "2.0", "id": 6, "method": "tools/call",
  "params": {
    "name": "wirellm_set_provider_enabled",
    "arguments": { "name": "some-paid-provider", "enabled": false }
  }
}
```
