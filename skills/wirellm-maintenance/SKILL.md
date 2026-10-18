---
name: wirellm-maintenance
description: "Maintenance audit for WireLLM: probe every enabled provider rung for gone/dead models, diff upstream catalogs for new free models (incl. OpenCode Go free tier), sort opportunities best-first (1M ctx), and dispatch the wirellm-model-scout subagent to scan freellmapi.co for what's new or missing"
---

<objective>
Run a full maintenance cycle over the WireLLM install: health snapshot → rung probing (what's gone?) → catalog diffing (what's new?) → subagent scout report (freellmapi.co + other catalogs) → sorted suggestions for the user. Report-only by default: nothing is added, removed, or reordered without explicit user approval.
</objective>

<prerequisites>
- WireLLM MCP server connected (wirellm-local, optionally wirellm-remote for the relay)
- The `wirellm-model-scout` agent available. It ships with this skill at `agents/wirellm-model-scout.md` — if it's not installed, copy it to `~/.config/opencode/agents/wirellm-model-scout.md` (global) or the project's `.opencode/agent/` directory and reload.
</prerequisites>

<process>

## Step 1 — Snapshot the current state

Gather (in parallel where possible):
```
wirellm_get_status            # providers, circuits, recent_faults, lifetime successes
wirellm_list_providers        # registered providers + enabled flags
wirellm_list_virtual_models   # current main / embed / openviking compositions
wirellm_list_secrets          # which keys exist (masked)
```
Note which providers are benched (enabled: false) and confirm the paid ones **stay** benched (`opencode-qwen-max`, `router9-probe` on the reference install).

## Step 2 — Probe every enabled rung (what's gone?)

For each enabled provider, batched 4-6 at a time (never paid/benched ones):
```
wirellm_test_provider { "name": "<provider>", "max_tokens": 16 }
```
Classify each result:
- **200** → healthy (record `served_by` — free pools may substitute; note deviations)
- **429** → alive but rate-limited (transient; note Retry-After)
- **401 / 403 `INVALID_TOKEN` / plan-upgrade errors** → severed key or tier change → needs the human
- **404 / 410** → model likely gone → verify against the upstream catalog in Step 3 before proposing removal (Kilo's catalog especially can 404 live models and serve unlisted ones)

## Step 3 — Catalog diff per provider (what's new?)

For each distinct base_url among enabled providers:
```
wirellm_discover_provider_models { "base_url": "<url>", "vault_secret": "<SECRET>" }
```
Diff against the registered model ids:
- **Registered but absent from catalog** → candidate "gone" (cross-check with Step 2 status)
- **In catalog but not registered** → candidate "new" — filter to free-family only: ids ending `:free`, OpenCode ids ending `-free`, and already-free families (nemotron, gemma). Flag anything new that would need a NEW signup as "requires new key from human".
- Record `recommended_coding` / `recommended_fast` / `recommended_reasoning` from discovery.

Special: for the OpenCode Go endpoint (`https://opencode.ai/zen/go/v1`), list the catalog and call out **new `-free`-labeled models**. Apply the harness-lock caveat: new `-free` OpenCode models are almost certainly locked to the OpenCode harness (403 from outside) except the grandfathered rungs — mark them "harness-locked, likely unusable via API" unless a probe proves otherwise. Never assume; probe costs nothing on free models but the lock is server-side.

## Step 4 — Dispatch the Model Scout subagent

Compose a message for the scout containing:
1. The registered provider list with model ids + enabled/benched state (from Step 1)
2. The current ladder compositions (from Step 1)
3. The vault secret names (from Step 1)
4. Any suspected-gone or suspected-new findings from Steps 2-3 for cross-checking

Launch the subagent:
```
@wirellm-model-scout <the composed message + the scout's own data-source instructions apply>
```
(In OpenCode use the task/subagent tool with agent `wirellm-model-scout`; if spawning programmatically use agent type `general` with model `opencode-go/glm-5.3-flash` or `wirellm/main` and paste the agent's full instructions from `~/.config/opencode/agents/wirellm-model-scout.md` into the prompt.)

The scout returns the four-section report: Gone/Dead, New, Missing, Suggestions.

## Step 5 — Merge, sort, and report to the user

Merge the scout's findings with your own Steps 2-3 results and produce the **Maintenance Report**:

1. **Health table** — rung | status | served_by | latency | classification
2. **Gone / Dead** — with proposed action (bench / remove rung / keep-transient)
3. **New free models** — sorted best-first: **largest context on top (1M wins)**, then expected quality; each with: exact upstream id, source, which vault secret could serve it (or "no key needed"), suggested provider name + pacing
4. **Ladder suggestions** — wiring slots under the largest-context-first rule for `main` (and `embed` only if dimension-matched 2048)
5. **Relay note** — the relay is optional; only mention mirroring if the user wants to act on new models

**Hard rule:** close the report with "Nothing was changed — say the word and I'll wire/bench/remove the items you approve." Any actual change (add provider, update ladder, bench/remove) requires the user's explicit go-ahead in that same session.

</process>

<success_criteria>
- Every enabled rung probed and classified with evidence
- Every distinct upstream catalog diffed for new and gone models
- Scout subagent dispatched and its report merged
- New opportunities sorted largest-context-first (1M on top)
- Zero modifications without explicit user approval; report closes with the approval prompt
</success_criteria>
