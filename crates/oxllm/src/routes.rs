use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, warn};

use oxllm_core::router::{AdaptivePriorityStrategy, RoutingStrategy};
use oxllm_core::state::{AppState, CircuitState, ProviderState};
use oxllm_core::telemetry::{TelemetryClient, TelemetryEvent};

#[derive(Serialize)]
struct ModelObject {
    id: String,
    object: &'static str,
    created: u64,
    owned_by: &'static str,
}

#[derive(Serialize)]
struct ModelsResponse {
    object: &'static str,
    data: Vec<ModelObject>,
}

/// GET /v1/models
pub async fn list_models(State(app_state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut data = Vec::new();
    let now = Instant::now();

    for (vm_name, targets) in &app_state.virtual_models {
        let mut is_healthy = false;

        for target in targets {
            if let Some(provider) = app_state
                .providers
                .iter()
                .find(|p| p.name == target.provider)
            {
                // Check if provider is healthy (Closed, or expired cooldown ready for HalfOpen)
                let circuit = *provider.circuit.read().await;
                let is_tripped = match circuit {
                    CircuitState::Closed | CircuitState::HalfOpen => false,
                    CircuitState::Open { until } => now < until,
                };

                let rl = *provider.rate_limited_until.read().await;
                let is_rate_limited = match rl {
                    Some(until) => now < until,
                    None => false,
                };

                if !is_tripped && !is_rate_limited {
                    is_healthy = true;
                    break;
                }
            }
        }

        if is_healthy {
            data.push(ModelObject {
                id: vm_name.clone(),
                object: "model",
                created: 1717070400, // May 30, 2026 constant
                owned_by: "oxllm-virtual",
            });
        }
    }

    Json(ModelsResponse {
        object: "list",
        data,
    })
}

#[derive(Serialize)]
struct RouteEntry {
    provider: String,
    model: String,
    circuit: String,
    requests: u64,
    successes: u64,
}

/// GET /status (localhost restricted in main.rs routing)
pub async fn get_status(
    State((app_state, start_time)): State<(Arc<AppState>, Instant)>,
) -> impl IntoResponse {
    #[derive(Serialize)]
    struct ProviderStatus {
        name: String,
        models: String,
        circuit: String,
        failures: u32,
        rate_limited: bool,
        requests: u64,
        successes: u64,
        tokens_input: u64,
        tokens_output: u64,
        last_request: String,
    }

    #[derive(Serialize)]
    struct StatusResponse {
        uptime_secs: u64,
        total_requests: u64,
        providers: Vec<ProviderStatus>,
        virtual_models: std::collections::HashMap<String, Vec<RouteEntry>>,
    }

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
        let is_limited = match rl {
            Some(until) => now < until,
            None => false,
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
                    } else if elapsed.as_secs() < 86400 {
                        format!("{}h ago", elapsed.as_secs() / 3600)
                    } else {
                        format!("{}d ago", elapsed.as_secs() / 86400)
                    }
                },
                None => "Never".to_string(),
            }
        };

        total_requests += requests;

        status_list.push(ProviderStatus {
            name: provider.name.clone(),
            models: provider.models.join(", "),
            circuit: circuit_str,
            failures,
            rate_limited: is_limited,
            requests,
            successes,
            tokens_input,
            tokens_output,
            last_request,
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
            let (circuit_str, requests, successes) = match provider_state {
                Some(provider) => {
                    let circ = *provider.circuit.read().await;
                    let now = std::time::Instant::now();
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
                    (circuit_str, requests, successes)
                },
                None => ("Unknown".to_string(), 0, 0),
            };
            entries.push(RouteEntry {
                provider: target.provider.clone(),
                model: target.model.clone(),
                circuit: circuit_str,
                requests,
                successes,
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

/// POST /v1/embeddings
pub async fn create_embeddings(
    State((app_state, telemetry)): State<(Arc<AppState>, TelemetryClient)>,
    headers: HeaderMap,
    body: Bytes, // Bounded ref-counted bytes for zero-cost routing retries
) -> impl IntoResponse {
    let mut payload: Value = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON payload: {}", e),
            )
                .into_response()
        },
    };

    let requested_model = match payload.get("model").and_then(|m| m.as_str()) {
        Some(m) => m,
        None => return (StatusCode::BAD_REQUEST, "Missing required 'model' field").into_response(),
    };

    let candidates = app_state.resolve_candidates(requested_model);
    if candidates.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            format!("Invalid or unmapped virtual model: {}", requested_model),
        )
            .into_response();
    }

    // Extract W3C traceparent headers if present for tracing
    let (trace_id, parent_span_id) = extract_traceparent(&headers);

    let strategy = AdaptivePriorityStrategy;
    let mut attempts = 0;
    let start_time = Instant::now();

    // Map candidate provider references for select strategy
    let candidate_states: Vec<&ProviderState> = candidates.iter().map(|(p, _)| *p).collect();

    while let Some(selected) = strategy.select(&candidate_states).await {
        attempts += 1;
        let target_model = candidates
            .iter()
            .find(|(p, _)| p.name == selected.name)
            .map(|(_, m)| m.clone())
            .unwrap();

        // 1. Rewrite model field in JSON body
        payload["model"] = Value::String(target_model.clone());
        let rewritten_body = Bytes::from(serde_json::to_vec(&payload).unwrap());

        // 2. Safely parse base URL and join embeddings path (Url::joinTrailingSlashes mitigation)
        let endpoint_url = match selected.base_url.join("embeddings") {
            Ok(url) => url,
            Err(e) => {
                error!("Invalid base URL path join for {}: {}", selected.name, e);
                continue;
            },
        };

        // 3. Construct upstream request
        let mut req = app_state
            .http_client
            .post(endpoint_url.as_str())
            .body(rewritten_body)
            .timeout(Duration::from_secs(app_state.upstream_timeout_secs))
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", selected.api_key));

        // Propagate tracing headers if present
        if let Some(traceparent) = headers.get("traceparent") {
            req = req.header("traceparent", traceparent);
        }

        debug!(
            "Embedding request routing to {} (attempt {})",
            selected.name, attempts
        );

        // Increment request counter
        if let Some(target) = app_state.providers.iter().find(|p| p.name == selected.name) {
            target
                .requests
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        let res = req.send().await;

        match res {
            Ok(res) if res.status().is_success() => {
                let status_code = res.status().as_u16();
                let upstream_headers = res.headers().clone();
                let res_body = match res.bytes().await {
                    Ok(b) => b,
                    Err(e) => {
                        warn!(
                            "Failed to read success response body from {}: {}",
                            selected.name, e
                        );
                        let target_provider_state = app_state
                            .providers
                            .iter()
                            .find(|p| p.name == selected.name)
                            .unwrap();
                        strategy
                            .feedback(
                                target_provider_state,
                                false,
                                selected.is_probe,
                                Some(status_code),
                                None,
                            )
                            .await;
                        continue;
                    },
                };

                // Parse token counts from upstream response
                let (input_tokens, output_tokens) = serde_json::from_slice::<Value>(&res_body)
                    .map(|v| {
                        let usage = v.get("usage");
                        let input = usage
                            .and_then(|u| u.get("prompt_tokens"))
                            .and_then(|t| t.as_u64())
                            .unwrap_or(0);
                        let output = usage
                            .and_then(|u| u.get("completion_tokens"))
                            .and_then(|t| t.as_u64())
                            .unwrap_or(0);
                        (input, output)
                    })
                    .unwrap_or((0, 0));

                // Report Success feedback
                let target_provider_state = app_state
                    .providers
                    .iter()
                    .find(|p| p.name == selected.name)
                    .unwrap();
                strategy
                    .feedback(
                        target_provider_state,
                        true,
                        selected.is_probe,
                        Some(status_code),
                        None,
                    )
                    .await;

                // Increment local counters
                target_provider_state
                    .successes
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                target_provider_state
                    .tokens_input
                    .fetch_add(input_tokens, std::sync::atomic::Ordering::Relaxed);
                target_provider_state
                    .tokens_output
                    .fetch_add(output_tokens, std::sync::atomic::Ordering::Relaxed);

                // Push Telemetry Metrics
                telemetry.emit(TelemetryEvent::RecordTransaction {
                    operation: "embeddings".to_string(),
                    provider: selected.name.clone(),
                    model: target_model,
                    input_tokens,
                    output_tokens,
                    duration: start_time.elapsed(),
                    attempts,
                    failure_reason: None,
                    trace_id: trace_id.clone(),
                    parent_span_id: parent_span_id.clone(),
                });

                // Construct clean Axum response copying headers cleanly via bytes
                let mut response = Response::new(Body::from(res_body));
                *response.status_mut() = StatusCode::OK;
                copy_response_headers(&upstream_headers, response.headers_mut());
                return response.into_response();
            },
            Ok(res) => {
                let status_code = res.status().as_u16();
                warn!(
                    "Embedding request upstream {} failed with status {}",
                    selected.name, status_code
                );

                // Extract Retry-After if present
                let retry_after = extract_retry_after(res.headers());

                let target_provider_state = app_state
                    .providers
                    .iter()
                    .find(|p| p.name == selected.name)
                    .unwrap();
                strategy
                    .feedback(
                        target_provider_state,
                        false,
                        selected.is_probe,
                        Some(status_code),
                        retry_after,
                    )
                    .await;
            },
            Err(e) => {
                warn!(
                    "Embedding request upstream {} connection failed: {}",
                    selected.name, e
                );
                let target_provider_state = app_state
                    .providers
                    .iter()
                    .find(|p| p.name == selected.name)
                    .unwrap();
                strategy
                    .feedback(target_provider_state, false, selected.is_probe, None, None)
                    .await;
            },
        }
    }

    (
        StatusCode::BAD_GATEWAY,
        "All upstream embeddings providers failed or are rate-limited",
    )
        .into_response()
}

