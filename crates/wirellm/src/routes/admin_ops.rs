use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::sync::Arc;
use std::time::Instant;
use wirellm_core::state::{AppState, CircuitState};

#[derive(Serialize)]
pub struct RouteEntry {
    pub provider: String,
    pub model: String,
    pub circuit: String,
    pub requests: u64,
    pub successes: u64,
    pub max_rpm: Option<u32>,
    pub current_rpm: u32,
    pub is_rpm_capped: bool,
    pub min_interval_ms: Option<u64>,
    pub is_burst_limited: bool,
    pub rate_limited_until_secs: Option<u64>,
    pub in_flight: u32,
    pub recent_faults: u32,
    pub breathing_room_secs: u64,
    pub auto_disabled_reason: Option<String>,
    /// Seconds until a severed (auto-disabled) provider is re-probed; None when healthy.
    pub auto_disabled_retry_secs: Option<u64>,
}

#[derive(Serialize)]
pub struct ProviderStatus {
    pub name: String,
    pub models: String,
    pub circuit: String,
    pub failures: u32,
    pub rate_limited: bool,
    pub rate_limited_until_secs: Option<u64>,
    pub requests: u64,
    pub successes: u64,
    pub tokens_input: u64,
    pub tokens_output: u64,
    pub last_request: String,
    pub max_rpm: Option<u32>,
    pub current_rpm: u32,
    pub is_rpm_capped: bool,
    pub min_interval_ms: Option<u64>,
    pub is_burst_limited: bool,
    pub in_flight: u32,
    pub recent_faults: u32,
    pub breathing_room_secs: u64,
    pub auto_disabled_reason: Option<String>,
    /// Seconds until a severed (auto-disabled) provider is re-probed; None when healthy.
    pub auto_disabled_retry_secs: Option<u64>,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub uptime_secs: u64,
    pub total_requests: u64,
    pub providers: Vec<ProviderStatus>,
    pub virtual_models: std::collections::HashMap<String, Vec<RouteEntry>>,
}

