# WireLLM • Engineering Handoff & Project Guide

> **Generated**: October 14, 2026  
> **Official Repository**: [https://github.com/nchanged/wirellm](https://github.com/nchanged/wirellm) (`origin -> git@github.com:nchanged/wirellm.git`)  
> **Active Root Directory**: `/Users/nc/work/wirellm`  
> **Backward-Compatible Symlink**: `/Users/nc/work/oxllm -> /Users/nc/work/wirellm`  

---

## 1. Executive Summary & Runtime State

**WireLLM** is an ultra-low-overhead (<2ms wire latency, ~16–19 MB RSS memory footprint) Autonomous AI Model Gateway and Reverse Proxy built in Rust (Axum + Tokio + SQLite WAL) with an integrated Neon-Obsidian React dashboard and Model Context Protocol (MCP) server.

### Current Service Status
- **Daemon PID**: Run `cat ~/.wirellm/wirellm.pid` to check current PID.
- **Port**: `8765` (`http://localhost:8765/ui` for Dashboard, `http://localhost:8765/mcp/sse` for MCP SSE, `ws://localhost:8765/ws/live` for WebSocket).
- **Installed Binary**: `/Users/nc/.local/bin/wirellm` (Managed automatically by `./scripts/deploy.sh`).
- **Data & Config Directory**: `~/.wirellm/`
  - `~/.wirellm/wirellm.db` — SQLite WAL database storing runtime providers, virtual model ladders, daily token ledger, and request metrics.
  - `~/.wirellm/config.yaml` — Gateway server configuration.
  - `~/.wirellm/wirellm.log` — Rolling daemon execution log.
  - `~/.wirellm/wirellm.pid` — Active background process identifier.

---

## 2. Architecture & File Layout

```
/Users/nc/work/wirellm
├── crates/
│   ├── wirellm/                   # Main executable & API layer
│   │   ├── src/
│   │   │   ├── main.rs            # Entrypoint & CLI arguments
│   │   │   ├── server.rs          # Axum router, state, WS /ws/live, recent_streams buffer (<760 lines)
│   │   │   ├── admin.rs           # REST handlers for status, providers, virtual models, analytics
│   │   │   ├── service.rs         # Daemon lifecycle (start/stop/status/PID)
│   │   │   ├── mcp.rs             # Model Context Protocol SSE handler
│   │   │   └── routes/
│   │   │       ├── chat.rs        # Chat completions proxy, retry cascade, live streaming (<650 lines)
│   │   │       ├── models.rs      # Virtual models and provider listing endpoint
│   │   │       ├── embeddings.rs  # Embeddings proxy
│   │   │       └── logs.rs        # Live log reader
│   │   └── Cargo.toml
│   └── wirellm-core/              # Core routing, telemetry, and database logic
│       ├── src/
│       │   ├── lib.rs
│       │   ├── db.rs              # SQLite connection pool, migrations, daily_usage, request_metrics
│       │   ├── router.rs          # Cascade routing engine & target selection
│       │   ├── telemetry.rs       # OpenTelemetry spans & SQLite metrics worker (<310 lines)
│       │   ├── state.rs           # Atomic counters, circuit breaker states, in-flight tracking
│       │   ├── strategy.rs        # Feedback handling, cooldown calculation, burst spacing
│       │   └── config.rs          # Provider and virtual model schema definitions
│       └── Cargo.toml
├── frontend/                      # Neon-Obsidian React Dashboard (React 19 + Vite + Tailwind CSS)
│   ├── src/
│   │   ├── components/
│   │   │   ├── Overview.tsx       # Live status, telemetry metrics, routing ladders
│   │   │   ├── LiveAgentDeck.tsx  # Rolling 5-minute multi-agent stream interceptor & tool call visualizer
│   │   │   ├── AnalyticsView.tsx  # Token analytics, latency curves, granular model ledger
│   │   │   ├── ProvidersStudio.tsx# Provider CRUD, credentials, RPM, burst spacing
│   │   │   ├── VirtualModelsStudio.tsx # Cascade priority ladder builder
│   │   │   └── LogsView.tsx       # Real-time daemon log viewer
│   │   ├── types.ts               # Shared TypeScript schemas
│   │   ├── api.ts                 # Fetch & admin REST clients
│   │   └── App.tsx                # Tab navigation & live WebSocket integration
│   ├── dist/                      # Precompiled production frontend (embedded in binary)
│   └── package.json
├── scripts/
│   └── deploy.sh                  # Atomic zero-downtime hot-swap deployment script
├── start.sh                       # Local daemon CLI manager (start/stop/restart/status/logs)
└── README.md                      # Public project documentation & feature overview
```

---

## 3. Key Accomplishments & State in this Session

### A. Codebase Path Migration
- The repository was moved from `~/work/oxllm` to the canonical `~/work/wirellm` matching `git@github.com:nchanged/wirellm.git`.
- A symlink `/Users/nc/work/oxllm -> /Users/nc/work/wirellm` was created so any legacy path references remain fully operational.
- [`start.sh`](file:///Users/nc/work/wirellm/start.sh) was updated to dynamically locate the binary via `$SCRIPT_DIR` and `~/work/wirellm`.

### B. Live Agent Deck: Rolling 5-Minute Multi-Agent Interceptor
- **Multi-Agent 5-Minute Window**: Retains all agents that streamed or completed in the past 5 minutes (`300,000ms`).
- **De-duplicated per Agent Identity**: Grouped by `virtual_model:provider` (e.g. `code-reviewer:nvidia-nemotron-ultra`, `direct:nvidia-glm-flash`).
- **Relative Fired Timestamps**: Displays `Fired just now`, `Fired 24s ago`, `Fired 2m ago`.
- **Tool-Call Visualizer**: Highlights `🔧 call: tool_name(args)` in distinct amber badges with dark borders.
- **Backend Ring Buffer (`recent_streams`)**: Added in [`crates/wirellm/src/server.rs`](file:///Users/nc/work/wirellm/crates/wirellm/src/server.rs) so opening or refreshing the UI immediately hydrates the latest streams without waiting for a new request.
- **Char-Boundary UTF-8 Safe Delta Slicing**: Slices strings strictly on character boundaries using `.char_indices()`, avoiding panics on multi-byte UTF-8 symbols (em-dashes, emojis).

### C. Predictive Pacing vs Standby Cascade Disambiguation
- Clarified UI target evaluation states in `Overview.tsx`:
  - `ACTIVE (in-flight)` — Actively receiving or streaming tokens right now (pulsing cyan).
  - `PRIMARY READY` — Default root route (Priority 0) healthy and ready on standby.
  - `FAILOVER READY (#N)` — Upstream targets temporarily busy/rate-limited; standby failover armed and ready (amber badge).

### D. Failed Cascade Upstream Telemetry
- In [`crates/wirellm/src/routes/chat.rs`](file:///Users/nc/work/wirellm/crates/wirellm/src/routes/chat.rs), non-2xx status codes (e.g. HTTP 429, 500) and connection errors during the cascade loop now emit `RecordTransaction` events with `failure_reason: Some(...)`.
- This ensures failed upstream attempts are permanently recorded into `request_metrics` and `daily_usage` in SQLite WAL, giving 100% transparency in the Granular Model Ledger on the Analytics tab.

### E. Zero-Downtime Deployment
- Created [`scripts/deploy.sh`](file:///Users/nc/work/wirellm/scripts/deploy.sh):
  1. Builds frontend production bundle (`npm run build`).
  2. Compiles release binary (`cargo build --release`) while current daemon remains online serving traffic.
  3. Verifies binary integrity (`--version`).
  4. Atomically replaces `/Users/nc/.local/bin/wirellm`.
  5. Performs sub-second daemon restart (<200ms) with zero dropped requests.

---

## 4. Standard Operational Procedures

### Running Commands
- **Working Directory**: Always run commands in `/Users/nc/work/wirellm`.
- **Compiling**:
  ```bash
  cargo check --workspace
  cd frontend && npm run build && cd ..
  ```
- **Deploying with Zero Downtime**:
  ```bash
  ./scripts/deploy.sh
  ```
- **Daemon Management**:
  ```bash
  ./start.sh status
  ./start.sh logs
  ./start.sh restart
  ```
- **Checking SQLite WAL State**:
  ```bash
  sqlite3 ~/.wirellm/wirellm.db "SELECT * FROM daily_usage ORDER BY date DESC, total_requests DESC;"
  sqlite3 ~/.wirellm/wirellm.db "SELECT * FROM request_metrics ORDER BY id DESC LIMIT 10;"
  ```

---

## 5. Development Invariants & Constraints

1. **Strict File Line Limit**: Keep every Rust file strictly under ~700–800 lines of code. Modularize into clean submodules if any file approaches this ceiling.
2. **Memory Footprint**: Target RSS must remain flat between 12 MB and 20 MB. Never introduce unbounded background queues.
3. **No Window-Level Scrolling**: In UI components, never use `.scrollIntoView()`. Always use container-bounded `.scrollTop = container.scrollHeight` to prevent jumping.
4. **Theme Consistency**: High-contrast Neon Cyan (`#00f0ff`) on Obsidian Dark (`#020408`) with JetBrains Mono for metrics, code, and badges.
5. **Git Cleanliness**: Ensure `git status` is clean and all functional changes are pushed to `origin/main` (`git@github.com:nchanged/wirellm.git`).
