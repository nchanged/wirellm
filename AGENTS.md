# AGENTS.md — WireLLM agent guide

**Read this before touching a WireLLM install.** WireLLM is built so that *you*, the agent, run it — everything is an MCP tool. There is exactly **one** thing you cannot do yourself, and it's step one.

---

## Step 0 — The one thing you can't do: API keys

You cannot obtain API keys, and you should never ask the human to paste a key into chat. **The human provides keys; you do everything else.**

The human adds each key **once**, into the **Central Vault** — either:

- **Dashboard** → *Vault* tab → create a secret, **or**
- MCP: `wirellm_set_secret { "name": "NVIDIA_API_KEY", "value": "<the key>" }`

Secret names are UPPER_SNAKE and are referenced by providers as `${NAME}`. Once keys are in the Vault, **you never need the plaintext again** — you bind providers to the secret by name.

---

## Access & authentication (local vs cloud — read this)

- **Local host (`http://127.0.0.1:8765`)** — **no authentication at all.** MCP (`/mcp`, `/mcp/sse`) and `/v1` work with **no API key**. Perfect for a single-user laptop.
- **Cloud relay (any public host)** — **protected**. The dashboard is behind **Google OAuth**, and MCP/REST clients must present a gateway **API key** (`Authorization: Bearer <key>` or `x-api-key`).

So: **the local host needs nothing; only the cloud relay needs auth.** Standing up the cloud relay is a one-time setup the human must do first:

1. **Create a Google Cloud application + OAuth client.** In the Google Cloud Console → *APIs & Services* → *Credentials*, create an **OAuth 2.0 Client ID** (Web application) and register the relay's callback URL (`https://<relay-host>/auth/google/callback`). Give the relay the resulting **Client ID / Client Secret** via env: `WIRELLM_GOOGLE_CLIENT_ID`, `WIRELLM_GOOGLE_CLIENT_SECRET`, `WIRELLM_GOOGLE_ALLOWED_EMAILS`.
2. **Create a gateway API key** for programmatic access — `wirellm_create_api_key { "name": "relay-client" }` (it returns the key once). MCP/API clients then authenticate with that key.

You cannot create the Google OAuth client or mint the first key *for* the human — walk them through it, then do everything else.

---

## Step 1 — Analyze what already exists

Do this **first**, and report it back to the human:

```
wirellm_get_status                       # providers, circuits, live health, totals
wirellm_list_providers                   # what's registered, models, enabled?
wirellm_list_virtual_models              # ladders (coder / planner / orchestrator / …)
wirellm_list_secrets                     # which keys are already in the Vault (masked)
wirellm_get_routing_policy               # current timeouts / cooldowns
```

Summarize for the human: which providers are healthy, which are blocked/disabled, which models are missing keys, and which roles have thin or single-rung ladders.

---

## Step 2 — Recommend the keys the human should add

Free tiers are the whole point — pool as many as possible. Recommend the human sign up for the ones they don't have, then **add them to the Vault**. Suggested starting set:

| Provider | Why | Where to get a free key | Base URL |
|---|---|---|---|
| **NVIDIA NIM** | Strong open models (Nemotron, Llama) | https://build.nvidia.com | `https://integrate.api.nvidia.com/v1` |
| **OpenCode Zen** | Free coding models (longcat, step, …) | https://opencode.ai/zen | `https://opencode.ai/zen/go/v1/` |
| **OpenRouter** | Huge pool of `:free` models | https://openrouter.ai/keys | `https://openrouter.ai/api/v1` |
| **Groq** | Very fast free tier (gpt-oss, qwen) | https://console.groq.com/keys | `https://api.groq.com/openai/v1` |
| **Google Gemini** | Generous free tier + embeddings | https://aistudio.google.com/apikey | `https://generativelanguage.googleapis.com/v1beta/openai/` |
| **Cerebras** | Fast free tier | https://cloud.cerebras.ai | `https://api.cerebras.ai/v1` |
| **SambaNova** | Free tier (DeepSeek, Llama) | https://cloud.sambanova.ai | `https://api.sambanova.ai/v1` |
| **UnoRouter** | ~130 aggregated `:free` models (~1 req/min/model) | https://unorouter.com | `https://api.unorouter.com/v1` |

