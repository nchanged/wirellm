# `oxllm-core` 🦀

[![Crates.io](https://img.shields.io/crates/v/oxllm-core.svg)](https://crates.io/crates/oxllm-core)
[![Docs.rs](https://docs.rs/oxllm-core/badge.svg)](https://docs.rs/oxllm-core)
[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

`oxllm-core` is the core library engine behind **`oxllm`** (Oxide LLM Proxy). It is a minimalist, ultra-low-footprint, high-performance, and concurrency-safe LLM adaptive routing gateway library written in Rust.

It is designed to be **fully generic and modular**, allowing you to build your own custom LLM proxies, local developer routers, or background networking daemons without any CLI or HTTP web framework dependencies.

---

## 🚀 Core Features

- **Zero Disk Persistence**: Operating state (cooldowns, consecutive failures, rate limits) is stored strictly in memory using granular asynchronous locks (`tokio::sync::RwLock`).
- **Thundering Herd Protection**: Enforces a strict `HalfOpen` circuit breaker state using lock-free atomic `compare_exchange` operations to prevent concurrent bursts from bombarding a recovering upstream provider.
- **Extensible Routing**: Abstracted behind the generic `RoutingStrategy` trait, allowing you to easily drop in custom round-robin, least-connections, or random failover algorithms.
- **Unix Shell Environment Expansion**: Custom zero-dependency TOML character parser that safely expands environment variable placeholders (e.g. `${MY_API_KEY}`) during configuration parsing.
- **OOM-Proof OpenTelemetry Ingest**: Non-blocking bounded event channel (`1024` capacity) utilizing a strict `try_send` strategy to gracefully discard telemetry spans if your collector goes offline, protecting edge system RAM.
- **W3C Trace Context Propagation**: Automated extraction, child span linking, and injection of W3C `traceparent` contexts for continuous distributed tracing.

---

## 🛠️ Usage Example

Here is how you can use `oxllm-core` to resolve candidate providers for a virtual model and select the healthiest option:

```rust
use std::sync::Arc;
use std::time::Duration;
use oxllm_core::config::Config;
use oxllm_core::state::{AppState, ProviderState, CircuitState};
use oxllm_core::router::{RoutingStrategy, AdaptivePriorityStrategy};
use reqwest::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Parse and validate your config
    let config = Config::load_from_file("config.toml")?;
    config.validate()?;

    // 2. Build your thread-safe, concurrent AppState
    let mut providers = Vec::new();
    for p in config.providers {
        if !p.enabled { continue; }
        let url = Url::parse(&p.base_url)?;
        providers.push(ProviderState::new(p.name, url, p.api_key, p.models));
    }

    let http_client = reqwest::Client::builder()
        .pool_idle_timeout(Duration::from_secs(90))
        .build()?;

    let app_state = AppState {
        providers,
        virtual_models: config.virtual_models,
        http_client,
    };

    // 3. Resolve candidates for a virtual model
    let candidates = app_state.resolve_candidates("free-tier");
    let candidate_states: Vec<&ProviderState> = candidates.iter().map(|(p, _)| p).collect();

    // 4. Select the healthiest provider using the Adaptive Priority Strategy
    let strategy = AdaptivePriorityStrategy;
    if let Some(selected) = strategy.select(&candidate_states).await {
        println!("Selected upstream: {} -> {}", selected.name, selected.base_url);
        
        // ... Make request ...
        
        // 5. Report success/failure feedback
        strategy.feedback(
            &candidate_states[0],
            true, // success
            selected.is_probe,
            Some(200), // HTTP status code
            None // optional Retry-After backoff duration
        ).await;
    }

    Ok(())
}
```

---

## 📄 License

Licensed under the Apache License, Version 2.0. See [LICENSE](https://github.com/planetf1/oxllm/blob/main/LICENSE) for details.