/// GET /status (localhost restricted in main.rs routing)
pub async fn get_status(
    State((app_state, start_time)): State<(Arc<AppState>, Instant)>,
) -> impl IntoResponse {
    let mut status_list = Vec::new();
    let mut total_requests: u64 = 0;
    let now = Instant::now();
    let policy = wirellm_core::state::current_routing_policy();

    for provider in &app_state.providers {
        let circ = *provider.circuit.read().await;
        let circuit_str = match circ {
            CircuitState::Closed => "Closed (Healthy)".to_string(),
            CircuitState::HalfOpen => "Half-Open (Probing)".to_string(),
            CircuitState::Open { until } => {
                let left = until.saturating_duration_since(now).as_secs();
                format!("Open (Cooldown: {}s left)", left)
            },
        };

        let rl = *provider.rate_limited_until.read().await;
        let (is_limited, rate_limited_until_secs) = match rl {
            Some(until) if now < until => (true, Some(until.saturating_duration_since(now).as_secs())),
            _ => (false, None),
        };

        let failures = *provider.consecutive_failures.read().await;

        let requests = provider.requests.load(std::sync::atomic::Ordering::Relaxed);
        let successes = provider
            .successes
            .load(std::sync::atomic::Ordering::Relaxed);
        let tokens_input = provider
            .tokens_input
            .load(std::sync::atomic::Ordering::Relaxed);
        let tokens_output = provider
            .tokens_output
            .load(std::sync::atomic::Ordering::Relaxed);

        let last_request = {
            let last = provider.last_attempt_time.read().await;
            match *last {
                Some(instant) => {
                    let elapsed = now.saturating_duration_since(instant);
                    if elapsed.as_secs() < 60 {
                        "Just now".to_string()
                    } else if elapsed.as_secs() < 3600 {
                        format!("{}m ago", elapsed.as_secs() / 60)
                    } else {
                        format!("{}h ago", elapsed.as_secs() / 3600)
                    }
                },
                None => "Never".to_string(),
            }
        };

        let current_rpm = provider.current_rpm(now).await;
        let is_rpm_capped = provider.is_rpm_limited(now).await;
        let is_burst_limited = provider.is_burst_limited(now).await;
        let in_flight = provider.in_flight.load(std::sync::atomic::Ordering::Relaxed);
        let recent_faults = provider.fault_history.read().await.len() as u32;
        // `severed_hold` (not the raw marker) so an elapsed quarantine is
        // released and status never reports a recovered model as blocked.
        let auto_disabled_reason = provider.severed_hold(now, &policy).await;
        let auto_disabled_retry_secs = provider.severed_retry_secs(now, &policy).await;

        total_requests += requests;

        status_list.push(ProviderStatus {
            name: provider.name.clone(),
            models: provider.models.join(", "),
            circuit: circuit_str,
            failures,
            rate_limited: is_limited,
            rate_limited_until_secs,
            requests,
            successes,
            tokens_input,
            tokens_output,
            last_request,
            max_rpm: provider.max_rpm,
            current_rpm,
            is_rpm_capped,
            min_interval_ms: provider.min_interval_ms,
            is_burst_limited,
            in_flight,
            recent_faults,
            breathing_room_secs: rate_limited_until_secs.unwrap_or(0),
            auto_disabled_reason,
            auto_disabled_retry_secs,
        });
    }

    // Build virtual model routing table
    let mut virtual_models: std::collections::HashMap<String, Vec<RouteEntry>> =
        std::collections::HashMap::new();

    for (vm_name, targets) in &app_state.virtual_models {
        let mut entries = Vec::new();
        for target in targets {
            let provider_state = app_state
                .providers
                .iter()
                .find(|p| p.name == target.provider);
            let (circuit_str, requests, successes, max_rpm, current_rpm, is_rpm_capped, min_interval_ms, is_burst_limited, rate_limited_until_secs, in_flight) = match provider_state {
                Some(provider) => {
                    let circ = *provider.circuit.read().await;
                    let now = Instant::now();
                    let circuit_str = match circ {
                        CircuitState::Closed => "Closed (Healthy)".to_string(),
                        CircuitState::HalfOpen => "Half-Open (Probing)".to_string(),
                        CircuitState::Open { until } => {
                            let left = until.saturating_duration_since(now).as_secs();
                            format!("Open ({}s cooldown)", left)
                        },
                    };
                    let requests = provider.requests.load(std::sync::atomic::Ordering::Relaxed);
                    let successes = provider
                        .successes
                        .load(std::sync::atomic::Ordering::Relaxed);
                    let current_rpm = provider.current_rpm(now).await;
                    let is_rpm_capped = provider.is_rpm_limited(now).await;
                    let is_burst_limited = provider.is_burst_limited(now).await;
                    let in_flight = provider.in_flight.load(std::sync::atomic::Ordering::Relaxed);
                    let rl = *provider.rate_limited_until.read().await;
                    let rate_limited_until_secs = match rl {
                        Some(until) if now < until => Some(until.saturating_duration_since(now).as_secs()),
                        _ => None,
                    };
                    (
                        circuit_str,
                        requests,
                        successes,
                        provider.max_rpm,
                        current_rpm,
                        is_rpm_capped,
                        provider.min_interval_ms,
                        is_burst_limited,
                        rate_limited_until_secs,
                        in_flight,
                    )
                },
                None => ("Unknown".to_string(), 0, 0, None, 0, false, None, false, None, 0),
            };
            let (recent_faults, auto_disabled_reason, auto_disabled_retry_secs) = match provider_state {
                Some(p) => (
                    p.fault_history.read().await.len() as u32,
                    p.severed_hold(now, &policy).await,
                    p.severed_retry_secs(now, &policy).await,
                ),
                None => (0, None, None),
            };
            entries.push(RouteEntry {
                provider: target.provider.clone(),
                model: target.model.clone(),
                circuit: circuit_str,
                requests,
                successes,
                max_rpm,
                current_rpm,
                is_rpm_capped,
                min_interval_ms,
                is_burst_limited,
                rate_limited_until_secs,
                in_flight,
                recent_faults,
                breathing_room_secs: rate_limited_until_secs.unwrap_or(0),
                auto_disabled_reason,
                auto_disabled_retry_secs,
            });
        }
        virtual_models.insert(vm_name.clone(), entries);
    }

    Json(StatusResponse {
        uptime_secs: start_time.elapsed().as_secs(),
        total_requests,
        providers: status_list,
        virtual_models,
    })
}

/// POST /admin/providers/{name}/offline
pub async fn admin_offline(
    State(app_state): State<Arc<AppState>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    match app_state.providers.iter().find(|p| p.name == name) {
        Some(provider) => {
            provider
                .manual_disabled
                .store(true, std::sync::atomic::Ordering::Release);
            (StatusCode::OK, format!("Provider '{}' taken offline", name))
        },
        None => (
            StatusCode::NOT_FOUND,
            format!("Provider '{}' not found", name),
        ),
    }
}

/// POST /admin/providers/{name}/online
pub async fn admin_online(
    State(app_state): State<Arc<AppState>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    match app_state.providers.iter().find(|p| p.name == name) {
        Some(provider) => {
            provider
                .manual_disabled
                .store(false, std::sync::atomic::Ordering::Release);
            // Manual re-enable also clears an auto-disabled (severed) marker —
            // e.g. after fixing the model name in config or the upstream
            // restoring a removed model — and any pending breathing window.
            *provider.auto_disabled_reason.write().await = None;
            *provider.auto_disabled_at.write().await = None;
            provider
                .severed_disable_count
                .store(0, std::sync::atomic::Ordering::Release);
            provider
                .severed_strikes
                .store(0, std::sync::atomic::Ordering::Release);
            *provider.rate_limited_until.write().await = None;
            (
                StatusCode::OK,
                format!("Provider '{}' brought online", name),
            )
        },
        None => (
            StatusCode::NOT_FOUND,
            format!("Provider '{}' not found", name),
        ),
    }
}

