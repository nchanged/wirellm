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
/// failures to 0, rate limit cleared, manual disabled cleared.
pub async fn admin_reset(
    State(app_state): State<Arc<AppState>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    match app_state.providers.iter().find(|p| p.name == name) {
        Some(provider) => {
            *provider.circuit.write().await = CircuitState::Closed;
            *provider.consecutive_failures.write().await = 0;
            *provider.rate_limited_until.write().await = None;
            provider
                .manual_disabled
                .store(false, std::sync::atomic::Ordering::Release);
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

/// GET /logs — Returns recent server logs from ~/.wirellm/wirellm.log or /tmp/wirellm.log
pub async fn get_logs(
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<LogsQuery>,
) -> impl IntoResponse {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let default_log = format!("{}/.wirellm/wirellm.log", home);
    let log_path = std::env::var("WIRELLM_LOG")
        .or_else(|_| std::env::var("OXLLM_LOG"))
        .unwrap_or_else(|_| {
            if std::path::Path::new(&default_log).exists() {
                default_log
            } else if std::path::Path::new("/tmp/wirellm.log").exists() {
                "/tmp/wirellm.log".to_string()
            } else {
                "/tmp/wirellm.log".to_string()
            }
        });
    let limit = query.limit.unwrap_or(200).min(2000);
    let filter = query.filter.map(|f| f.to_lowercase());

    let (lines, count) = match tokio::fs::read_to_string(&log_path).await {
        Ok(content) => {
            let mut matched: Vec<String> = content
                .lines()
                .filter(|line| {
                    if let Some(ref term) = filter {
                        line.to_lowercase().contains(term)
                    } else {
                        true
                    }
                })
                .map(|s| s.to_string())
                .collect();

            let total = matched.len();
            if total > limit {
                matched = matched.split_off(total - limit);
            }
            (matched, total)
        },
        Err(e) => (
            vec![format!("Log file not accessible at {}: {}", log_path, e)],
            1,
        ),
    };

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