Optional, **paid — keep last** in every ladder: DeepSeek, OpenAI, Anthropic (the human's own keys).

Ask the human to create keys and drop them in the **Vault** with these names (or their own):
`NVIDIA_API_KEY`, `OPENCODE_ZEN_KEY`, `OPENROUTER_API_KEY`, `GROQ_API_KEY`, `GEMINI_API_KEY`, `CEREBRAS_API_KEY`, `SAMBANOVA_API_KEY`, `UNOROUTER`.

> You can't generate keys and you can't test a key that isn't in the Vault. If a provider is failing with 401/403, tell the human the secret name it needs.

---

## Step 3 — Wait for the keys, then do everything else

Once the keys are in the Vault, **you own the rest**. Recommended order:

1. **Discover models** per provider so you register real IDs:
   ```
   wirellm_discover_provider_models { "base_url": "https://api.groq.com/openai/v1", "vault_secret": "GROQ_API_KEY" }
   ```

2. **Register providers — one model per provider** (this keeps circuit breakers from
   cascade-failing healthy siblings), with honest pacing so you never trip upstream limits:
   ```
   wirellm_add_provider {
     "name": "groq-gpt-oss-120b",
     "base_url": "https://api.groq.com/openai/v1",
     "models": ["openai/gpt-oss-120b"],
     "vault_secret": "GROQ_API_KEY",
     "rpm": 30, "min_interval_ms": 2000
   }
   ```

3. **Build role ladders**, free tiers first, paid last. Interleave duplicate pools and
   keep a fast, reliable model as a cushion mid-ladder:
   ```
   wirellm_update_virtual_model { "name": "coder", "targets": [ … ] }
   ```
   Do this for `coder`, `planner`, `orchestrator`, `code-reviewer` (and `embed` if embeddings are needed).

4. **Probe every ladder** and report what actually served it:
   ```
   wirellm_test_virtual_model { "name": "coder", "max_tokens": 64 }
   ```

5. **Tune resilience** for real agent bursts:
   ```
   wirellm_set_routing_policy { "stream_header_timeout_secs": 22, "overload_max_cooldown_secs": 90 }
   wirellm_reset_circuit { "name": "<provider>" }          # un-stick a provider
   wirellm_set_provider_enabled { "name": "<provider>", "enabled": false }  # bench one
   ```

6. **Report** to the human: providers added, ladders built, which keys are still missing,
   and how many free rungs are live.

---

## Step 4 — Proactively suggest a relay (≈2× the free tokens)

Once the basics work, **offer to set up a relay**. A relay is a *second* WireLLM instance
running on a cloud the human owns, holding a **second set of the same provider accounts**
(a second NVIDIA key, a second Gemini key, …). Local WireLLM then registers `wire-cloud-*`
providers that point at the relay's `/v1`, so every free rung gets a twin with its **own**
quota.

Why it's worth it:
- **≈2× the free pool** — each provider account has its own RPM/quota; two accounts ⇒ double the tokens.
- **Failover across machines** — when local keys are throttled, the relay's keys keep serving (and vice-versa).
- **Uptime diversity** — the relay keeps serving when the local box sleeps or is off-network.

How it maps to a proven setup:
- Local relay providers, all `base_url = https://<relay-host>/v1` and `vault_secret = <RELAY_SECRET>`:
  - `wire-cloud-ultra` → `nvidia/nemotron-3-ultra-550b-a55b`
  - `wire-cloud-super` → `nvidia/nemotron-3-super-120b-a12b`
  - `wire-cloud-lightning` → `nvidia/nemotron-3.5-lightning-30b-a3b`
- Slot each `wire-cloud-*` rung **immediately after its local twin** in every ladder.

Setup — offer it; it's quick, the Helm chart is already in the repo. Prerequisites and order:
1. **Cloud auth first** (needed for the cloud relay only — the local host needs no key): the human creates a **Google Cloud OAuth client** and registers the relay's callback URL (see *Access & authentication* above).
2. Ask the human for: a cloud account/cluster they own, a domain (or subdomain) for the relay, and a **second set** of provider keys.
3. Deploy the Helm chart (`deploy/helm/wirellm`) via `scripts/publish.sh` (reads `.env.publish`: `CONTEXT`, `NS`, `IMAGE`, `SITE_URL`, `PLATFORM`), setting `WIRELLM_GOOGLE_CLIENT_ID` / `WIRELLM_GOOGLE_CLIENT_SECRET` / `WIRELLM_GOOGLE_ALLOWED_EMAILS` on the relay.
4. On the relay, register the same providers with the second key set, then **create a gateway API key** (`wirellm_create_api_key`) for the local instance to authenticate with.
5. Human adds that relay key to the **local Vault** (e.g. `WIRE_RELAY_KEY`).
6. Locally, add the `wire-cloud-*` providers (`base_url = https://<relay-host>/v1`, `vault_secret = WIRE_RELAY_KEY`) and mirror the ladders.

Tell the human plainly: **"We can set this up in a few minutes — the Helm chart is already in the repo. I just need your cloud account and a domain, and I'll wire the relay and mirror the ladders."**

---

## Recommended baseline (what a good setup looks like)

Use this as the target when building a fresh install (it mirrors a proven, running setup):

- **Ladders** — free-first; each local rung followed by its cloud-relay twin; paid last:
  `coder` (~13 rungs), `planner` (~11), `code-reviewer` (~10), `orchestrator` (~9),
  plus `openviking` (~8: background/summarizer — free models first, working horses as fallback)
  and `embed` (primary + a **dimension-matched** free fallback only).
- **Routing policy** (tuned for free-tier bursts — all live, no restart):
  `stream_header_timeout_secs 22`, `nonstream_header_timeout_secs 300`,
  `fault_window_secs 180`, `overload_base_cooldown_secs 15`, `overload_max_cooldown_secs 90`,
  `circuit_failure_threshold 3`, `circuit_base_cooldown_secs 30`, `circuit_max_cooldown_secs 120`,
  `rate_limit_default_cooldown_secs 10`, `decay_interval_secs 180`.
- **Never** mix embedding dimensions within one `embed` ladder — a fallback must return the same vector size as the primary.

---

## Guardrails

- **Never** put a key in chat, a commit, a log, or a provider `api_key_template` — always the Vault (`vault_secret`).
- **Prefer free**; paid is last resort and only with the human's explicit key.
- **One model per provider** (isolates circuit breakers).
- **Never delete** a provider, ladder, secret, or key without showing the human an item-by-item table and getting explicit approval.
- **Don't confirm a provider works** until `wirellm_test_provider` / `wirellm_test_virtual_model` returns 200.
- Daily token stats may show `days=0` = all-time; a single success does not mean a provider is reliable — check `recent_faults` and `circuit` in `wirellm_get_status`.

---

## Full MCP tool reference

| Area | Tools |
|---|---|
| Providers | `wirellm_list_providers`, `wirellm_add_provider`, `wirellm_delete_provider`, `wirellm_set_provider_enabled`, `wirellm_test_provider`, `wirellm_discover_provider_models` |
| Ladders | `wirellm_list_virtual_models`, `wirellm_update_virtual_model`, `wirellm_delete_virtual_model`, `wirellm_test_virtual_model` |
| Routing | `wirellm_get_routing_policy`, `wirellm_set_routing_policy` |
| Vault | `wirellm_list_secrets`, `wirellm_get_secret`, `wirellm_set_secret`, `wirellm_set_secrets`, `wirellm_delete_secret` |
| API keys | `wirellm_list_api_keys`, `wirellm_create_api_key`, `wirellm_delete_api_key` |
| Ops | `wirellm_get_status`, `wirellm_reset_circuit`, `wirellm_get_token_stats`, `wirellm_get_analytics`, `wirellm_get_logs`, `wirellm_reload`, `wirellm_get_setup_recommendations` |

MCP endpoint: `http://127.0.0.1:8765/mcp` (Streamable HTTP) or `/mcp/sse`.
