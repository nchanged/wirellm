# `wirellm-core` 🦀

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)

`wirellm-core` is the core library engine behind **WireLLM**. It provides adaptive routing, circuit breaker state machines, encrypted vault key management, SQLite state persistence, and OpenTelemetry instrumentation.

---

## 🚀 Core Features

- **Lock-free Concurrency**: Operating state (cooldowns, consecutive failures, rate limits) evaluated using atomic operations and granular asynchronous locks (`tokio::sync::RwLock`).
- **Thundering Herd Protection**: Enforces a strict `HalfOpen` circuit breaker state using lock-free atomic `compare_exchange` operations to prevent concurrent bursts from bombarding a recovering upstream provider.
- **Adaptive Routing Engine**: Proactive rate limiting, RPM burst tracking, and health decay.
- **AES-256-GCM Vault**: Machine-salted symmetric encryption for secure API credential storage on disk with `0600` permissions.
- **SQLite WAL Storage**: Embedded persistence for provider definitions, virtual model target chains, and daily token metrics.
- **OOM-Proof OpenTelemetry Ingest**: Non-blocking bounded event channel (`1024` capacity) utilizing a strict `try_send` strategy to gracefully discard telemetry spans if your collector goes offline.

---

## 📄 License

Licensed under the Apache License, Version 2.0. See [LICENSE](../../LICENSE) for details.
