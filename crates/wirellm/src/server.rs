use axum::{
    body::Body,
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, ConnectInfo, State},
    http::{header, HeaderValue, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Router,
};
use std::collections::VecDeque;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info, warn};
use reqwest::Url;
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};

use wirellm_core::config::Config;
use wirellm_core::db::{get_wirellm_home, DbManager};
use wirellm_core::state::{AppState, CircuitState, FaultHistogram, ProviderState};
use wirellm_core::telemetry::{TelemetryClient, TelemetryWorker};

use crate::admin;
use crate::cli::resolve_config_path;
use crate::mcp;
use crate::routes;
use crate::service::write_pid_file;

#[derive(RustEmbed)]
#[folder = "../../frontend/dist/"]
pub struct Assets;

pub async fn static_handler(uri: axum::http::Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/').to_string();
    if path.is_empty()
        || path == "ui"
        || path == "dashboard"
        || path == "overview"
        || path == "providers"
        || path == "models"
        || path == "analytics"
        || path == "logs"
    {
        path = "index.html".to_string();
    }
    let is_html = path == "index.html" || path.ends_with(".html");
    match Assets::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            let cache_control = if is_html {
                "no-cache, no-store, must-revalidate"
            } else {
                "public, max-age=31536000, immutable"
            };
            (
                [
                    (header::CONTENT_TYPE, mime.as_ref()),
                    (header::CACHE_CONTROL, cache_control),
                ],
                content.data,
            ).into_response()
        },
        None => match Assets::get("index.html") {
            Some(content) => (
                [
                    (header::CONTENT_TYPE, "text/html"),
                    (header::CACHE_CONTROL, "no-cache, no-store, must-revalidate"),
                ],
                content.data,
            ).into_response(),
            None => (StatusCode::NOT_FOUND, "Not Found").into_response(),
        },
    }
}

#[derive(Clone)]
pub struct Reloader {
    pub sender: tokio::sync::watch::Sender<Arc<AppState>>,
    pub config_path: PathBuf,
    pub db: Arc<DbManager>,
}

impl Reloader {
    pub fn reload(&self) -> Result<(), String> {
        let mut config = match self.db.load_config() {
            Ok(c) if !c.providers.is_empty() => c,
            _ => Config::load_from_file(&self.config_path)
                .map_err(|e| format!("Failed to load config file: {}", e))?,
        };
        // Merge dynamic database API keys into active auth config
        if let Ok(db_keys) = self.db.get_active_api_keys() {
            for k in db_keys {
                if !config.auth.api_keys.contains(&k) {
                    config.auth.api_keys.push(k);
                }
            }
        }
        config.validate().map_err(|e| format!("Config validation failed: {}", e))?;
        let new_state = build_app_state(config, self.db.clone())?;
        self.sender
            .send(Arc::new(new_state))
            .map_err(|e| format!("Failed to update watch channel: {}", e))?;
        info!("WireLLM state reloaded successfully");
        Ok(())
    }

