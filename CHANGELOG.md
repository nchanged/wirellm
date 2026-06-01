# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.8] - 2026-06-01

### Fixed
  middleware regardless of bind address.
- `localhost_only` middleware now correctly recognizes IPv4-mapped IPv6
  loopback addresses (`::ffff:127.0.0.0/104`). This fixes CLI `oxllm status`
  failures when the server is bound to a dual-stack `[::]` socket.


  middleware regardless of bind address.


## [0.1.7] - 2026-06-01

### Fixed
- `oxllm --version` now reports the actual crate version from Cargo.toml
  instead of a hardcoded `0.1.0` string (broken since v0.1.5).
  Uses `env!("CARGO_PKG_VERSION")` via clap derive.

### Added
- Router-ready config with all API keys inlined (no shell variables) and
  Ollama fallback removed — written to `/etc/oxllm/config.toml`.
- systemd service file, update script (`/usr/local/bin/oxllm-update`).


## [0.1.6] - 2026-05-30

### Added
- Admin API: `POST /admin/providers/{name}/offline|online|reset` — runtime provider management.
- CLI: `oxllm provider list|offline|online|reset` subcommands — manage providers without curl.
- `oxllm provider list` — condensed provider status table.
- Friendly error messages when server not running (all CLI commands).
- XDG config path support (`~/.config/oxllm/config.toml` with `./config.toml` fallback).
- Provider guide: `docs/providers.md` — free-tier services, model names verified live (2026-05-30).
- Token counting from upstream JSON responses (non-streaming).

### Fixed
- Corrected model names for all 6 providers (verified via each provider's `/v1/models` endpoint).
- Gemini 2.5 Pro excluded (paid-only on free tier, quota = 0).
- Table column widths widened to fit 45-character model names.
- Last Request column width fixed (12 chars).
- Cleaned up debug `println!` statements from error paths.

### Changed
- Model names: updated to verified values (e.g. `llama-4-scout` → `meta-llama/llama-4-scout-17b-16e-instruct`).
- Ollama defaults: `granite4:micro` → `granite4.1:3b`.
- Default config searches XDG path before current directory.

### Documentation
- Full README overhaul: binary size, routing algorithm, CLI examples, telemetry section.
- Provider guide with snapshot date and research methodology.
- API endpoint table includes admin routes.

## [0.1.5] - 2026-05-30

### Added
- `oxllm stop` subcommand — gracefully stops the daemon via SIGTERM.
- `oxllm serve -v` / `-vv` — verbosity flags for per-request routing info or full trace.
- `POST /reload` HTTP endpoint — trigger config reload without shell access.
- `bind_family` config option (`"ipv4"`, `"ipv6"`, `"dual"`) — dual-stack IPv4/IPv6 binding.
- Last request time per provider — shown in `/status` and `oxllm status` ("Just now", "5m ago", etc.).
- Virtual model routing table in `/status` — shows each virtual model's fallback chain with per-hop health and counters.
- Circuit transition logging at `info!` level — see when circuits open, close, or rate-limit.

### Changed
- Per-request routing logs demoted from `info!` to `debug!` — default output is now quiet (errors and circuit transitions only). Use `-v` to see routing decisions.
- PID file cleaned up on graceful shutdown.

### Documentation
- Installation section restructured: Homebrew first (easiest), then `cargo install`, then source build.
- Full `oxllm status` output sample in README showing virtual model routing table.

## [0.1.4] - 2026-05-30

### Added
- Local per-provider request/success/token counters visible via `GET /status` and `oxllm status` — no external collector needed.
- `upstream_timeout_secs` config field in `[server]` section (default 5 seconds).
- Multi-tier `config.toml` with `smart`/`basic` virtual models and local Ollama fallback.
- Token counting from upstream JSON responses (non-streaming).
- crates.io publish workflow (tag-triggered, idempotent).

### Fixed
- Removed `println!` debug statements from error paths.
- Removed invalid `crates-io` value from `dist-workspace.toml`.

### Documentation
- Overhauled README with endpoint table, status output example, and quick start guide.

## [0.1.3] - 2026-05-30

### Fixed
- Proxy no longer crashes at startup when the OTLP collector is unreachable — telemetry exporter failure now logs a `WARN` and falls back to a silent no-op drain loop.
- `base_url` must end with a trailing `/v1/` path so relative URL joins produce correct endpoints (e.g. `http://localhost:11434/v1/` for Ollama).

### Documentation
- Added Ollama local-only example config to README with full self-contained setup instructions.
- Documented `base_url` trailing-slash convention and optional telemetry behaviour.


## [0.1.2] - 2026-05-30

### Fixed
- Calibrated CI coverage thresholds to match actual coverage (workspace 43%, oxllm-core 55%, oxllm 36%) — thresholds now set ~3pp below measured values so regressions are caught without false failures.

## [0.1.1] - 2026-05-30

### Added
- Configured automated security audit compliance scanning in workflows.

## [0.1.0] - 2026-05-30

### Added
- Core OpenAI-compatible chat completions proxy with full SSE streaming support.
- Embeddings proxy route with automatic reactive failover.
- Adaptive Priority Routing Strategy supporting circuit breakers, exponential backoffs, and idle-based decay aging.
- Strict lock-free thundering herd permit shielding using atomic operations.
- POSIX signal SIGHUP reloader watcher to hot-swap configuration on the fly.
- Graceful shutdown logic that drains active streaming clients on SIGINT/SIGTERM.
- Backpressure-safe, bounded OpenTelemetry span and metrics pipeline.
