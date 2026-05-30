# Provider Guide: Free-Tier LLM API Services

> **Snapshot: 2026-05-30** — Free-tier model availability, pricing, and rate limits
> change frequently. This guide reflects what was current at the snapshot date.
> Always verify current status on the provider's website before committing to a setup.

`oxllm` routes requests through a configurable pool of LLM providers. This guide documents the free-tier services supported by the default `config.toml`, what models are available, and their typical rate limits.

---

## Quick Reference

| Provider | Env Var | Sign Up | Free Tier? | Best For |
|---|---|---|---|---|
| **Groq** | `GROQ_API_KEY` | [console.groq.com/keys](https://console.groq.com/keys) | ✅ Yes | Fast inference, Llama models |
| **Google AI Studio** | `GOOGLE_API_KEY` | [aistudio.google.com/apikey](https://aistudio.google.com/apikey) | ✅ Yes | Gemini Flash/Pro, massive context window |
| **Cerebras** | `CEREBRAS_API_KEY` | [cloud.cerebras.ai](https://cloud.cerebras.ai) | ✅ Yes | Fastest inference latency |
| **SambaNova** | `SAMBANOVA_API_KEY` | [sambanova.ai](https://sambanova.ai) | ✅ Yes | Llama models, generous free tier |
| **OpenRouter** | `OPENROUTER_API_KEY` | [openrouter.ai/keys](https://openrouter.ai/keys) | ⚠️ Some free | Aggregator (check current free list) |
| **Ollama** (local) | None | [ollama.com](https://ollama.com) | ✅ Always free | Zero-cost local fallback |

---

## How Free Models Are Selected

The providers and models in `config.toml` were chosen using this methodology:

1. **Identify providers with a genuine free tier** — not just "first N tokens free" trial credits.
   Platforms like DeepSeek offer a one-time credit, not an ongoing free tier, and were excluded.
2. **Verify OpenAI API compatibility** — each provider must expose an OpenAI-compatible
   endpoint so the proxy can forward requests without protocol translation.
3. **Check model quality-to-speed ratio** — free models vary widely. The config balances
   powerful models (Gemini Pro, Llama Maverick) for quality with fast models (Llama Scout,
   Gemini Flash) for volume.
4. **Confirm rate limits are documented** — providers with opaque or undocumented rate
   limits are avoided to ensure predictable behavior.

To verify the current free tier status of any provider:

- **Check the provider's pricing page** for a "Free" or "Starter" tier.
  Be wary of "free trial" or "first N tokens free" — these expire.
- **Look for community reports** on forums like r/LocalLLaMA, Hacker News, or the
  provider's Discord. Free tiers that have existed for months are more trustworthy.
- **Test with a small request** — if you get a 402 Payment Required or 403 Forbidden,
  the model may no longer be free.
- **Monitor your rate limits** via `oxllm status` — if a provider consistently returns
  429s, you may have hit undocumented per-day caps.

---

## Provider Details

### Groq

Groq provides extremely fast inference using custom LPU hardware. Their free tier is generous and does not expire.

| Model | Tier in config | Notes |
|---|---|---|
| `llama-4-maverick` | `smart` (strong) | 128K context, competitive with GPT-4 class |
| `llama-4-scout` | `basic` (fast) | 1M context, excellent for bulk work |

**Free tier limits** (approximate): 30 requests/min, 15,000 tokens per day. Rate limits may change — check [Groq's console](https://console.groq.com).

**Env var:** `GROQ_API_KEY` — get yours at [console.groq.com/keys](https://console.groq.com/keys)

**Base URL:** `https://api.groq.com/openai/v1/`

---

### Google AI Studio (Gemini)

Google's Gemini API via AI Studio has one of the most generous free tiers available. No credit card required.

| Model | Tier in config | Notes |
|---|---|---|
| `gemini-2.5-pro` | `smart` (strong) | Powerful reasoning, 1M context window |
| `gemini-2.5-flash` | `basic` (fast) | Fast, cheap, 1M context |

**Free tier limits** (approximate): Gemini Flash — 1,500 requests/min, 1M tokens/min. Pro — lower but still generous. Check [AI Studio quotas](https://aistudio.google.com).

**Env var:** `GOOGLE_API_KEY` — get yours at [aistudio.google.com/apikey](https://aistudio.google.com/apikey)

**Base URL:** `https://generativelanguage.googleapis.com/v1beta/openai/`

> **Note:** Google's OpenAI-compatible endpoint requires a trailing `/v1beta/openai/` in the base URL.

---

### Cerebras

Cerebras uses wafer-scale chips for the lowest-latency inference available. Their free tier is genuinely free (no expiry).

| Model | Tier in config | Notes |
|---|---|---|
| `llama-3.3-70b` | `basic` (fast) | Very fast, competitive quality |

**Free tier limits**: Typically generous. Sign up at [cloud.cerebras.ai](https://cloud.cerebras.ai).

**Env var:** `CEREBRAS_API_KEY`

**Base URL:** `https://api.cerebras.ai/v1/`

---

### SambaNova

SambaNova provides fast Llama inference with a generous free tier.

| Model | Tier in config | Notes |
|---|---|---|
| `llama-3.3-70b` | `basic` (fast) | Good quality-to-speed ratio |

**Free tier limits**: Generous free tier. Sign up at [sambanova.ai](https://sambanova.ai).

**Env var:** `SAMBANOVA_API_KEY`

**Base URL:** `https://api.sambanova.ai/v1/`

---

### OpenRouter

OpenRouter aggregates many providers. Some models are free (community-hosted), while others are pay-per-token. Free models change frequently.

**Current free models** (check [openrouter.ai/models](https://openrouter.ai/models?order=price&direction=asc) for the latest):

Common free community models include:
- `mistral-7b-instruct` (free tier)
- `google/gemma-2-2b-it` (free tier)
- Various community-hosted fine-tunes

**Env var:** `OPENROUTER_API_KEY` — get yours at [openrouter.ai/keys](https://openrouter.ai/keys)

**Base URL:** `https://openrouter.ai/api/v1/`

> **Note:** OpenRouter adds a small markup to paid models. For truly free usage, filter by price = $0 on their models page.

---

### Ollama (Local)

Ollama runs models entirely on your machine. No API key, no rate limits, no network needed. Perfect as a last-resort fallback.

| Model | Notes |
|---|---|
| `granite4:micro` | 1.9 GB, ~1B params, fast on any hardware |

**Setup:**
```bash
# Install Ollama
brew install ollama

# Pull the fallback model used in config.toml
ollama pull granite4:micro
```

**Base URL:** `http://localhost:11434/v1/`

---

## Tier Architecture

The default `config.toml` defines two virtual models with automatic failover:

### `smart` — Maximum Quality
```
google-strong (gemini-2.5-pro)
  → groq-strong (llama-4-maverick)
    → google-basic (gemini-2.5-flash)     ← fallback
      → groq-basic (llama-4-scout)        ← fallback
        → cerebras-basic (llama-3.3-70b)  ← fallback
          → sambanova-basic (llama-3.3-70b) ← fallback
            → openrouter-basic             ← fallback
              → ollama-fallback            ← last resort
```

If every cloud provider is rate-limited or down, requests cascade all the way to your local Ollama instance. You always get an answer — it just might be from a very small model.

### `basic` — Fast & Cheap
```
groq-basic (llama-4-scout)
  → google-basic (gemini-2.5-flash)
    → cerebras-basic (llama-3.3-70b)
      → sambanova-basic (llama-3.3-70b)
        → openrouter-basic
          → ollama-fallback
```

Bypasses the expensive strong models entirely. Use for high-volume bulk work.

---

## Getting Started

1. **Sign up** for the providers you want (links in the table above)
2. **Set environment variables** with your API keys:
   ```bash
   export GROQ_API_KEY="gsk_..."
   export GOOGLE_API_KEY="AIza..."
   export CEREBRAS_API_KEY="..."
   export SAMBANOVA_API_KEY="..."
   # Optional:
   export OPENROUTER_API_KEY="sk-or-..."
   ```
3. **Start the proxy:**
   ```bash
   oxllm serve
   ```
4. **Test it:**
   ```bash
   curl -X POST http://127.0.0.1:8080/v1/chat/completions \
     -H "Content-Type: application/json" \
     -d '{"model": "smart", "messages": [{"role": "user", "content": "Hello!"}]}'
   ```

The circuit breaker handles rate limits automatically — if one provider returns a 429, the proxy transparently fails over to the next in the chain without any client-visible error.
