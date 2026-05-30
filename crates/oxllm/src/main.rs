use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};

use oxllm_core::config::Config;
use oxllm_core::state::{AppState, CircuitState, ProviderState};
use oxllm_core::telemetry::{TelemetryClient, TelemetryWorker};
use reqwest::Url;

mod routes;

#[derive(Parser, Debug)]
#[command(
    name = "oxllm",
    version = "0.1.0",
    author = "Nigel Jones",
    about = "Minimalist adaptive routing LLM proxy"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Starts the gateway Axum server and telemetry worker
    Serve {
        /// Path to the configuration TOML file
        #[arg(short, long, default_value = "config.toml", env = "OXLLM_CONFIG")]
        config: PathBuf,
    },
    /// Parses and validates the configuration syntax and cross-references
    Validate {
        /// Path to the configuration TOML file
        #[arg(short, long, default_value = "config.toml", env = "OXLLM_CONFIG")]
        config: PathBuf,
    },
    /// Fetches and displays the active health and circuit status from localhost
    Status {
        /// Port of the running gateway server
        #[arg(short, long, default_value_t = 8080, env = "OXLLM_PORT")]
        port: u16,
    },
    /// Triggers hot-reload by sending SIGHUP to the running gateway daemon
    Reload {
        /// PID of the running oxllm process (optional, reads from /tmp/oxllm.pid by default)
        #[arg(short, long)]
        pid: Option<u32>,
    },
}

#[derive(Clone)]
pub struct ReloadableState {
    pub app_state: tokio::sync::watch::Receiver<Arc<AppState>>,
    pub telemetry: TelemetryClient,
    pub start_time: Instant,
}

impl axum::extract::FromRef<ReloadableState> for Arc<AppState> {
    fn from_ref(state: &ReloadableState) -> Self {
        state.app_state.borrow().clone()
    }
}

impl axum::extract::FromRef<ReloadableState> for (Arc<AppState>, TelemetryClient) {
    fn from_ref(state: &ReloadableState) -> Self {
        (state.app_state.borrow().clone(), state.telemetry.clone())
    }
}

impl axum::extract::FromRef<ReloadableState> for Instant {
    fn from_ref(state: &ReloadableState) -> Self {
        state.start_time
    }
}

impl axum::extract::FromRef<ReloadableState> for (Arc<AppState>, Instant) {
    fn from_ref(state: &ReloadableState) -> Self {
        (state.app_state.borrow().clone(), state.start_time)
    }
}

fn build_app_state(config: Config) -> Result<AppState, String> {
    let mut providers = Vec::new();
    for p in config.providers {
        if !p.enabled {
            continue;
        }
        let url = Url::parse(&p.base_url).map_err(|e| {
            format!(
                "Invalid base URL '{}' for provider '{}': {}",
                p.base_url, p.name, e
            )
        })?;

        providers.push(ProviderState {
            name: p.name,
            base_url: url,
            api_key: p.api_key,
            models: p.models,
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
        });
    }

    let http_client = reqwest::Client::builder()
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    Ok(AppState {
        providers,
        virtual_models: config.virtual_models,
        http_client,
        upstream_timeout_secs: config.server.upstream_timeout_secs,
    })
}

fn write_pid_file() -> std::io::Result<()> {
    let pid = std::process::id();
    std::fs::write("/tmp/oxllm.pid", pid.to_string())
}

fn send_sighup(pid: u32) -> std::io::Result<()> {
    let status = std::process::Command::new("kill")
        .args(["-HUP", &pid.to_string()])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "kill command failed with exit code: {:?}",
            status.code()
        )))
    }
}

async fn localhost_only(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if addr.ip().is_loopback() {
        Ok(next.run(req).await)
    } else {
        warn!(target: "oxllm::security", "Blocked external attempt to access administrative route from IP: {}", addr.ip());
        Err(StatusCode::FORBIDDEN)
    }
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received SIGINT, shutting down gracefully...");
        },
        _ = terminate => {
            info!("Received SIGTERM, shutting down gracefully...");
        },
    }
}

async fn handle_sighup(
    config_path: PathBuf,
    watch_sender: tokio::sync::watch::Sender<Arc<AppState>>,
) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sig = match signal(SignalKind::hangup()) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to register SIGHUP handler: {}", e);
                return;
            },
        };

        info!("Registered SIGHUP reload listener");
        while sig.recv().await.is_some() {
            info!("SIGHUP received, reloading configuration...");
            match Config::load_from_file(&config_path) {
                Ok(new_config) => {
                    if let Err(e) = new_config.validate() {
                        error!("Configuration validation failed during hot-reload: {}", e);
                        continue;
                    }
                    match build_app_state(new_config) {
                        Ok(new_state) => {
                            if let Err(e) = watch_sender.send(Arc::new(new_state)) {
                                error!("Failed to update watch channel: {}", e);
                            } else {
                                info!("Configuration successfully reloaded!");
                            }
                        },
                        Err(e) => {
                            error!("Failed to build new app state during hot-reload: {}", e);
                        },
                    }
                },
                Err(e) => {
                    error!("Failed to load config file during hot-reload: {}", e);
                },
            }
        }
    }
}