/// POST /admin/providers/{name}/reset — resets circuit breaker to Closed,
/// failures to 0, rate limit cleared, manual disabled and auto-disabled cleared,
/// breathing room window evaporated.
pub async fn admin_reset(
    State(app_state): State<Arc<AppState>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    match app_state.providers.iter().find(|p| p.name == name) {
        Some(provider) => {
            // Canonical lock order: consecutive_failures → circuit → everything
            // else. Acquiring circuit first inverts the order used by the
            // routing feedback path and can deadlock a concurrent completion.
            *provider.consecutive_failures.write().await = 0;
            *provider.circuit.write().await = CircuitState::Closed;
            *provider.rate_limited_until.write().await = None;
            provider
                .manual_disabled
                .store(false, std::sync::atomic::Ordering::Release);
            *provider.auto_disabled_reason.write().await = None;
            *provider.auto_disabled_at.write().await = None;
            provider
                .severed_disable_count
                .store(0, std::sync::atomic::Ordering::Release);
            provider
                .severed_strikes
                .store(0, std::sync::atomic::Ordering::Release);
            provider.fault_history.write().await.clear();
            (
                StatusCode::OK,
                format!("Provider '{}' reset to healthy", name),
            )
        },
        None => (
            StatusCode::NOT_FOUND,
            format!("Provider '{}' not found", name),
        ),
    }
}

/// GET /ui or GET /dashboard
#[allow(dead_code)]
pub async fn ui_dashboard() -> impl IntoResponse {
    axum::response::Html(include_str!("../ui.html"))
}

#[derive(serde::Deserialize)]
pub struct LogsQuery {
    pub limit: Option<usize>,
    pub filter: Option<String>,
    pub format: Option<String>,
}

#[derive(serde::Serialize)]
pub struct LogsResponse {
    pub count: usize,
    pub path: String,
    pub lines: Vec<String>,
}

/// Resolves the configured log file, filters by `filter`, and returns
/// `(total_matched, path, last-`limit`-matching lines)`. Shared by the HTTP
/// `/logs` endpoint and the MCP `wirellm_get_logs` tool so both behave identically.
pub async fn read_log_lines(limit: usize, filter: Option<String>) -> (usize, String, Vec<String>) {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let default_log = format!("{}/.wirellm/wirellm.log", home);
    let log_path = std::env::var("WIRELLM_LOG")
        .or_else(|_| std::env::var("OXLLM_LOG"))
        .unwrap_or_else(|_| {
            if std::path::Path::new(&default_log).exists() {
                default_log
            } else {
                "/tmp/wirellm.log".to_string()
            }
        });
    let limit = limit.min(2000);
    let filter = filter.map(|f| f.to_lowercase());

    match tokio::fs::read_to_string(&log_path).await {
        Ok(content) => {
            let mut matched: Vec<String> = content
                .lines()
                .filter(|line| filter.as_ref().map_or(true, |term| line.to_lowercase().contains(term)))
                .map(|s| s.to_string())
                .collect();
            let total = matched.len();
            if total > limit {
                matched = matched.split_off(total - limit);
            }
            (total, log_path, matched)
        },
        Err(e) => (
            1,
            log_path.clone(),
            vec![format!("Log file not accessible at {}: {}", log_path, e)],
        ),
    }
}

/// GET /logs — Returns recent server logs from ~/.wirellm/wirellm.log or /tmp/wirellm.log
pub async fn get_logs(
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<LogsQuery>,
) -> impl IntoResponse {
    let (count, log_path, lines) = read_log_lines(query.limit.unwrap_or(200), query.filter).await;

    let wants_text = query.format.as_deref() == Some("text")
        || headers
            .get(header::ACCEPT)
            .and_then(|h| h.to_str().ok())
            .map(|a| a.contains("text/plain"))
            .unwrap_or(false);

    if wants_text {
        let mut resp = Response::new(Body::from(lines.join("\n")));
        *resp.status_mut() = StatusCode::OK;
        resp.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        resp
    } else {
        let body = serde_json::to_vec(&LogsResponse {
            count,
            path: log_path,
            lines,
        })
        .unwrap_or_default();
        let mut resp = Response::new(Body::from(body));
        *resp.status_mut() = StatusCode::OK;
        resp.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        resp
    }
}