/// POST /v1/chat/completions
pub async fn create_chat_completions(
    State((app_state, telemetry)): State<(Arc<AppState>, TelemetryClient)>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let mut payload: Value = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON payload: {}", e),
            )
                .into_response()
        },
    };

    let requested_model = match payload.get("model").and_then(|m| m.as_str()) {
        Some(m) => m,
        None => return (StatusCode::BAD_REQUEST, "Missing required 'model' field").into_response(),
    };

    let candidates = app_state.resolve_candidates(requested_model);
    if candidates.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            format!("Invalid or unmapped virtual model: {}", requested_model),
        )
            .into_response();
    }

    let is_streaming = payload
        .get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);
    let (trace_id, parent_span_id) = extract_traceparent(&headers);

    let strategy = AdaptivePriorityStrategy;
    let mut attempts = 0;
    let start_time = Instant::now();

    let candidate_states: Vec<&ProviderState> = candidates.iter().map(|(p, _)| *p).collect();

    while let Some(selected) = strategy.select(&candidate_states).await {
        attempts += 1;
        let target_model = candidates
            .iter()
            .find(|(p, _)| p.name == selected.name)
            .map(|(_, m)| m.clone())
            .unwrap();

        payload["model"] = Value::String(target_model.clone());
        let rewritten_body = Bytes::from(serde_json::to_vec(&payload).unwrap());

        let endpoint_url = match selected.base_url.join("chat/completions") {
            Ok(url) => url,
            Err(e) => {
                error!("Invalid base URL path join for {}: {}", selected.name, e);
                continue;
            },
        };

        let mut req = app_state
            .http_client
            .post(endpoint_url.as_str())
            .body(rewritten_body)
            .timeout(Duration::from_secs(app_state.upstream_timeout_secs))
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", selected.api_key));

        if let Some(traceparent) = headers.get("traceparent") {
            req = req.header("traceparent", traceparent);
        }

        debug!(
            "Chat request routing to {} (attempt {})",
            selected.name, attempts
        );

        // Increment request counter
        if let Some(target) = app_state.providers.iter().find(|p| p.name == selected.name) {
            target
                .requests
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        let res = req.send().await;

        match res {
            Ok(res) if res.status().is_success() => {
                let status_code = res.status().as_u16();
                let upstream_headers = res.headers().clone();
                let target_provider_state = app_state
                    .providers
                    .iter()
                    .find(|p| p.name == selected.name)
                    .unwrap();

                // Report Success feedback
                strategy
                    .feedback(
                        target_provider_state,
                        true,
                        selected.is_probe,
                        Some(status_code),
                        None,
                    )
                    .await;

                // Increment local counters (streaming: token counts deferred)
                target_provider_state
                    .successes
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                if is_streaming {
                    // SSE chunk-streaming via bytes_stream() mapped to Boxed Error
                    let reqwest_stream = res.bytes_stream();
                    let axum_stream = reqwest_stream.map(|chunk_res| {
                        chunk_res
                            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
                    });

                    // Push Telemetry Metrics
                    // Token counts deferred — parsing requires buffering the full stream
                    telemetry.emit(TelemetryEvent::RecordTransaction {
                        operation: "chat".to_string(),
                        provider: selected.name.clone(),
                        model: target_model,
                        input_tokens: 0,
                        output_tokens: 0,
                        duration: start_time.elapsed(),
                        attempts,
                        failure_reason: None,
                        trace_id: trace_id.clone(),
                        parent_span_id: parent_span_id.clone(),
                    });

                    let mut response = Response::new(Body::from_stream(axum_stream));
                    *response.status_mut() = StatusCode::OK;
                    copy_response_headers(&upstream_headers, response.headers_mut());
                    return response.into_response();
                } else {
                    let res_body = match res.bytes().await {
                        Ok(b) => b,
                        Err(e) => {
                            warn!(
                                "Failed to read success response body from {}: {}",
                                selected.name, e
                            );
                            strategy
                                .feedback(
                                    target_provider_state,
                                    false,
                                    selected.is_probe,
                                    Some(status_code),
                                    None,
                                )
                                .await;
                            continue;
                        },
                    };

                    // Parse token counts from upstream response
                    let (input_tokens, output_tokens) = serde_json::from_slice::<Value>(&res_body)
                        .map(|v| {
                            let usage = v.get("usage");
                            let input = usage
                                .and_then(|u| u.get("prompt_tokens"))
                                .and_then(|t| t.as_u64())
                                .unwrap_or(0);
                            let output = usage
                                .and_then(|u| u.get("completion_tokens"))
                                .and_then(|t| t.as_u64())
                                .unwrap_or(0);
                            (input, output)
                        })
                        .unwrap_or((0, 0));

                    // Increment local counters
                    target_provider_state
                        .successes
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    target_provider_state
                        .tokens_input
                        .fetch_add(input_tokens, std::sync::atomic::Ordering::Relaxed);
                    target_provider_state
                        .tokens_output
                        .fetch_add(output_tokens, std::sync::atomic::Ordering::Relaxed);

                    // Push Telemetry Metrics
                    telemetry.emit(TelemetryEvent::RecordTransaction {
                        operation: "chat".to_string(),
                        provider: selected.name.clone(),
                        model: target_model,
                        input_tokens,
                        output_tokens,
                        duration: start_time.elapsed(),
                        attempts,
                        failure_reason: None,
                        trace_id: trace_id.clone(),
                        parent_span_id: parent_span_id.clone(),
                    });

                    let mut response = Response::new(Body::from(res_body));
                    *response.status_mut() = StatusCode::OK;
                    copy_response_headers(&upstream_headers, response.headers_mut());
                    return response.into_response();
                }
            },
            Ok(res) => {
                let status_code = res.status().as_u16();
                warn!(
                    "Chat completions upstream {} failed with status {}",
                    selected.name, status_code
                );
                let retry_after = extract_retry_after(res.headers());

                let target_provider_state = app_state
                    .providers
                    .iter()
                    .find(|p| p.name == selected.name)
                    .unwrap();
                strategy
                    .feedback(
                        target_provider_state,
                        false,
                        selected.is_probe,
                        Some(status_code),
                        retry_after,
                    )
                    .await;
            },
            Err(e) => {
                warn!(
                    "Chat completions upstream {} connection failed: {}",
                    selected.name, e
                );
                let target_provider_state = app_state
                    .providers
                    .iter()
                    .find(|p| p.name == selected.name)
                    .unwrap();
                strategy
                    .feedback(target_provider_state, false, selected.is_probe, None, None)
                    .await;
            },
        }
    }

    (
        StatusCode::BAD_GATEWAY,
        "All upstream chat completions providers failed or are rate-limited",
    )
        .into_response()
}

