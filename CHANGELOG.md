# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
