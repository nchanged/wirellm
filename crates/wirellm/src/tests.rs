use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::RwLock;

use axum::middleware;
use tower_http::cors::{Any, CorsLayer};
use serde_json::Value;
use reqwest::Url;

use wirellm_core::config::VirtualModelTarget;
use wirellm_core::state::{AppState, CircuitState, FaultHistogram, ProviderState};
use wirellm_core::telemetry::TelemetryClient;

use crate::admin;
use crate::mcp;
use crate::routes;
use crate::server::*;


    async fn spawn_mock_upstream(responses: Vec<String>) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let responses = Arc::new(responses);
            let mut idx = 0;

            while let Ok((mut stream, _)) = listener.accept().await {
                let responses = responses.clone();
                let current_idx = idx;
                idx += 1;

                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let _ = stream.read(&mut buf).await;

                    let response = if responses.is_empty() {
                        "HTTP/1.1 500 Internal Error\r\nContent-Length: 0\r\n\r\n"
                    } else {
                        let r_idx = current_idx.min(responses.len() - 1);
                        &responses[r_idx]
                    };

                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.flush().await;
                });
            }
        });

        addr
    }

    #[tokio::test]
    async fn test_integration_rate_limit_failover() {
        let upstream1_resp = vec![
            "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 1\r\nContent-Length: 0\r\n\r\n"
                .to_string(),
        ];
        let upstream2_resp = vec![
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 75\r\n\r\n{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"Hello from prov2\"}}]}".to_string()
        ];

        let addr1 = spawn_mock_upstream(upstream1_resp).await;
        let addr2 = spawn_mock_upstream(upstream2_resp).await;

        let p1 = ProviderState {
            name: "prov1".to_string(),
            base_url: Url::parse(&format!("http://{}", addr1)).unwrap(),
            api_key: "key1".to_string(),
            models: vec!["gpt-4-upstream".to_string()],
            headers: std::collections::HashMap::new(),
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            manual_disabled: AtomicBool::new(false),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };

        let p2 = ProviderState {
            name: "prov2".to_string(),
            base_url: Url::parse(&format!("http://{}", addr2)).unwrap(),
            api_key: "key2".to_string(),
            models: vec!["gpt-4-upstream".to_string()],
            headers: std::collections::HashMap::new(),
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            manual_disabled: AtomicBool::new(false),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };

        let mut virtual_models = std::collections::HashMap::new();
        virtual_models.insert(
            "gpt-4".to_string(),
            vec![
                VirtualModelTarget {
                    provider: "prov1".to_string(),
                    model: "gpt-4-upstream".to_string(),
                },
                VirtualModelTarget {
                    provider: "prov2".to_string(),
                    model: "gpt-4-upstream".to_string(),
                },
            ],
        );

        let http_client = reqwest::Client::builder().build().unwrap();
        let app_state = Arc::new(AppState {
            providers: vec![p1, p2],
            virtual_models,
            http_client,
            upstream_timeout_secs: 5,
            auth: Default::default(),
        });

        let (_watch_sender, watch_receiver) = tokio::sync::watch::channel(app_state.clone());
        let (telemetry_tx, _telemetry_rx) = tokio::sync::mpsc::channel(1024);
        let telemetry_client = TelemetryClient::new(telemetry_tx);

        let dummy_reloader = Reloader::test_new(_watch_sender.clone());
        let reloadable_state = ReloadableState::test_new(watch_receiver, telemetry_client, dummy_reloader);

        let router = axum::Router::new()
            .route(
                "/v1/chat/completions",
                axum::routing::post(routes::create_chat_completions),
            )
            .layer(middleware::from_fn(add_request_id))
            .with_state(reloadable_state);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();
        let payload = serde_json::json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "hi"}]
        });

        let res = client
            .post(format!("http://{}/v1/chat/completions", proxy_addr))
            .json(&payload)
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 200);

        let body: Value = res.json().await.unwrap();
        let content = body["choices"][0]["message"]["content"].as_str().unwrap();
        assert_eq!(content, "Hello from prov2");

        let p1_limited = app_state.providers[0].rate_limited_until.read().await;
        assert!(p1_limited.is_some());
    }

    #[tokio::test]
    async fn test_integration_sse_streaming() {
        let sse_response = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\ndata: {\"token\": \"Hello\"}\n\ndata: {\"token\": \" World\"}\n\n".to_string();

        let addr = spawn_mock_upstream(vec![sse_response]).await;

        let p = ProviderState {
            name: "prov1".to_string(),
            base_url: Url::parse(&format!("http://{}", addr)).unwrap(),
            api_key: "key1".to_string(),
            models: vec!["gpt-4-upstream".to_string()],
            headers: std::collections::HashMap::new(),
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            manual_disabled: AtomicBool::new(false),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };

        let mut virtual_models = std::collections::HashMap::new();
        virtual_models.insert(
            "gpt-4".to_string(),
            vec![VirtualModelTarget {
                provider: "prov1".to_string(),
                model: "gpt-4-upstream".to_string(),
            }],
        );

        let http_client = reqwest::Client::builder().build().unwrap();
        let app_state = Arc::new(AppState {
            providers: vec![p],
            virtual_models,
            http_client,
            upstream_timeout_secs: 5,
            auth: Default::default(),
        });

        let (_watch_sender, watch_receiver) = tokio::sync::watch::channel(app_state.clone());
        let (telemetry_tx, _telemetry_rx) = tokio::sync::mpsc::channel(1024);
        let telemetry_client = TelemetryClient::new(telemetry_tx);

        let dummy_reloader = Reloader::test_new(_watch_sender.clone());
        let reloadable_state = ReloadableState::test_new(watch_receiver, telemetry_client, dummy_reloader);

        let router = axum::Router::new()
            .route(
                "/v1/chat/completions",
                axum::routing::post(routes::create_chat_completions),
            )
            .layer(middleware::from_fn(add_request_id))
            .with_state(reloadable_state);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();
        let payload = serde_json::json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "hi"}],
            "stream": true
        });

        let mut res = client
            .post(format!("http://{}/v1/chat/completions", proxy_addr))
            .json(&payload)
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 200);
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "text/event-stream"
        );

        let mut body = String::new();
        while let Some(chunk) = res.chunk().await.unwrap() {
            body.push_str(std::str::from_utf8(&chunk).unwrap());
        }

        assert!(body.contains("data: {\"token\": \"Hello\"}"));
        assert!(body.contains("data: {\"token\": \" World\"}"));
    }

    // -----------------------------------------------------------------------
    // New integration tests: timeout failover, all-providers-fail, manual
    // offline, rate-limit recovery, embeddings endpoint
    // -----------------------------------------------------------------------

    /// Circuit breaker trips after 3 failures; failover routes to healthy provider.
    #[tokio::test]
    async fn test_integration_circuit_breaker_failover() {
        use wirellm_core::router::{AdaptivePriorityStrategy, RoutingStrategy};

        let p1 = ProviderState {
            name: "primary".into(),
            base_url: Url::parse("https://api.fail.example.com/v1/").unwrap(),
            api_key: "key".into(),
            models: vec!["model".into()],
            headers: std::collections::HashMap::new(),
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            manual_disabled: AtomicBool::new(false),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };

        let p2 = ProviderState {
            name: "secondary".into(),
            base_url: Url::parse("https://api.ok.example.com/v1/").unwrap(),
            api_key: "key".into(),
            models: vec!["model".into()],
            headers: std::collections::HashMap::new(),
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            manual_disabled: AtomicBool::new(false),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };

        let strategy = AdaptivePriorityStrategy;
        let candidates = vec![&p1, &p2];

        // Initially selects primary
        let selected = strategy.select(&candidates).await.unwrap();
        assert_eq!(selected.name, "primary");

        // Simulate 3 failures on primary
        for _ in 0..3 {
            strategy.feedback(&p1, false, false, Some(500), None).await;
        }

        // Circuit should be open now; select should skip to secondary
        assert!(matches!(
            *p1.circuit.read().await,
            CircuitState::Open { .. }
        ));
        let selected = strategy.select(&candidates).await.unwrap();
        assert_eq!(selected.name, "secondary");
    }

    /// When every provider fails, the proxy returns 502 Bad Gateway.
    #[tokio::test]
    async fn test_integration_all_providers_fail_gives_502() {
        let fail_response =
            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n".to_string();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(fail_response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        let make_provider = |name: &str| ProviderState {
            name: name.into(),
            base_url: Url::parse(&format!("http://{}/v1/", addr)).unwrap(),
            api_key: "key".into(),
            models: vec!["model".into()],
            headers: std::collections::HashMap::new(),
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            manual_disabled: AtomicBool::new(false),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };

        let p1 = make_provider("p1");
        let p2 = make_provider("p2");

        let mut virtual_models = std::collections::HashMap::new();
        virtual_models.insert(
            "test".into(),
            vec![
                VirtualModelTarget {
                    provider: "p1".into(),
                    model: "model".into(),
                },
                VirtualModelTarget {
                    provider: "p2".into(),
                    model: "model".into(),
                },
            ],
        );

        let app_state = Arc::new(AppState {
            providers: vec![p1, p2],
            virtual_models,
            http_client: reqwest::Client::builder().build().unwrap(),
            upstream_timeout_secs: 5,
            auth: Default::default(),
        });

        let (_ws, wr) = tokio::sync::watch::channel(app_state.clone());
        let (ttx, _trx) = tokio::sync::mpsc::channel(1024);
        let reloader = Reloader::test_new(_ws);
        let state = ReloadableState::test_new(wr, TelemetryClient::new(ttx), reloader);
        let router = axum::Router::new()
            .route(
                "/v1/chat/completions",
                axum::routing::post(routes::create_chat_completions),
            )
            .layer(middleware::from_fn(add_request_id))
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();
        let res = client
            .post(format!("http://{}/v1/chat/completions", proxy_addr))
            .json(&serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 502);
        let body = res.text().await.unwrap();
        assert!(body.contains("All upstream chat completions providers failed"));
    }

    /// Manual offline takes a provider out of rotation; requests bypass it.
    #[tokio::test]
    async fn test_integration_manual_offline_bypasses_provider() {
        use std::sync::atomic::Ordering;

        let ok_response =
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 32\r\n\r\n{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"}}]}"
                .to_string();

        // Two independent upstreams
        let p1_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p1_addr = p1_listener.local_addr().unwrap();
        let ok_response_p1 = ok_response.clone();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = p1_listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                // Should never be hit — but respond just in case
                let _ = stream.write_all(ok_response_p1.as_bytes()).await;
            }
        });

        let p2_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p2_addr = p2_listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = p2_listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(ok_response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        let p1 = ProviderState {
            name: "p1".into(),
            base_url: Url::parse(&format!("http://{}/v1/", p1_addr)).unwrap(),
            api_key: "key".into(),
            models: vec!["model".into()],
            headers: std::collections::HashMap::new(),
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            manual_disabled: AtomicBool::new(true), // MANUALLY DISABLED
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };

        let p2 = ProviderState {
            name: "p2".into(),
            base_url: Url::parse(&format!("http://{}/v1/", p2_addr)).unwrap(),
            api_key: "key".into(),
            models: vec!["model".into()],
            headers: std::collections::HashMap::new(),
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            manual_disabled: AtomicBool::new(false),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };

        let mut virtual_models = std::collections::HashMap::new();
        virtual_models.insert(
            "test".into(),
            vec![
                VirtualModelTarget {
                    provider: "p1".into(),
                    model: "model".into(),
                },
                VirtualModelTarget {
                    provider: "p2".into(),
                    model: "model".into(),
                },
            ],
        );

        let app_state = Arc::new(AppState {
            providers: vec![p1, p2],
            virtual_models,
            http_client: reqwest::Client::builder().build().unwrap(),
            upstream_timeout_secs: 5,
            auth: Default::default(),
        });

        let (_ws, wr) = tokio::sync::watch::channel(app_state.clone());
        let (ttx, _trx) = tokio::sync::mpsc::channel(1024);
        let reloader = Reloader::test_new(_ws);
        let state = ReloadableState::test_new(wr, TelemetryClient::new(ttx), reloader);
        let router = axum::Router::new()
            .route(
                "/v1/chat/completions",
                axum::routing::post(routes::create_chat_completions),
            )
            .layer(middleware::from_fn(add_request_id))
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();
        let res = client
            .post(format!("http://{}/v1/chat/completions", proxy_addr))
            .json(&serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // p1 was never hit; p2 handled the request
        assert_eq!(app_state.providers[0].requests.load(Ordering::Relaxed), 0);
        assert_eq!(app_state.providers[1].requests.load(Ordering::Relaxed), 1);
    }

    /// Admin reset endpoint clears circuit breaker, failures, and rate limit.
    #[tokio::test]
    async fn test_integration_admin_reset() {
        use std::sync::atomic::Ordering;

        let ok_response =
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 32\r\n\r\n{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"}}]}"
                .to_string();
        let ok_response_clone = ok_response.clone();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(ok_response_clone.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        let p = ProviderState {
            name: "resetme".into(),
            base_url: Url::parse(&format!("http://{}/v1/", addr)).unwrap(),
            api_key: "key".into(),
            models: vec!["model".into()],
            headers: std::collections::HashMap::new(),
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            manual_disabled: AtomicBool::new(false),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };

        let mut virtual_models = std::collections::HashMap::new();
        virtual_models.insert(
            "test".into(),
            vec![VirtualModelTarget {
                provider: "resetme".into(),
                model: "model".into(),
            }],
        );

        let app_state = Arc::new(AppState {
            providers: vec![p],
            virtual_models,
            http_client: reqwest::Client::builder().build().unwrap(),
            upstream_timeout_secs: 5,
            auth: Default::default(),
        });

        let (_ws, wr) = tokio::sync::watch::channel(app_state.clone());
        let (ttx, _trx) = tokio::sync::mpsc::channel(1024);
        let reloader = Reloader::test_new(_ws);
        let state = ReloadableState::test_new(wr, TelemetryClient::new(ttx), reloader);

        // Simulate a tripped state
        *app_state.providers[0].circuit.write().await = CircuitState::Open {
            until: Instant::now() + Duration::from_secs(300),
        };
        *app_state.providers[0].consecutive_failures.write().await = 5;
        *app_state.providers[0].rate_limited_until.write().await =
            Some(Instant::now() + Duration::from_secs(300));
        app_state.providers[0]
            .manual_disabled
            .store(true, Ordering::Release);

        // Build router with admin routes
        let router = axum::Router::new()
            .route(
                "/admin/providers/{name}/reset",
                axum::routing::post(routes::admin_reset),
            )
            .with_state(state.clone());
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(proxy_listener, router).await.unwrap();
        });

        // Call admin reset
        let client = reqwest::Client::new();
        let res = client
            .post(format!(
                "http://{}/admin/providers/resetme/reset",
                proxy_addr
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // Verify all state is cleared
        assert_eq!(
            *app_state.providers[0].circuit.read().await,
            CircuitState::Closed
        );
        assert_eq!(*app_state.providers[0].consecutive_failures.read().await, 0);
        assert!(app_state.providers[0]
            .rate_limited_until
            .read()
            .await
            .is_none());
        assert!(!app_state.providers[0]
            .manual_disabled
            .load(Ordering::Acquire));
    }

    /// /v1/embeddings endpoint proxies correctly and returns embeddings.
    #[tokio::test]
    async fn test_integration_embeddings_success() {
        let body = r#"{"data":[{"embedding":[0.1,0.2,0.3],"index":0}],"model":"test","usage":{"prompt_tokens":4}}"#;
        let ok_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(ok_response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        let p = ProviderState {
            name: "emb".into(),
            base_url: Url::parse(&format!("http://{}/v1/", addr)).unwrap(),
            api_key: "key".into(),
            models: vec!["model".into()],
            headers: std::collections::HashMap::new(),
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            manual_disabled: AtomicBool::new(false),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };

        let mut virtual_models = std::collections::HashMap::new();
        virtual_models.insert(
            "test-emb".into(),
            vec![VirtualModelTarget {
                provider: "emb".into(),
                model: "model".into(),
            }],
        );

        let app_state = Arc::new(AppState {
            providers: vec![p],
            virtual_models,
            http_client: reqwest::Client::builder().build().unwrap(),
            upstream_timeout_secs: 5,
            auth: Default::default(),
        });

        let (_ws, wr) = tokio::sync::watch::channel(app_state.clone());
        let (ttx, _trx) = tokio::sync::mpsc::channel(1024);
        let reloader = Reloader::test_new(_ws);
        let state = ReloadableState::test_new(wr, TelemetryClient::new(ttx), reloader);
        let router = axum::Router::new()
            .route(
                "/v1/chat/completions",
                axum::routing::post(routes::create_chat_completions),
            )
            .route(
                "/v1/embeddings",
                axum::routing::post(routes::create_embeddings),
            )
            .layer(middleware::from_fn(add_request_id))
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();
        let res = client
            .post(format!("http://{}/v1/embeddings", proxy_addr))
            .json(&serde_json::json!({
                "model": "test-emb",
                "input": "hello"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["data"][0]["embedding"].as_array().unwrap().len(), 3);
    }

    /// Invalid model name returns 400 Bad Request.
    #[tokio::test]
    async fn test_integration_invalid_model_returns_400() {
        let ok_response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 32\r\n\r\n{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"}}]}".to_string();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(ok_response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        let p = ProviderState {
            name: "prov".into(),
            base_url: Url::parse(&format!("http://{}/v1/", addr)).unwrap(),
            api_key: "key".into(),
            models: vec!["real-model".into()],
            headers: std::collections::HashMap::new(),
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            manual_disabled: AtomicBool::new(false),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };
        let mut vm = std::collections::HashMap::new();
        vm.insert(
            "known-model".into(),
            vec![VirtualModelTarget {
                provider: "prov".into(),
                model: "real-model".into(),
            }],
        );

        let state = Arc::new(AppState {
            providers: vec![p],
            virtual_models: vm,
            http_client: reqwest::Client::builder().build().unwrap(),
            upstream_timeout_secs: 5,
            auth: Default::default(),
        });
        let (_ws, wr) = tokio::sync::watch::channel(state.clone());
        let (ttx, _trx) = tokio::sync::mpsc::channel(1024);
        let reloader = Reloader::test_new(_ws);
        let rs = ReloadableState::test_new(wr, TelemetryClient::new(ttx), reloader);
        let router = axum::Router::new()
            .route(
                "/v1/chat/completions",
                axum::routing::post(routes::create_chat_completions),
            )
            .layer(middleware::from_fn(add_request_id))
            .with_state(rs);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();
        let res = client.post(format!("http://{}/v1/chat/completions", addr))
            .json(&serde_json::json!({"model": "nonexistent", "messages": [{"role":"user","content":"hi"}]}))
            .send().await.unwrap();
        assert_eq!(res.status(), 400);
    }

    /// Admin online re-enables a manually disabled provider.
    #[tokio::test]
    async fn test_integration_admin_online() {
        use std::sync::atomic::Ordering;

        let ok_response =
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 32\r\n\r\n{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"}}]}"
                .to_string();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(ok_response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        let p = ProviderState {
            name: "target".into(),
            base_url: Url::parse(&format!("http://{}/v1/", addr)).unwrap(),
            api_key: "key".into(),
            models: vec!["model".into()],
            headers: std::collections::HashMap::new(),
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            manual_disabled: AtomicBool::new(true), // Start disabled
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };

        let mut vms = std::collections::HashMap::new();
        vms.insert(
            "test".into(),
            vec![VirtualModelTarget {
                provider: "target".into(),
                model: "model".into(),
            }],
        );
        let state = Arc::new(AppState {
            providers: vec![p],
            virtual_models: vms,
            http_client: reqwest::Client::builder().build().unwrap(),
            upstream_timeout_secs: 5,
            auth: Default::default(),
        });
        let (_ws, wr) = tokio::sync::watch::channel(state.clone());
        let (ttx, _trx) = tokio::sync::mpsc::channel(1024);
        let reloader = Reloader::test_new(_ws.clone());
        let rs = ReloadableState::test_new(wr, TelemetryClient::new(ttx), reloader);
        let router = axum::Router::new()
            .route(
                "/admin/providers/{name}/online",
                axum::routing::post(routes::admin_online),
            )
            .with_state(rs);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        // Verify provider is disabled
        assert!(state.providers[0].manual_disabled.load(Ordering::Acquire));

        // Call admin online
        let client = reqwest::Client::new();
        let res = client
            .post(format!(
                "http://{}/admin/providers/target/online",
                proxy_addr
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // Verify provider is now enabled
        assert!(!state.providers[0].manual_disabled.load(Ordering::Acquire));
    }

    /// Token counts are parsed from upstream JSON and recorded in provider counters.
    #[tokio::test]
    async fn test_integration_token_count_parsed() {
        // Upstream returns a response with usage data
        let body = r#"{"id":"test","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":"Hello"},"finish_reason":"stop"}],"usage":{"prompt_tokens":42,"completion_tokens":7,"total_tokens":49}}"#;
        let ok_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(ok_response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        let p = ProviderState {
            name: "counter".into(),
            base_url: Url::parse(&format!("http://{}/v1/", addr)).unwrap(),
            api_key: "key".into(),
            models: vec!["model".into()],
            headers: std::collections::HashMap::new(),
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            manual_disabled: AtomicBool::new(false),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };

        let mut vms = std::collections::HashMap::new();
        vms.insert(
            "test".into(),
            vec![VirtualModelTarget {
                provider: "counter".into(),
                model: "model".into(),
            }],
        );
        let state = Arc::new(AppState {
            providers: vec![p],
            virtual_models: vms,
            http_client: reqwest::Client::builder().build().unwrap(),
            upstream_timeout_secs: 5,
            auth: Default::default(),
        });
        let (_ws, wr) = tokio::sync::watch::channel(state.clone());
        let (ttx, _trx) = tokio::sync::mpsc::channel(1024);
        let reloader = Reloader::test_new(_ws);
        let rs = ReloadableState::test_new(wr, TelemetryClient::new(ttx), reloader);
        let router = axum::Router::new()
            .route(
                "/v1/chat/completions",
                axum::routing::post(routes::create_chat_completions),
            )
            .layer(middleware::from_fn(add_request_id))
            .with_state(rs);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();
        let res = client
            .post(format!("http://{}/v1/chat/completions", proxy_addr))
            .json(
                &serde_json::json!({"model": "test", "messages": [{"role":"user","content":"hi"}]}),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // Verify token counts were parsed
        let tokens_in = state.providers[0]
            .tokens_input
            .load(std::sync::atomic::Ordering::Relaxed);
        let tokens_out = state.providers[0]
            .tokens_output
            .load(std::sync::atomic::Ordering::Relaxed);
        assert!(
            tokens_in > 0,
            "Expected input tokens > 0, got {}",
            tokens_in
        );
        assert!(
            tokens_out > 0,
            "Expected output tokens > 0, got {}",
            tokens_out
        );
        assert_eq!(tokens_in, 42);
        assert_eq!(tokens_out, 7);
    }

    // -----------------------------------------------------------------------
    // Test helpers for JSON error format, CORS, and x-request-id tests
    // -----------------------------------------------------------------------

    /// Builds a minimal test AppState + ReloadableState with one provider
    /// pointing at the given upstream address.
    fn build_test_state(upstream_addr: SocketAddr) -> (Arc<AppState>, ReloadableState) {
        let p = ProviderState {
            name: "prov".into(),
            base_url: Url::parse(&format!("http://{}/v1/", upstream_addr)).unwrap(),
            api_key: "key".into(),
            models: vec!["model".into()],
            headers: std::collections::HashMap::new(),
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            manual_disabled: AtomicBool::new(false),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };
        let mut vm = std::collections::HashMap::new();
        vm.insert(
            "test".into(),
            vec![VirtualModelTarget {
                provider: "prov".into(),
                model: "model".into(),
            }],
        );

        let app_state = Arc::new(AppState {
            providers: vec![p],
            virtual_models: vm,
            http_client: reqwest::Client::builder().build().unwrap(),
            upstream_timeout_secs: 5,
            auth: Default::default(),
        });
        let (_ws, wr) = tokio::sync::watch::channel(app_state.clone());
        let (ttx, _trx) = tokio::sync::mpsc::channel(1024);
        let reloader = Reloader::test_new(_ws);
        let rs = ReloadableState::test_new(wr, TelemetryClient::new(ttx), reloader);
        (app_state, rs)
    }

    /// Builds a test router with all middleware layers (request-id, CORS).
    fn build_test_router(state: ReloadableState) -> axum::Router {
        axum::Router::new()
            .route(
                "/v1/chat/completions",
                axum::routing::post(routes::create_chat_completions),
            )
            .route(
                "/v1/embeddings",
                axum::routing::post(routes::create_embeddings),
            )
            .route("/v1/models", axum::routing::get(routes::list_models))
            .layer(middleware::from_fn(add_request_id))
            .layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods([
                        axum::http::Method::GET,
                        axum::http::Method::POST,
                        axum::http::Method::OPTIONS,
                    ])
                    .allow_headers(Any),
            )
            .with_state(state)
    }

    // -----------------------------------------------------------------------
    // Integration tests: JSON error format
    // -----------------------------------------------------------------------

    /// Invalid JSON body returns 400 with structured JSON error.
    #[tokio::test]
    async fn test_integration_invalid_json() {
        let ok_response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 32\r\n\r\n{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"}}]}".to_string();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(ok_response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        let (_app_state, rs) = build_test_state(addr);
        let router = build_test_router(rs);
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(proxy_listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();
        let res = client
            .post(format!("http://{}/v1/chat/completions", proxy_addr))
            .body("not valid json {{{")
            .header("Content-Type", "application/json")
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 400);
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "application/json"
        );

        let body: Value = res.json().await.unwrap();
        let error = body.get("error").expect("response should have 'error' key");
        assert!(error.get("message").is_some());
        assert_eq!(
            error.get("type").unwrap().as_str().unwrap(),
            "invalid_request_error"
        );
        assert_eq!(error.get("code").unwrap().as_u64().unwrap(), 400);
    }

    /// Missing 'model' field returns 400 with structured JSON error.
    #[tokio::test]
    async fn test_integration_missing_model() {
        let ok_response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 32\r\n\r\n{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"}}]}".to_string();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(ok_response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        let (_app_state, rs) = build_test_state(addr);
        let router = build_test_router(rs);
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(proxy_listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();
        let res = client
            .post(format!("http://{}/v1/chat/completions", proxy_addr))
            .json(&serde_json::json!({
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 400);
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "application/json"
        );

        let body: Value = res.json().await.unwrap();
        let error = body.get("error").expect("response should have 'error' key");
        assert!(error.get("message").is_some());
        assert_eq!(
            error.get("type").unwrap().as_str().unwrap(),
            "invalid_request_error"
        );
        assert_eq!(error.get("code").unwrap().as_u64().unwrap(), 400);
    }

    /// Unknown virtual model returns 400 with structured JSON error.
    #[tokio::test]
    async fn test_integration_unknown_model() {
        let ok_response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 32\r\n\r\n{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"}}]}".to_string();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(ok_response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        let (_app_state, rs) = build_test_state(addr);
        let router = build_test_router(rs);
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(proxy_listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();
        let res = client
            .post(format!("http://{}/v1/chat/completions", proxy_addr))
            .json(&serde_json::json!({
                "model": "nonexistent-model",
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 400);
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "application/json"
        );

        let body: Value = res.json().await.unwrap();
        let error = body.get("error").expect("response should have 'error' key");
        let message = error.get("message").unwrap().as_str().unwrap();
        assert!(message.contains("nonexistent-model"));
        assert_eq!(
            error.get("type").unwrap().as_str().unwrap(),
            "invalid_request_error"
        );
        assert_eq!(error.get("code").unwrap().as_u64().unwrap(), 400);
    }

    // -----------------------------------------------------------------------
    // Integration tests: CORS headers
    // -----------------------------------------------------------------------

    /// OPTIONS preflight to /v1/chat/completions returns CORS headers.
    #[tokio::test]
    async fn test_integration_cors_options() {
        let ok_response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 32\r\n\r\n{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"}}]}".to_string();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(ok_response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        let (_app_state, rs) = build_test_state(addr);
        let router = build_test_router(rs);
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(proxy_listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();
        let res = client
            .request(
                reqwest::Method::OPTIONS,
                format!("http://{}/v1/chat/completions", proxy_addr),
            )
            .header("Origin", "http://localhost:3000")
            .header("Access-Control-Request-Method", "POST")
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 200);
        assert_eq!(
            res.headers().get("access-control-allow-origin").unwrap(),
            "*"
        );
        assert!(res.headers().contains_key("access-control-allow-methods"));
    }

    /// POST to /v1/chat/completions includes CORS origin header in response.
    #[tokio::test]
    async fn test_integration_cors_post() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"Hello from prov"}}]}"#;
        let ok_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(ok_response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        let (_app_state, rs) = build_test_state(addr);
        let router = build_test_router(rs);
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(proxy_listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();
        let res = client
            .post(format!("http://{}/v1/chat/completions", proxy_addr))
            .header("Origin", "http://localhost:3000")
            .json(&serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 200);
        assert_eq!(
            res.headers().get("access-control-allow-origin").unwrap(),
            "*"
        );
    }

    // -----------------------------------------------------------------------
    // Integration tests: x-request-id header
    // -----------------------------------------------------------------------

    /// Successful response includes x-request-id matching oxllm-[0-9a-f]{16}.
    #[tokio::test]
    async fn test_integration_request_id_on_success() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"Hello from prov"}}]}"#;
        let ok_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(ok_response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        let (_app_state, rs) = build_test_state(addr);
        let router = build_test_router(rs);
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(proxy_listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();
        let res = client
            .post(format!("http://{}/v1/chat/completions", proxy_addr))
            .json(&serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 200);
        let request_id = res
            .headers()
            .get("x-request-id")
            .expect("x-request-id header should be present");
        let id_str = request_id.to_str().unwrap();
        assert!(
            id_str.starts_with("wirellm-"),
            "x-request-id should start with 'wirellm-', got: {}",
            id_str
        );
        assert_eq!(
            id_str.len(),
            24,
            "x-request-id should be 'wirellm-' + 16 hex chars (len 24), got len {}",
            id_str.len()
        );
        // Verify remaining chars are valid hex
        let hex_part = &id_str[8..];
        assert!(
            hex_part.chars().all(|c| c.is_ascii_hexdigit()),
            "x-request-id hex part should be all hex chars, got: {}",
            hex_part
        );
    }

    /// Error response also includes x-request-id header.
    #[tokio::test]
    async fn test_integration_request_id_on_error() {
        let ok_response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 32\r\n\r\n{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"}}]}".to_string();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(ok_response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        let (_app_state, rs) = build_test_state(addr);
        let router = build_test_router(rs);
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(proxy_listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();
        let res = client
            .post(format!("http://{}/v1/chat/completions", proxy_addr))
            .body("not valid json")
            .header("Content-Type", "application/json")
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 400);
        let request_id = res
            .headers()
            .get("x-request-id")
            .expect("x-request-id header should be present on error responses");
        let id_str = request_id.to_str().unwrap();
        assert!(
            id_str.starts_with("wirellm-"),
            "x-request-id should start with 'wirellm-', got: {}",
            id_str
        );
        assert_eq!(id_str.len(), 24);
    }

    #[tokio::test]
    async fn test_integration_provider_custom_headers_and_client_passthrough() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"ok"}}]}"#;
        let ok_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let req_str = String::from_utf8_lossy(&buf[..n]).to_string();
                let _ = tx.send(req_str).await;
                let _ = stream.write_all(ok_response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        let mut provider_headers = std::collections::HashMap::new();
        provider_headers.insert("User-Agent".to_string(), "opencode/1.19.0".to_string());
        provider_headers.insert("x-provider-header".to_string(), "provider-val".to_string());
        provider_headers.insert(
            "x-overridden-header".to_string(),
            "provider-wins".to_string(),
        );

        let p = ProviderState {
            name: "custom-prov".into(),
            base_url: Url::parse(&format!("http://{}/v1/", addr)).unwrap(),
            api_key: "secret-key".into(),
            models: vec!["test-model".into()],
            headers: provider_headers,
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            manual_disabled: AtomicBool::new(false),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };

        let mut vms = std::collections::HashMap::new();
        vms.insert(
            "custom-chat".into(),
            vec![VirtualModelTarget {
                provider: "custom-prov".into(),
                model: "test-model".into(),
            }],
        );

        let app_state = Arc::new(AppState {
            providers: vec![p],
            virtual_models: vms,
            http_client: reqwest::Client::builder().build().unwrap(),
            upstream_timeout_secs: 5,
            auth: Default::default(),
        });

        let (_ws, wr) = tokio::sync::watch::channel(app_state.clone());
        let (ttx, _trx) = tokio::sync::mpsc::channel(1024);
        let reloader = Reloader::test_new(_ws);
        let rs = ReloadableState::test_new(wr, TelemetryClient::new(ttx), reloader);

        let router = axum::Router::new()
            .route(
                "/v1/chat/completions",
                axum::routing::post(routes::create_chat_completions),
            )
            .layer(middleware::from_fn(add_request_id))
            .with_state(rs);

        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(proxy_listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();
        let res = client
            .post(format!("http://{}/v1/chat/completions", proxy_addr))
            .json(&serde_json::json!({
                "model": "custom-chat",
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .header("x-client-session", "sess-12345")
            .header("x-overridden-header", "client-attempt")
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 200);

        let captured_request = rx.recv().await.expect("Must capture upstream request");
        let lower = captured_request.to_ascii_lowercase();

        // 1. Upstream received auth
        assert!(lower.contains("authorization: bearer secret-key"));

        // 2. Upstream received custom User-Agent from provider config
        assert!(lower.contains("user-agent: opencode/1.19.0"));

        // 3. Upstream received provider header
        assert!(lower.contains("x-provider-header: provider-val"));

        // 4. Upstream received client passthrough header
        assert!(lower.contains("x-client-session: sess-12345"));

        // 5. Provider config takes precedence over client for same header name
        assert!(lower.contains("x-overridden-header: provider-wins"));
        assert!(!lower.contains("x-overridden-header: client-attempt"));
    }

    #[tokio::test]
    async fn test_integration_provider_custom_authorization_override() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"ok"}}]}"#;
        let ok_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let req_str = String::from_utf8_lossy(&buf[..n]).to_string();
                let _ = tx.send(req_str).await;
                let _ = stream.write_all(ok_response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        let mut provider_headers = std::collections::HashMap::new();
        provider_headers.insert(
            "Authorization".to_string(),
            "CustomAuth custom-token-val".to_string(),
        );

        let p = ProviderState {
            name: "custom-auth-prov".into(),
            base_url: Url::parse(&format!("http://{}/v1/", addr)).unwrap(),
            api_key: "default-key".into(),
            models: vec!["test-model".into()],
            headers: provider_headers,
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            manual_disabled: AtomicBool::new(false),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };

        let mut vms = std::collections::HashMap::new();
        vms.insert(
            "auth-chat".into(),
            vec![VirtualModelTarget {
                provider: "custom-auth-prov".into(),
                model: "test-model".into(),
            }],
        );

        let app_state = Arc::new(AppState {
            providers: vec![p],
            virtual_models: vms,
            http_client: reqwest::Client::builder().build().unwrap(),
            upstream_timeout_secs: 5,
            auth: Default::default(),
        });

        let (_ws, wr) = tokio::sync::watch::channel(app_state.clone());
        let (ttx, _trx) = tokio::sync::mpsc::channel(1024);
        let reloader = Reloader::test_new(_ws);
        let rs = ReloadableState::test_new(wr, TelemetryClient::new(ttx), reloader);

        let router = axum::Router::new()
            .route(
                "/v1/chat/completions",
                axum::routing::post(routes::create_chat_completions),
            )
            .layer(middleware::from_fn(add_request_id))
            .with_state(rs);

        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(proxy_listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();
        let res = client
            .post(format!("http://{}/v1/chat/completions", proxy_addr))
            .json(&serde_json::json!({
                "model": "auth-chat",
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 200);

        let captured_request = rx.recv().await.expect("Must capture upstream request");
        let lower = captured_request.to_ascii_lowercase();

        // Upstream received overridden Authorization header
        assert!(lower.contains("authorization: customauth custom-token-val"));
        assert!(!lower.contains("authorization: bearer default-key"));
    }

    fn build_full_test_router(state: ReloadableState) -> axum::Router {
        use axum::routing::{delete, get, post, put};
        axum::Router::new()
            .route("/v1/models", get(routes::list_models))
            .route("/v1/embeddings", post(routes::create_embeddings))
            .route("/v1/chat/completions", post(routes::create_chat_completions))
            .route("/api/status", get(admin::get_api_status))
            .route("/api/providers", get(admin::list_providers).post(admin::create_provider))
            .route("/api/providers/{name}", put(admin::update_provider).delete(admin::delete_provider))
            .route("/api/providers/{name}/test", post(admin::test_provider))
            .route("/api/virtual-models", get(admin::list_virtual_models))
            .route("/api/virtual-models/{name}", put(admin::upsert_virtual_model).delete(admin::delete_virtual_model))
            .route("/api/analytics/daily", get(admin::get_daily_analytics))
            .route("/api/vault/secrets", get(admin::list_vault_secrets).post(admin::create_vault_secret))
            .route("/api/vault/secrets/{name}", delete(admin::delete_vault_secret))
            .route("/api/logs", get(routes::get_logs))
            .route("/mcp/sse", get(mcp::mcp_sse_handler))
            .route("/mcp/messages", post(mcp::mcp_messages_handler))
            .route("/status", get(routes::get_status))
            .route("/logs", get(routes::get_logs))
            .route("/", get(static_handler))
            .route("/ui", get(static_handler))
            .route("/dashboard", get(static_handler))
            .route("/overview", get(static_handler))
            .layer(middleware::from_fn(add_request_id))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_integration_api_status() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (_app_state, rs) = build_test_state(addr);

        let router = build_full_test_router(rs);
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(proxy_listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();
        let res = client
            .get(format!("http://{}/api/status", proxy_addr))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let json: Value = res.json().await.unwrap();
        assert!(json.get("uptime_secs").is_some());
        assert!(json.get("providers").is_some());
        assert!(json.get("virtual_models").is_some());
    }

    #[tokio::test]
    async fn test_integration_api_providers_crud() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (_app_state, rs) = build_test_state(addr);

        let router = build_full_test_router(rs);
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(proxy_listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();

        // 1. Create a provider
        let create_body = serde_json::json!({
            "name": "nvidia_test",
            "enabled": true,
            "base_url": "https://integrate.api.nvidia.com/v1",
            "api_key_template": "${NVIDIA_API_KEY}",
            "models": ["deepseek-ai/deepseek-v4.1-flash", "z-ai/glm-5.3-flash"],
            "rpm": 40,
            "min_interval_ms": 1500,
            "max_concurrency": 2,
            "smartness_score": 92
        });

        let res = client
            .post(format!("http://{}/api/providers", proxy_addr))
            .json(&create_body)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);

        // 2. List providers
        let res = client
            .get(format!("http://{}/api/providers", proxy_addr))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let list: Vec<Value> = res.json().await.unwrap();
        assert!(list.iter().any(|p| p["name"] == "nvidia_test"));

        // 3. Update provider
        let update_body = serde_json::json!({
            "name": "nvidia_test",
            "enabled": false,
            "base_url": "https://integrate.api.nvidia.com/v1",
            "api_key_template": "${NVIDIA_API_KEY}",
            "models": ["deepseek-ai/deepseek-v4.1-flash"],
            "rpm": 30,
            "min_interval_ms": 2000,
            "max_concurrency": 1,
            "smartness_score": 95
        });

        let res = client
            .put(format!("http://{}/api/providers/nvidia_test", proxy_addr))
            .json(&update_body)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // 4. Delete provider
        let res = client
            .delete(format!("http://{}/api/providers/nvidia_test", proxy_addr))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    }

    #[tokio::test]
    async fn test_integration_mcp_endpoints() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (_app_state, rs) = build_test_state(addr);

        let router = build_full_test_router(rs);
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(proxy_listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();

        // 1. Initialize MCP
        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });
        let res = client
            .post(format!("http://{}/mcp/messages", proxy_addr))
            .json(&init_req)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let init_json: Value = res.json().await.unwrap();
        assert_eq!(init_json["result"]["serverInfo"]["name"], "wirellm");

        // 2. List tools
        let tools_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });
        let res = client
            .post(format!("http://{}/mcp/messages", proxy_addr))
            .json(&tools_req)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let tools_json: Value = res.json().await.unwrap();
        let tools = tools_json["result"]["tools"].as_array().unwrap();
        assert!(tools.iter().any(|t| t["name"] == "wirellm_get_status"));
        assert!(tools.iter().any(|t| t["name"] == "wirellm_add_provider"));

        // 3. Call tool: wirellm_get_status
        let call_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "wirellm_get_status",
                "arguments": {}
            }
        });
        let res = client
            .post(format!("http://{}/mcp/messages", proxy_addr))
            .json(&call_req)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let call_json: Value = res.json().await.unwrap();
        assert!(call_json["result"]["content"][0]["text"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_integration_static_embedded_ui() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (_app_state, rs) = build_test_state(addr);

        let router = build_full_test_router(rs);
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(proxy_listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();

        // Check root index.html
        let res = client
            .get(format!("http://{}/", proxy_addr))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let content_type = res.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(content_type.contains("text/html"));
        let body = res.text().await.unwrap();
        assert!(body.contains("WireLLM"));

        // Check /dashboard route (SPA fallback)
        let res = client
            .get(format!("http://{}/dashboard", proxy_addr))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    }

    #[tokio::test]
    async fn test_integration_direct_model_passthrough() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"direct model output"}}]}"#;
        let ok_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 2048];
            let n = socket.read(&mut buf).await.unwrap();
            let req_str = String::from_utf8_lossy(&buf[..n]);
            assert!(req_str.contains("deepseek-ai/deepseek-v4.1-flash"));
            socket.write_all(ok_response.as_bytes()).await.unwrap();
        });

        let p = ProviderState {
            name: "prov".into(),
            base_url: Url::parse(&format!("http://{}/v1/", addr)).unwrap(),
            api_key: "key".into(),
            models: vec!["deepseek-ai/deepseek-v4.1-flash".into()],
            headers: std::collections::HashMap::new(),
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            manual_disabled: AtomicBool::new(false),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };

        let app_state = Arc::new(AppState {
            providers: vec![p],
            virtual_models: std::collections::HashMap::new(), // No virtual models!
            http_client: reqwest::Client::builder().build().unwrap(),
            upstream_timeout_secs: 5,
            auth: Default::default(),
        });

        let (ws, wr) = tokio::sync::watch::channel(app_state.clone());
        let (ttx, _trx) = tokio::sync::mpsc::channel(1024);
        let reloader = Reloader::test_new(ws);
        let rs = ReloadableState::test_new(wr, TelemetryClient::new(ttx), reloader);

        let router = build_full_test_router(rs);
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(proxy_listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();

        // 1. Verify GET /v1/models exposes the direct model
        let models_res = client
            .get(format!("http://{}/v1/models", proxy_addr))
            .send()
            .await
            .unwrap();
        assert_eq!(models_res.status(), 200);
        let models_json: Value = models_res.json().await.unwrap();
        let model_ids: Vec<&str> = models_json["data"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|m| m["id"].as_str())
            .collect();
        assert!(model_ids.contains(&"deepseek-ai/deepseek-v4.1-flash"));

        // 2. Verify direct passthrough via short alias "deepseek-v4.1-flash"
        let res = client
            .post(format!("http://{}/v1/chat/completions", proxy_addr))
            .json(&serde_json::json!({
                "model": "deepseek-v4.1-flash",
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let res_json: Value = res.json().await.unwrap();
        assert_eq!(
            res_json["choices"][0]["message"]["content"].as_str().unwrap(),
            "direct model output"
        );
    }

    /// Integration test verifying central Vault secret storage, AES-256-GCM encryption,
    /// cross-provider template reuse (${SECRET_NAME}), alias resolution, and MCP tools.
    #[tokio::test]
    async fn test_central_vault_secret_reuse_and_mcp() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (_app_state, rs) = build_test_state(addr);

        let router = build_full_test_router(rs.clone());
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(proxy_listener, router).await.unwrap();
        });

        let client = reqwest::Client::new();

        // 1. Add secret to vault once via MCP
        let set_res = client
            .post(format!("http://{}/mcp/messages", proxy_addr))
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "wirellm_set_secret",
                    "arguments": {
                        "name": "NVIDIA_API_KEY",
                        "value": "nvapi-secret-12345-very-long",
                        "description": "Primary NVIDIA key"
                    }
                }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(set_res.status(), 200);

        // 2. Verify masked preview via MCP, plaintext is never exposed
        let list_res = client
            .post(format!("http://{}/mcp/messages", proxy_addr))
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "wirellm_list_secrets",
                    "arguments": {}
                }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(list_res.status(), 200);
        let list_json: Value = list_res.json().await.unwrap();
        let text_content = list_json["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text_content.contains("NVIDIA_API_KEY"));
        assert!(text_content.contains("••••"));
        assert!(!text_content.contains("12345"));

        // 3. Verify template resolution with exact name and alias
        let resolved = rs.db.resolve_secret_template("${NVIDIA_API_KEY}").unwrap();
        assert_eq!(resolved, "nvapi-secret-12345-very-long");

        let resolved_alias = rs.db.resolve_secret_template("${NVIDIA}").unwrap();
        assert_eq!(resolved_alias, "nvapi-secret-12345-very-long");

        // 4. Configure two providers reusing the single vault secret
        let empty_headers = std::collections::HashMap::new();
        rs.db.upsert_provider(
            "nvidia-nemotron",
            true,
            "https://integrate.api.nvidia.com/v1/",
            "${NVIDIA_API_KEY}",
            &["nvidia/nemotron-3-ultra-550b-a55b".to_string()],
            &empty_headers,
            Some(40),
            Some(1200),
            Some(3),
            Some(95),
            None,
        ).unwrap();

        rs.db.upsert_provider(
            "nvidia-llama",
            true,
            "https://integrate.api.nvidia.com/v1/",
            "${NVIDIA}",
            &["meta/llama-3.3-70b-instruct".to_string()],
            &empty_headers,
            Some(40),
            Some(1200),
            Some(3),
            Some(95),
            None,
        ).unwrap();

        // 5. Load config and verify BOTH providers successfully decrypted the same secret
        let cfg = rs.db.load_config().unwrap();
        let p1 = cfg.providers.iter().find(|p| p.name == "nvidia-nemotron").unwrap();
        let p2 = cfg.providers.iter().find(|p| p.name == "nvidia-llama").unwrap();
        assert!(p1.enabled);
        assert_eq!(p1.api_key, "nvapi-secret-12345-very-long");
        assert!(p2.enabled);
        assert_eq!(p2.api_key, "nvapi-secret-12345-very-long");

        // 6. Test Admin REST API endpoints: GET /api/vault/secrets and DELETE
        let api_secrets: Vec<wirellm_core::db::VaultSecretRecord> = client
            .get(format!("http://{}/api/vault/secrets", proxy_addr))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(api_secrets.len(), 1);
        assert_eq!(api_secrets[0].name, "NVIDIA_API_KEY");
        assert!(api_secrets[0].key_preview.contains("••••"));

        let del_res = client
            .delete(format!("http://{}/api/vault/secrets/NVIDIA_API_KEY", proxy_addr))
            .send()
            .await
            .unwrap();
        assert_eq!(del_res.status(), 200);

        let after_del = rs.db.list_vault_secrets().unwrap();
        assert_eq!(after_del.len(), 0);
    }