/// Extracts W3C traceparent segments: 00-{trace_id}-{span_id}-{flags}
fn extract_traceparent(headers: &HeaderMap) -> (Option<String>, Option<String>) {
    if let Some(val) = headers.get("traceparent").and_then(|v| v.to_str().ok()) {
        let segments: Vec<&str> = val.split('-').collect();
        if segments.len() >= 3 {
            return (Some(segments[1].to_string()), Some(segments[2].to_string()));
        }
    }
    (None, None)
}

/// Copies upstream headers safely to prevent cross-crate type conflicts (Gotcha #2)
fn copy_response_headers(src: &HeaderMap, dest: &mut HeaderMap) {
    for (key, value) in src.iter() {
        // Only copy standard metadata headers, ignore compression or transfer encodings
        let name_str = key.as_str();
        if name_str.starts_with("x-")
            || name_str == "content-type"
            || name_str == "cache-control"
            || name_str == "openai-version"
        {
            if let Ok(name) = HeaderName::from_bytes(name_str.as_bytes()) {
                if let Ok(val) = HeaderValue::from_bytes(value.as_bytes()) {
                    dest.insert(name, val);
                }
            }
        }
    }
}

/// Helper to parse Retry-After backoff duration
fn extract_retry_after(headers: &HeaderMap) -> Option<Duration> {
    if let Some(retry_after) = headers.get("retry-after").and_then(|h| h.to_str().ok()) {
        if let Ok(seconds) = retry_after.parse::<u64>() {
            return Some(Duration::from_secs(seconds));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Admin handlers — must be mounted behind localhost_only middleware
// ---------------------------------------------------------------------------

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
            // Reset circuit state
            *provider.circuit.write().await = CircuitState::Closed;
            // Reset consecutive failures
            *provider.consecutive_failures.write().await = 0;
            // Clear rate limit
            *provider.rate_limited_until.write().await = None;
            // Clear manual disabled
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