async fn run_serve(config_path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load initial config
    let config = Config::load_from_file(&config_path)?;
    config.validate()?;

    // 2. Build initial AppState
    let app_state = Arc::new(build_app_state(config.clone())?);

    // 3. Initialize watch channel
    let (watch_sender, watch_receiver) = tokio::sync::watch::channel(app_state.clone());

    // 4. Initialize telemetry channel & spawn TelemetryWorker
    let (telemetry_tx, telemetry_rx) = tokio::sync::mpsc::channel(1024);
    let telemetry_client = TelemetryClient::new(telemetry_tx);
    let otel_endpoint = config.server.otel_endpoint.clone();
    let _worker_handle = TelemetryWorker::spawn(&otel_endpoint, telemetry_rx)?;

    // 5. Spawn SIGHUP listener
    let config_path_clone = config_path.clone();
    tokio::spawn(handle_sighup(config_path_clone, watch_sender));

    // 6. Build Axum Router
    let start_time = Instant::now();
    let reloadable_state = ReloadableState {
        app_state: watch_receiver,
        telemetry: telemetry_client,
        start_time,
    };

    use axum::routing::{get, post};
    let app = axum::Router::new()
        .route("/v1/models", get(routes::list_models))
        .route("/v1/embeddings", post(routes::create_embeddings))
        .route(
            "/v1/chat/completions",
            post(routes::create_chat_completions),
        )
        .route(
            "/status",
            get(routes::get_status).layer(middleware::from_fn(localhost_only)),
        )
        .route(
            "/health",
            get(health_check).layer(middleware::from_fn(localhost_only)),
        )
        .with_state(reloadable_state);

    // Write PID file
    if let Err(e) = write_pid_file() {
        warn!("Failed to write PID file: {}", e);
    }

    // Start listening
    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    info!("Listening on http://{}", bind_addr);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

fn run_validate(config_path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load_from_file(&config_path)?;
    config.validate()?;
    println!("Configuration file at '{:?}' is VALID!", config_path);
    Ok(())
}

async fn run_status(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/status", port);
    let res = client.get(&url).send().await?;
    if !res.status().is_success() {
        return Err(format!("Server returned error status: {}", res.status()).into());
    }

    #[derive(serde::Deserialize)]
    struct ProviderStatus {
        name: String,
        circuit: String,
        failures: u32,
        rate_limited: bool,
        requests: u64,
        successes: u64,
        tokens_input: u64,
        tokens_output: u64,
    }

    #[derive(serde::Deserialize)]
    struct StatusResponse {
        uptime_secs: u64,
        total_requests: u64,
        providers: Vec<ProviderStatus>,
    }

    let status: StatusResponse = res.json().await?;
    println!(
        "Uptime: {}s | Total requests: {}",
        status.uptime_secs, status.total_requests
    );
    println!(
        "\n+--------------------+--------------------------------+----------+---------------+----------+-----------+--------------+---------------+"
    );
    println!("| Provider Name      | Circuit Breaker State          | Failures | Rate Limited? | Requests | Successes | Tokens Input | Tokens Output |");
    println!("+--------------------+--------------------------------+----------+---------------+----------+-----------+--------------+---------------+");
    for s in status.providers {
        println!(
            "| {:<18} | {:<30} | {:<8} | {:<13} | {:<8} | {:<9} | {:<12} | {:<13} |",
            s.name,
            s.circuit,
            s.failures,
            if s.rate_limited { "Yes" } else { "No" },
            s.requests,
            s.successes,
            s.tokens_input,
            s.tokens_output,
        );
    }
    println!(
        "+--------------------+--------------------------------+----------+---------------+----------+-----------+--------------+---------------+\n"
    );

    Ok(())
}

fn run_reload(pid_opt: Option<u32>) -> Result<(), Box<dyn std::error::Error>> {
    let pid = match pid_opt {
        Some(p) => p,
        None => {
            let content = std::fs::read_to_string("/tmp/oxllm.pid")
                .map_err(|_| "Failed to read PID from /tmp/oxllm.pid. Is the gateway running?")?;
            content
                .trim()
                .parse::<u32>()
                .map_err(|_| "Invalid PID in /tmp/oxllm.pid")?
        },
    };
    send_sighup(pid)?;
    println!(
        "Successfully sent hot-reload signal (SIGHUP) to process {}",
        pid
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup logging
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,oxllm=debug,oxllm_core=debug"));
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { config } => {
            run_serve(config).await?;
        },
        Commands::Validate { config } => {
            run_validate(config)?;
        },
        Commands::Status { port } => {
            run_status(port).await?;
        },
        Commands::Reload { pid } => {
            run_reload(pid)?;
        },
    }

    Ok(())
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use oxllm_core::config::VirtualModelTarget;
    use serde_json::Value;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

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
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
        };

        let p2 = ProviderState {
            name: "prov2".to_string(),
            base_url: Url::parse(&format!("http://{}", addr2)).unwrap(),
            api_key: "key2".to_string(),
            models: vec!["gpt-4-upstream".to_string()],
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
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
        });

        let (_watch_sender, watch_receiver) = tokio::sync::watch::channel(app_state.clone());
        let (telemetry_tx, _telemetry_rx) = tokio::sync::mpsc::channel(1024);
        let telemetry_client = TelemetryClient::new(telemetry_tx);

        let reloadable_state = ReloadableState {
            app_state: watch_receiver,
            telemetry: telemetry_client,
            start_time: Instant::now(),
        };

        let router = axum::Router::new()
            .route(
                "/v1/chat/completions",
                axum::routing::post(routes::create_chat_completions),
            )
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
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
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
        });

        let (_watch_sender, watch_receiver) = tokio::sync::watch::channel(app_state.clone());
        let (telemetry_tx, _telemetry_rx) = tokio::sync::mpsc::channel(1024);
        let telemetry_client = TelemetryClient::new(telemetry_tx);

        let reloadable_state = ReloadableState {
            app_state: watch_receiver,
            telemetry: telemetry_client,
            start_time: Instant::now(),
        };

        let router = axum::Router::new()
            .route(
                "/v1/chat/completions",
                axum::routing::post(routes::create_chat_completions),
            )
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
}
