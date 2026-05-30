# CLAUDE.md - Developer Guide for `oxllm`

This guide outlines the commands and guidelines for building, testing, linting, and maintaining the `oxllm` (Oxide LLM Proxy) codebase.

## Build Commands
- **Check Compilation**: `cargo check`
- **Build Development Binary**: `cargo build`
- **Build Size-Optimized Release Binary**: `cargo build --release` (stripped binary size under 15 MB)

## Test Commands
- **Run All Tests**: `cargo test`
- **Run Unit Tests only**: `cargo test --lib`
- **Run Latency Performance Test**: `cargo test --test performance -- --nocapture`

## Formatting & Linting Commands
- **Enforce Code Formatting**: `cargo fmt`
- **Verify Formatting Check**: `cargo fmt --check`
- **Strict Clippy Lint Gate**: `cargo clippy --workspace --all-targets -- -D warnings`

## Code Guidelines
- **Pure Rust TLS**: Enforce pure-Rust TLS by compiling with reqwest's `rustls-tls` feature to avoid dynamially linking `OpenSSL` on targeted edge routers (OpenWrt/musl).
- **Zero unwraps**: Never use `.unwrap()` or `.expect()` in user-facing paths. Convert all errors to `OxllmError` or map them gracefully.
- **Lock-Free Hot Path**: Always preserve the lock-free atomic `probe_in_flight` permit when evaluating cooled down `HalfOpen` providers to defend against thundering herds under shared locks.