    #[cfg(test)]
    pub fn test_new(sender: tokio::sync::watch::Sender<Arc<AppState>>) -> Self {
        let p = std::env::temp_dir().join(format!("oxllm_test_{:016x}.db", rand::random::<u64>()));
        let db = Arc::new(DbManager::new(Some(p)).unwrap());
        Self {
            sender,
            config_path: PathBuf::from("config.toml"),
            db,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct LiveStreamEvent {
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_model: Option<String>,
    pub provider: String,
    pub model: String,
    pub delta: String,
    pub is_done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub tokens_so_far: usize,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentStreamItem {
    pub event: LiveStreamEvent,
    pub timestamp_ms: u64,
}

#[derive(Clone)]
pub struct ReloadableState {
    pub app_state: tokio::sync::watch::Receiver<Arc<AppState>>,
    pub telemetry: TelemetryClient,
    pub start_time: Instant,
    pub reloader: Reloader,
    pub db: Arc<DbManager>,
    pub live_tx: tokio::sync::broadcast::Sender<LiveStreamEvent>,
    pub recent_streams: Arc<tokio::sync::RwLock<VecDeque<RecentStreamItem>>>,
}

#[cfg(test)]
impl ReloadableState {
    pub fn test_new(
        app_state: tokio::sync::watch::Receiver<Arc<AppState>>,
        telemetry: TelemetryClient,
        reloader: Reloader,
    ) -> Self {
        let db = reloader.db.clone();
        let (live_tx, _) = tokio::sync::broadcast::channel(16);
        let recent_streams = Arc::new(tokio::sync::RwLock::new(VecDeque::new()));
        Self {
            app_state,
            telemetry,
            start_time: Instant::now(),
            reloader,
            db,
            live_tx,
            recent_streams,
        }
    }
}

impl axum::extract::FromRef<ReloadableState> for Arc<DbManager> {
    fn from_ref(state: &ReloadableState) -> Self {
        state.db.clone()
    }
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

impl axum::extract::FromRef<ReloadableState> for tokio::sync::broadcast::Sender<LiveStreamEvent> {
    fn from_ref(state: &ReloadableState) -> Self {
        state.live_tx.clone()
    }
}

impl axum::extract::FromRef<ReloadableState>
    for (
        Arc<AppState>,
        TelemetryClient,
        tokio::sync::broadcast::Sender<LiveStreamEvent>,
    )
{
    fn from_ref(state: &ReloadableState) -> Self {
        (
            state.app_state.borrow().clone(),
            state.telemetry.clone(),
            state.live_tx.clone(),
        )
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

impl axum::extract::FromRef<ReloadableState> for Reloader {
    fn from_ref(state: &ReloadableState) -> Self {
        state.reloader.clone()
    }
}

pub fn build_app_state(config: Config, db: Arc<DbManager>) -> Result<AppState, String> {
    if let Ok(policy) = db.get_routing_policy() {
        wirellm_core::state::apply_routing_policy(&policy);
    }
    let mut providers = Vec::new();
    for p in config.providers {
        if !p.enabled {
            continue;
        }
        let mut clean_url = p.base_url.trim().to_string();
        if !clean_url.ends_with('/') {
            clean_url.push('/');
        }
        let url = Url::parse(&clean_url).map_err(|e| {
            format!(
                "Invalid base URL '{}' for provider '{}': {}",
                p.base_url, p.name, e
            )
        })?;

        // Restore the persisted per-hour fault memory for this provider
        let diurnal = FaultHistogram::new();
        if let Ok(Some(saved)) = db.load_provider_fault_pattern(&p.name) {
            diurnal.restore(&saved);
        }

        providers.push(ProviderState {
            name: p.name,
            base_url: url,
            api_key: p.api_key,
            models: p.models,
            headers: p.headers,
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
            max_rpm: p.rpm,
            min_interval_ms: p.min_interval_ms,
            max_concurrency: p.max_concurrency,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(diurnal),
            fault_pattern_store: Some(db.clone()),
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        });
    }

    let http_client = reqwest::Client::builder()
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let mut auth = config.auth.clone();
    auth.merge_env_vars();

    Ok(AppState {
        providers,
        virtual_models: config.virtual_models,
        http_client,
        upstream_timeout_secs: config.server.upstream_timeout_secs,
        auth,
    })
}

/// Generates a random request ID for response correlation.
pub fn generate_request_id() -> String {
    format!("wirellm-{:016x}", rand::random::<u64>())
}

/// Middleware that adds an `x-request-id` header to every response.
pub async fn add_request_id(mut req: Request<Body>, next: Next) -> Response {
    let request_id = generate_request_id();
    req.extensions_mut().insert(request_id.clone());
    let mut response = next.run(req).await;
    if !response.headers().contains_key("x-request-id") {
        response.headers_mut().insert(
            "x-request-id",
            HeaderValue::from_str(&request_id)
                .expect("generated request ID contains invalid characters"),
        );
    }
    response
}

pub async fn localhost_only(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let is_local = match addr.ip() {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback() || v6.to_canonical().is_loopback(),
    };
    if is_local {
        next.run(req).await
    } else {
        warn!(target: "wirellm::security", "Blocked external attempt to access administrative route from IP: {}", addr.ip());
        let body = serde_json::json!({
            "error": {
                "message": "Access denied: administrative routes are localhost-only",
                "type": "forbidden",
                "code": 403
            }
        });
        let bytes = serde_json::to_vec(&body).unwrap();
        let mut response = Response::new(Body::from(bytes));
        *response.status_mut() = StatusCode::FORBIDDEN;
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        response
    }
}

pub async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

pub async fn shutdown_signal() {
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

    // Clean up PID files on shutdown
    let _ = std::fs::remove_file(get_wirellm_home().join("wirellm.pid"));
    if let Ok(content) = std::fs::read_to_string("/tmp/wirellm.pid") {
        if content.trim() == std::process::id().to_string() {
            let _ = std::fs::remove_file("/tmp/wirellm.pid");
        }
    }
}

pub async fn handle_http_reload(
    State(reloader): State<Reloader>,
) -> impl IntoResponse {
    match reloader.reload() {
        Ok(()) => {
            let body = serde_json::json!({
                "message": "Configuration reloaded successfully"
            });
            let bytes = serde_json::to_vec(&body).unwrap();
            let mut response = Response::new(Body::from(bytes));
            *response.status_mut() = StatusCode::OK;
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            response
        },
        Err(e) => {
            let body = serde_json::json!({
                "error": {
                    "message": format!("Failed to reload config: {}", e),
                    "type": "invalid_request_error",
                    "code": 400
                }
            });
            let bytes = serde_json::to_vec(&body).unwrap();
            let mut response = Response::new(Body::from(bytes));
            *response.status_mut() = StatusCode::BAD_REQUEST;
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            response
        },
    }
}

pub async fn handle_sighup(reloader: Reloader) {
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
            if let Err(e) = reloader.reload() {
                error!("Failed to reload configuration during SIGHUP: {}", e);
            }
        }
    }
}

pub async fn ws_live_handler(
    ws: WebSocketUpgrade,
    crate::auth::IsLocalhost(is_local): crate::auth::IsLocalhost,
    headers: axum::http::HeaderMap,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
    State(state): State<ReloadableState>,
) -> Response {
    let app_state = state.app_state.borrow().clone();
    let auth = &app_state.auth;

    if !is_local && (auth.is_google_auth_enabled() || auth.is_api_auth_required()) {
        let secret = crate::auth::resolve_session_secret(auth);
        let has_session = crate::auth::extract_session_user(&headers, &secret).is_some();
        let query_token = query.get("token").map(|s| s.as_str());
        let has_api_key = crate::auth::extract_api_key(&headers, query_token)
            .map(|k| crate::auth::is_api_key_valid(&k, &auth.api_keys))
            .unwrap_or(false);

        if !has_session && !has_api_key {
            return (StatusCode::UNAUTHORIZED, "Unauthorized WebSocket connection").into_response();
        }
    }

    ws.on_upgrade(move |socket| handle_live_socket(socket, state))
}

async fn handle_live_socket(
    mut socket: WebSocket,
    state: ReloadableState,
) {
    let mut rx = state.live_tx.subscribe();
    let mut ticker = tokio::time::interval(Duration::from_millis(1000));
    // First tick fires immediately, let's consume it
    ticker.tick().await;

    // Send initial status immediately on connection
    if let Ok(initial_status) = crate::admin::build_status_response(&state).await {
        let msg = serde_json::json!({
            "type": "status",
            "data": initial_status,
        });
        if socket.send(Message::Text(msg.to_string().into())).await.is_err() {
            return;
        }
    }

    // Send recent streams from past 5 minutes immediately on connection
    {
        let guard = state.recent_streams.read().await;
        if !guard.is_empty() {
            let list: Vec<_> = guard.iter().cloned().collect();
            let msg = serde_json::json!({
                "type": "recent_streams",
                "data": list,
            });
            let _ = socket.send(Message::Text(msg.to_string().into())).await;
        }
    }

    loop {
        tokio::select! {
            // Live token stream broadcast
            event_res = rx.recv() => {
                match event_res {
                    Ok(event) => {
                        let msg = serde_json::json!({
                            "type": "token",
                            "data": event,
                        });
                        if socket.send(Message::Text(msg.to_string().into())).await.is_err() {
                            break;
                        }
                    },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }

            // Periodic 1-second real-time telemetry/status push
            _ = ticker.tick() => {
                if let Ok(current_status) = crate::admin::build_status_response(&state).await {
                    let msg = serde_json::json!({
                        "type": "status",
                        "data": current_status,
                    });
                    if socket.send(Message::Text(msg.to_string().into())).await.is_err() {
                        break;
                    }
                }
            }

            // Incoming client messages (ping, refresh requests)
            msg_res = socket.recv() => {
                match msg_res {
                    Some(Ok(Message::Text(text))) => {
                        if text.contains("refresh") {
                            if let Ok(current_status) = crate::admin::build_status_response(&state).await {
                                let msg = serde_json::json!({
                                    "type": "status",
                                    "data": current_status,
                                });
                                let _ = socket.send(Message::Text(msg.to_string().into())).await;
                            }
                        }
                    },
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    },
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

pub async fn run_serve(config_path: PathBuf, cli_port: Option<u16>) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = resolve_config_path(config_path);

    // 0. Initialize SQLite database manager (~/.wirellm/wirellm.db)
    let db = Arc::new(DbManager::new(None::<&str>)?);

    // 1. Load initial config (from DB if populated, otherwise fallback to config file)
    let mut config = match db.load_config() {
        Ok(cfg) if !cfg.providers.is_empty() => {
            info!("Loaded configuration with {} providers from SQLite database", cfg.providers.len());
            if let Ok(file_cfg) = Config::load_from_file(&config_path) {
                let mut merged = cfg;
                merged.server = file_cfg.server;
                merged
            } else {
                cfg
            }
        },
        _ => {
            let cfg = Config::load_from_file(&config_path)?;
            cfg.validate()?;
            cfg
        },
    };

    if let Some(p) = cli_port {
        config.server.port = p;
    } else if let Ok(p_str) = std::env::var("PORT").or_else(|_| std::env::var("WIRELLM_PORT")) {
        if let Ok(p) = crate::cli::parse_port(&p_str) {
            config.server.port = p;
        }
    }

    if let Ok(h) = std::env::var("WIRELLM_HOST").or_else(|_| std::env::var("HOST")) {
        if !h.trim().is_empty() {
            config.server.host = h.trim().to_string();
        }
    }

    // Merge active database API keys into active auth config
    if let Ok(db_keys) = db.get_active_api_keys() {
        for k in db_keys {
            if !config.auth.api_keys.contains(&k) {
                config.auth.api_keys.push(k);
            }
        }
    }

    // 2. Build initial AppState
    let app_state = Arc::new(build_app_state(config.clone(), db.clone())?);

    // 3. Initialize watch channel
    let (watch_sender, watch_receiver) = tokio::sync::watch::channel(app_state.clone());

    // 4. Initialize telemetry channel & spawn TelemetryWorker with SQLite integration
    let (telemetry_tx, telemetry_rx) = tokio::sync::mpsc::channel(1024);
    let telemetry_client = TelemetryClient::new(telemetry_tx);
    if let Err(e) = TelemetryWorker::spawn(
        &config.server.otel_endpoint,
        telemetry_rx,
        Some(db.clone()),
    ) {
        warn!("Failed to spawn telemetry worker: {}", e);
    }

    // 5. Initialize hot-reloader & spawn SIGHUP listener
    let reloader = Reloader {
        sender: watch_sender.clone(),
        config_path: config_path.clone(),
        db: db.clone(),
    };
    tokio::spawn(handle_sighup(reloader.clone()));

    // 6. Build Axum Router
    let start_time = Instant::now();
    let (live_tx, _) = tokio::sync::broadcast::channel::<LiveStreamEvent>(256);
    let recent_streams = Arc::new(tokio::sync::RwLock::new(VecDeque::<RecentStreamItem>::new()));

    // Background task: persist live events in a bounded ring buffer (past 5 minutes)
    let recent_streams_bg = recent_streams.clone();
    let mut live_rx = live_tx.subscribe();
    tokio::spawn(async move {
        while let Ok(event) = live_rx.recv().await {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let mut guard = recent_streams_bg.write().await;
            if let Some(pos) = guard.iter().position(|item| item.event.request_id == event.request_id) {
                if !event.delta.is_empty() {
                    guard[pos].event.delta.push_str(&event.delta);
                    if guard[pos].event.delta.len() > 1000 {
                        let excess = guard[pos].event.delta.len() - 1000;
                        let start = guard[pos].event.delta
                            .char_indices()
                            .map(|(idx, _)| idx)
                            .find(|&idx| idx >= excess)
                            .unwrap_or(0);
                        guard[pos].event.delta = guard[pos].event.delta[start..].to_string();
                    }
                }
                guard[pos].event.is_done = event.is_done;
                guard[pos].event.tokens_so_far = event.tokens_so_far;
                guard[pos].event.elapsed_ms = event.elapsed_ms;
                guard[pos].timestamp_ms = now;
            } else {
                guard.push_back(RecentStreamItem {
                    event,
                    timestamp_ms: now,
                });
            }
            while guard.len() > 30 || (guard.front().map_or(false, |it| now.saturating_sub(it.timestamp_ms) > 300_000)) {
                guard.pop_front();
            }
        }
    });

    let reloadable_state = ReloadableState {
        app_state: watch_receiver,
        telemetry: telemetry_client,
        start_time,
        reloader,
        db,
        live_tx,
        recent_streams,
    };

    // 1. API routes (/v1/* and /mcp/*) with optional API Key Bearer authentication
    let api_routes = Router::new()
        .route("/v1/models", get(routes::list_models))
        .route("/v1/embeddings", post(routes::create_embeddings))
        .route(
            "/v1/chat/completions",
            post(routes::create_chat_completions),
        )
        .route("/mcp/sse", get(mcp::mcp_sse_handler))
        .route("/mcp/messages", post(mcp::mcp_messages_handler))
        .route(
            "/mcp",
            post(mcp::mcp_streamable_post_handler)
                .get(mcp::mcp_streamable_get_handler)
                .delete(mcp::mcp_streamable_delete_handler),
        )
        .layer(middleware::from_fn_with_state(
            reloadable_state.clone(),
            crate::auth::require_api_auth,
        ));

    // 2. Admin routes (/api/*) with Google Auth session or API Key authentication
    let admin_routes = Router::new()
        .route("/api/status", get(admin::get_api_status))
        .route(
            "/api/providers",
            get(admin::list_providers).post(admin::create_provider),
        )
        .route(
            "/api/providers/{name}",
            put(admin::update_provider).delete(admin::delete_provider),
        )
        .route("/api/providers/{name}/test", post(admin::test_provider))
        .route("/api/providers/discover-models", post(admin::discover_models_endpoint))
        .route("/api/virtual-models", get(admin::list_virtual_models))
        .route(
            "/api/virtual-models/{name}",
            put(admin::upsert_virtual_model).delete(admin::delete_virtual_model),
        )
        .route("/api/analytics/daily", get(admin::get_daily_analytics))
        .route("/api/setup/recommendations", get(admin::get_setup_recommendations))
        .route("/api/keys", get(admin::list_api_keys).post(admin::create_api_key))
        .route("/api/keys/{id}", delete(admin::delete_api_key))
        .route("/api/vault/secrets", get(admin::list_vault_secrets).post(admin::create_vault_secret))
        .route("/api/vault/secrets/{name}", get(admin::get_vault_secret).delete(admin::delete_vault_secret))
        .route(
            "/api/routing-policy",
            get(admin::get_routing_policy)
                .post(admin::update_routing_policy)
                .put(admin::update_routing_policy),
        )
        .route("/api/logs", get(routes::get_logs))
        .layer(middleware::from_fn_with_state(
            reloadable_state.clone(),
            crate::auth::require_admin_or_ui_auth,
        ));

    // 3. Main composite router
    let app = Router::new()
        .merge(api_routes)
        .merge(admin_routes)
        // Public Health Check (essential for Kubernetes liveness & readiness probes)
        .route("/health", get(health_check))
        // Authentication endpoints
        .route("/auth/status", get(crate::auth::get_auth_status))
        .route("/auth/me", get(crate::auth::get_auth_status))
        .route("/auth/google/login", get(crate::auth::google_login_handler))
        .route("/auth/google/callback", get(crate::auth::google_callback_handler))
        .route(
            "/auth/logout",
            get(crate::auth::logout_handler).post(crate::auth::logout_handler),
        )
        .route("/ws/live", get(ws_live_handler))
        // Legacy admin and status routes (localhost only)
        .route(
            "/status",
            get(routes::get_status).layer(middleware::from_fn(localhost_only)),
        )
        .route(
            "/logs",
            get(routes::get_logs).layer(middleware::from_fn(localhost_only)),
        )
        .route(
            "/reload",
            post(handle_http_reload).layer(middleware::from_fn(localhost_only)),
        )
        .route(
            "/admin/providers/{name}/offline",
            post(routes::admin_offline).layer(middleware::from_fn(localhost_only)),
        )
        .route(
            "/admin/providers/{name}/online",
            post(routes::admin_online).layer(middleware::from_fn(localhost_only)),
        )
        .route(
            "/admin/providers/{name}/reset",
            post(routes::admin_reset).layer(middleware::from_fn(localhost_only)),
        )
        // Embedded SPA Dashboard (React + Vite)
        .route("/", get(static_handler))
        .route("/ui", get(static_handler))
        .route("/dashboard", get(static_handler))
        .route("/overview", get(static_handler))
        .route("/providers", get(static_handler))
        .route("/models", get(static_handler))
        .route("/analytics", get(static_handler))
        .route("/assets/{*file}", get(static_handler))
        .layer(middleware::from_fn(add_request_id))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::PUT,
                    axum::http::Method::DELETE,
                    axum::http::Method::OPTIONS,
                ])
                .allow_headers(Any),
        )
        .with_state(reloadable_state);

    // Write PID file
    if let Err(e) = write_pid_file() {
        warn!("Failed to write PID file: {}", e);
    }

    // Start listening
    let port = config.server.port;
    let listener = match config.server.bind_family.as_str() {
        "ipv6" => {
            let addr = format!("[::]:{}", port);
            info!("Listening on http://{} (IPv6 only)", addr);
            tokio::net::TcpListener::bind(&addr).await?
        },
        "dual" => {
            let addr = std::net::SocketAddr::new(
                std::net::IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
                port,
            );
            let socket = socket2::Socket::new(
                socket2::Domain::IPV6,
                socket2::Type::STREAM,
                Some(socket2::Protocol::TCP),
            )
            .map_err(|e| format!("Failed to create socket: {}", e))?;
            socket
                .set_only_v6(false)
                .map_err(|e| format!("Failed to set dual-stack: {}", e))?;
            socket
                .set_reuse_address(true)
                .map_err(|e| format!("Failed to set reuse address: {}", e))?;
            socket
                .bind(&addr.into())
                .map_err(|e| format!("Failed to bind: {}", e))?;
            socket
                .listen(1024)
                .map_err(|e| format!("Failed to listen: {}", e))?;
            socket
                .set_nonblocking(true)
                .map_err(|e| format!("Failed to set non-blocking: {}", e))?;
            info!("Listening on [::]:{} (dual-stack IPv4/IPv6)", port);
            tokio::net::TcpListener::from_std(socket.into())
                .map_err(|e| format!("Failed to create tokio listener: {}", e))?
        },
        _ => {
            // Default: IPv4
            let addr = format!("{}:{}", config.server.host, port);
            info!("Listening on http://{} (IPv4)", addr);
            tokio::net::TcpListener::bind(&addr).await?
        },
    };

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}
