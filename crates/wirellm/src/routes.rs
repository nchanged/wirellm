use axum::{
    body::Body,
    extract::{Extension, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use futures_util::{stream::unfold, StreamExt};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, warn};

use wirellm_core::router::{AdaptivePriorityStrategy, RoutingStrategy};
use wirellm_core::state::{AppState, CircuitState, ProviderState, SelectedProvider};
use wirellm_core::telemetry::{TelemetryClient, TelemetryEvent};

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
                owned_by: "wirellm-virtual",
            });
        }
    }

    // Also include direct models from healthy providers
    let mut seen_models: std::collections::HashSet<String> = data.iter().map(|m| m.id.clone()).collect();
    for provider in &app_state.providers {
        if provider.manual_disabled.load(std::sync::atomic::Ordering::Relaxed) {
            continue;
        }
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
            for model in &provider.models {
                if seen_models.insert(model.clone()) {
                    data.push(ModelObject {
                        id: model.clone(),
                        object: "model",
                        created: 1717070400,
                        owned_by: "wirellm-direct",
                    });
                }
            }
        }
    }

    Json(ModelsResponse {
        object: "list",
        data,
    })
}

struct InFlightGuard(Arc<std::sync::atomic::AtomicU32>);
impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[derive(Serialize)]
struct RouteEntry {
    provider: String,
    model: String,
    circuit: String,
    requests: u64,
    successes: u64,
    max_rpm: Option<u32>,
    current_rpm: u32,
    is_rpm_capped: bool,
    min_interval_ms: Option<u64>,
    is_burst_limited: bool,
    rate_limited_until_secs: Option<u64>,
    in_flight: u32,
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
        rate_limited_until_secs: Option<u64>,
        requests: u64,
        successes: u64,
        tokens_input: u64,
        tokens_output: u64,
        last_request: String,
        max_rpm: Option<u32>,
        current_rpm: u32,
        is_rpm_capped: bool,
        min_interval_ms: Option<u64>,
        is_burst_limited: bool,
        in_flight: u32,
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
                    } else if elapsed.as_secs() < 86400 {
                        format!("{}h ago", elapsed.as_secs() / 3600)
                    } else {
                        format!("{}d ago", elapsed.as_secs() / 86400)
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

/// Attaches standard headers, client forwarded headers (e.g. x-* headers), and provider custom headers.
fn attach_request_headers(
    mut req: reqwest::RequestBuilder,
    selected: &SelectedProvider,
    client_headers: &HeaderMap,
) -> reqwest::RequestBuilder {
    let has_custom_content_type = selected
        .headers
        .keys()
        .any(|k| k.eq_ignore_ascii_case("content-type"));
    if !has_custom_content_type {
        req = req.header("Content-Type", "application/json");
    }

    let has_custom_auth = selected
        .headers
        .keys()
        .any(|k| k.eq_ignore_ascii_case("authorization"));
    if !has_custom_auth && !selected.api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", selected.api_key));
    }

    // Propagate W3C tracing header if present
    if let Some(traceparent) = client_headers.get("traceparent") {
        req = req.header("traceparent", traceparent);
    }

    // Forward incoming client headers starting with 'x-' if not overridden by provider headers
    for (header_name, header_val) in client_headers {
        let name_str = header_name.as_str();
        if name_str.starts_with("x-")
            && !selected
                .headers
                .keys()
                .any(|k| k.eq_ignore_ascii_case(name_str))
        {
            if let Ok(val_str) = header_val.to_str() {
                req = req.header(name_str, val_str);
            }
        }
    }

    // Apply provider custom headers (overriding client headers or default Authorization if specified)
    for (k, v) in &selected.headers {
        req = req.header(k.as_str(), v.as_str());
    }

    req
}

/// POST /v1/embeddings
pub async fn create_embeddings(
    State((app_state, telemetry)): State<(Arc<AppState>, TelemetryClient)>,
    Extension(request_id): Extension<String>,
    headers: HeaderMap,
    body: Bytes, // Bounded ref-counted bytes for zero-cost routing retries
) -> impl IntoResponse {
    let mut payload: Value = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return json_error_response(
                &format!("Invalid JSON payload: {}", e),
                "invalid_request_error",
                StatusCode::BAD_REQUEST,
            )
        },
    };

    let requested_model = match payload.get("model").and_then(|m| m.as_str()) {
        Some(m) => m,
        None => {
            return json_error_response(
                "Missing required 'model' field",
                "invalid_request_error",
                StatusCode::BAD_REQUEST,
            )
        },
    };

    let candidates = app_state.resolve_candidates(requested_model);
    if candidates.is_empty() {
        return json_error_response(
            &format!("Invalid or unmapped virtual model: {}", requested_model),
            "invalid_request_error",
            StatusCode::BAD_REQUEST,
        );
    }

    // Extract W3C traceparent headers if present for tracing
    let (trace_id, parent_span_id) = extract_traceparent(&headers);

    let strategy = AdaptivePriorityStrategy;
    let mut attempts = 0;
    let start_time = Instant::now();
    let mut last_upstream_error = String::new();
    let mut last_failed_provider = String::new();
    let mut last_failed_status: u16 = 0;

    // Map candidate provider references for select strategy
    let candidate_states: Vec<&ProviderState> = candidates.iter().map(|(p, _)| *p).collect();

    while let Some(selected) = strategy.select(&candidate_states).await {
        attempts += 1;
        let target_model = match candidates
            .iter()
            .find(|(p, _)| p.name == selected.name)
            .map(|(_, m)| m.clone())
        {
            Some(m) => m,
            None => {
                warn!(
                    request_id = %request_id,
                    "Provider {} removed from candidates during embeddings routing",
                    selected.name
                );
                continue;
            },
        };

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
        let req = app_state
            .http_client
            .post(endpoint_url.as_str())
            .body(rewritten_body)
            .timeout(Duration::from_secs(app_state.upstream_timeout_secs));
        let req = attach_request_headers(req, &selected, &headers);

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
                            request_id = %request_id,
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
                        telemetry.emit(TelemetryEvent::UpdateStatus {
                            provider: selected.name.clone(),
                            status: circuit_status(target_provider_state).await,
                        });
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
                telemetry.emit(TelemetryEvent::UpdateStatus {
                    provider: selected.name.clone(),
                    status: circuit_status(target_provider_state).await,
                });

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
                    request_id: request_id.clone(),
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
                    request_id = %request_id,
                    "Embedding request upstream {} failed with status {}",
                    selected.name, status_code
                );

                // Extract Retry-After before consuming the response body
                let retry_after = extract_retry_after(res.headers());

                // Best-effort: read upstream error body for final 502 response
                let error_body = match res.bytes().await {
                    Ok(b) => String::from_utf8_lossy(&b).to_string(),
                    Err(e) => {
                        warn!(
                            request_id = %request_id,
                            "Failed to read error body from {}: {}", selected.name, e
                        );
                        String::new()
                    },
                };
                let parsed_error = if !error_body.is_empty() {
                    serde_json::from_str::<Value>(&error_body)
                        .ok()
                        .and_then(|v| {
                            v.get("error")
                                .and_then(|e| e.get("message"))
                                .and_then(|m| m.as_str().map(String::from))
                        })
                        .unwrap_or(error_body)
                } else {
                    String::new()
                };
                last_upstream_error = parsed_error;
                last_failed_provider = selected.name.clone();
                last_failed_status = status_code;

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
                telemetry.emit(TelemetryEvent::UpdateStatus {
                    provider: selected.name.clone(),
                    status: circuit_status(target_provider_state).await,
                });
            },
            Err(e) => {
                warn!(
                    request_id = %request_id,
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
                telemetry.emit(TelemetryEvent::UpdateStatus {
                    provider: selected.name.clone(),
                    status: circuit_status(target_provider_state).await,
                });
            },
        }
    }

    let message = if !last_upstream_error.is_empty() {
        format!(
            "All upstream embeddings providers failed or are rate-limited. Last error from {} ({}): {}",
            last_failed_provider, last_failed_status, last_upstream_error
        )
    } else {
        "All upstream embeddings providers failed or are rate-limited".to_string()
    };
    json_error_response(&message, "server_error", StatusCode::BAD_GATEWAY)
}

fn estimate_prompt_tokens(payload: &Value) -> u64 {
    let mut total_chars = 0usize;
    if let Some(messages) = payload.get("messages").and_then(|m| m.as_array()) {
        for msg in messages {
            if let Some(content) = msg.get("content") {
                if let Some(s) = content.as_str() {
                    total_chars += s.len();
                } else if let Some(arr) = content.as_array() {
                    for part in arr {
                        if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                            total_chars += t.len();
                        }
                    }
                }
            }
            if let Some(role) = msg.get("role").and_then(|r| r.as_str()) {
                total_chars += role.len() + 4;
            }
        }
    }
    if let Some(tools) = payload.get("tools").and_then(|t| t.as_array()) {
        for tool in tools {
            total_chars += tool.to_string().len();
        }
    }

    if total_chars > 0 {
        ((total_chars + 3) / 4).max(1) as u64
    } else {
        0
    }
}

/// POST /v1/chat/completions
pub async fn create_chat_completions(
    State((app_state, telemetry)): State<(Arc<AppState>, TelemetryClient)>,
    Extension(request_id): Extension<String>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let mut payload: Value = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return json_error_response(
                &format!("Invalid JSON payload: {}", e),
                "invalid_request_error",
                StatusCode::BAD_REQUEST,
            )
        },
    };

    let requested_model = match payload.get("model").and_then(|m| m.as_str()) {
        Some(m) => m,
        None => {
            return json_error_response(
                "Missing required 'model' field",
                "invalid_request_error",
                StatusCode::BAD_REQUEST,
            )
        },
    };

    let candidates = app_state.resolve_candidates(requested_model);
    if candidates.is_empty() {
        return json_error_response(
            &format!("Invalid or unmapped virtual model: {}", requested_model),
            "invalid_request_error",
            StatusCode::BAD_REQUEST,
        );
    }

    let estimated_prompt_tokens = estimate_prompt_tokens(&payload);
    let is_streaming = payload
        .get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);
    if is_streaming && payload.get("stream_options").is_none() {
        payload["stream_options"] = serde_json::json!({
            "include_usage": true
        });
    }
    let (trace_id, parent_span_id) = extract_traceparent(&headers);

    let strategy = AdaptivePriorityStrategy;
    let mut attempts = 0;
    let start_time = Instant::now();
    let mut last_upstream_error = String::new();
    let mut last_failed_provider = String::new();
    let mut last_failed_status: u16 = 0;

    let candidate_states: Vec<&ProviderState> = candidates.iter().map(|(p, _)| *p).collect();

    while let Some(selected) = strategy.select(&candidate_states).await {
        attempts += 1;
        let target_model = match candidates
            .iter()
            .find(|(p, _)| p.name == selected.name)
            .map(|(_, m)| m.clone())
        {
            Some(m) => m,
            None => {
                warn!(
                    request_id = %request_id,
                    "Provider {} removed from candidates during chat routing",
                    selected.name
                );
                continue;
            },
        };

        payload["model"] = Value::String(target_model.clone());
        let rewritten_body = Bytes::from(serde_json::to_vec(&payload).unwrap());

        let endpoint_url = match selected.base_url.join("chat/completions") {
            Ok(url) => url,
            Err(e) => {
                error!("Invalid base URL path join for {}: {}", selected.name, e);
                continue;
            },
        };

        let req = app_state
            .http_client
            .post(endpoint_url.as_str())
            .body(rewritten_body)
            .timeout(Duration::from_secs(app_state.upstream_timeout_secs));
        let req = attach_request_headers(req, &selected, &headers);

        debug!(
            "Chat request routing to {} (attempt {})",
            selected.name, attempts
        );

        // Increment request counter and active in_flight concurrency
        let _flight_guard = if let Some(target) = app_state.providers.iter().find(|p| p.name == selected.name) {
            target
                .requests
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            target
                .in_flight
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Some(InFlightGuard(target.in_flight.clone()))
        } else {
            None
        };

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

                if is_streaming {
                    let app_state_clone = app_state.clone();
                    let provider_name = selected.name.clone();
                    let is_probe = selected.is_probe;
                    let telemetry_clone = telemetry.clone();
                    let trace_id_clone = trace_id.clone();
                    let parent_span_id_clone = parent_span_id.clone();
                    let request_id_clone = request_id.clone();
                    let stream_start_time = start_time;
                    let attempt_count = attempts;
                    let model_name = target_model;
                    let est_prompt = estimated_prompt_tokens;

                    let (tx, rx) = tokio::sync::mpsc::channel::<
                        Result<Bytes, Box<dyn std::error::Error + Send + Sync>>,
                    >(32);

                    tokio::spawn(async move {
                        let mut reqwest_stream = res.bytes_stream();
                        let mut stream_success = true;
                        let mut exact_input_tokens: Option<u64> = None;
                        let mut exact_output_tokens: Option<u64> = None;
                        let mut accumulated_output_chars: usize = 0;
                        let mut sse_buffer = String::new();

                        while let Some(chunk_result) = reqwest_stream.next().await {
                            match chunk_result {
                                Ok(chunk) => {
                                    if let Ok(text) = std::str::from_utf8(&chunk) {
                                        sse_buffer.push_str(text);
                                        while let Some(pos) = sse_buffer.find('\n') {
                                            let line = sse_buffer[..pos].trim();
                                            if let Some(data) = line.strip_prefix("data:") {
                                                let data = data.trim();
                                                if data != "[DONE]" && !data.is_empty() {
                                                    if let Ok(val) = serde_json::from_str::<Value>(data) {
                                                        if let Some(usage) = val.get("usage") {
                                                            if let Some(pt) = usage.get("prompt_tokens").and_then(|t| t.as_u64()) {
                                                                exact_input_tokens = Some(pt);
                                                            }
                                                            if let Some(ct) = usage.get("completion_tokens").and_then(|t| t.as_u64()) {
                                                                exact_output_tokens = Some(ct);
                                                            }
                                                        }
                                                        if let Some(choices) = val.get("choices").and_then(|c| c.as_array()) {
                                                            for choice in choices {
                                                                if let Some(delta) = choice.get("delta") {
                                                                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                                                        accumulated_output_chars += content.len();
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            sse_buffer.drain(..=pos);
                                        }
                                    }

                                    if tx.send(Ok(chunk)).await.is_err() {
                                        // Client disconnected
                                        stream_success = false;
                                        break;
                                    }
                                },
                                Err(e) => {
                                    stream_success = false;
                                    let _ = tx
                                        .send(Err(
                                            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
                                        ))
                                        .await;
                                    break;
                                },
                            }
                        }

                        let final_input_tokens = exact_input_tokens.unwrap_or(est_prompt);
                        let final_output_tokens = exact_output_tokens.unwrap_or_else(|| {
                            if accumulated_output_chars > 0 {
                                ((accumulated_output_chars + 3) / 4).max(1) as u64
                            } else {
                                0
                            }
                        });

                        // Deferred circuit-breaker feedback after stream completes
                        if let Some(provider_state) = app_state_clone
                            .providers
                            .iter()
                            .find(|p| p.name == provider_name)
                        {
                            let strategy = AdaptivePriorityStrategy;
                            strategy
                                .feedback(
                                    provider_state,
                                    stream_success,
                                    is_probe,
                                    Some(status_code),
                                    None,
                                )
                                .await;
                            telemetry_clone.emit(TelemetryEvent::UpdateStatus {
                                provider: provider_name.clone(),
                                status: circuit_status(provider_state).await,
                            });

                            if stream_success {
                                provider_state
                                    .successes
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                provider_state
                                    .tokens_input
                                    .fetch_add(final_input_tokens, std::sync::atomic::Ordering::Relaxed);
                                provider_state
                                    .tokens_output
                                    .fetch_add(final_output_tokens, std::sync::atomic::Ordering::Relaxed);
                            }
                        }

                        // Push Telemetry Metrics
                        telemetry_clone.emit(TelemetryEvent::RecordTransaction {
                            operation: "chat".to_string(),
                            provider: provider_name,
                            model: model_name,
                            input_tokens: final_input_tokens,
                            output_tokens: final_output_tokens,
                            duration: stream_start_time.elapsed(),
                            attempts: attempt_count,
                            failure_reason: if stream_success {
                                None
                            } else {
                                Some("stream_failed".to_string())
                            },
                            trace_id: trace_id_clone,
                            parent_span_id: parent_span_id_clone,
                            request_id: request_id_clone,
                        });
                    });

                    // Convert mpsc receiver to a Stream for axum
                    let axum_stream = unfold(Some(rx), |state| async move {
                        let mut rx = state?;
                        rx.recv().await.map(|item| (item, Some(rx)))
                    });

                    let mut response = Response::new(Body::from_stream(axum_stream));
                    *response.status_mut() = StatusCode::OK;
                    copy_response_headers(&upstream_headers, response.headers_mut());
                    return response.into_response();
                } else {
                    // Report Success feedback (non-streaming: immediate, as before)
                    strategy
                        .feedback(
                            target_provider_state,
                            true,
                            selected.is_probe,
                            Some(status_code),
                            None,
                        )
                        .await;
                    telemetry.emit(TelemetryEvent::UpdateStatus {
                        provider: selected.name.clone(),
                        status: circuit_status(target_provider_state).await,
                    });

                    let res_body = match res.bytes().await {
                        Ok(b) => b,
                        Err(e) => {
                            warn!(
                                request_id = %request_id,
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
                            telemetry.emit(TelemetryEvent::UpdateStatus {
                                provider: selected.name.clone(),
                                status: circuit_status(target_provider_state).await,
                            });
                            continue;
                        },
                    };

                    // Parse token counts from upstream response
                    let (mut input_tokens, output_tokens) = serde_json::from_slice::<Value>(&res_body)
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

                    if input_tokens == 0 && estimated_prompt_tokens > 0 {
                        input_tokens = estimated_prompt_tokens;
                    }

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
                        request_id: request_id.clone(),
                    });

                    let mut response = Response::new(Body::from(res_body));
                    *response.status_mut() = StatusCode::OK;
                    copy_response_headers(&upstream_headers, response.headers_mut());
                    return response.into_response();
                }
            },
            Ok(res) => {
                let status_code = res.status().as_u16();

                // Extract Retry-After before consuming the response body
                let retry_after = extract_retry_after(res.headers());

                // Best-effort: read upstream error body for final 502 response
                let error_body = match res.bytes().await {
                    Ok(b) => String::from_utf8_lossy(&b).to_string(),
                    Err(e) => {
                        warn!(
                            request_id = %request_id,
                            "Failed to read error body from {}: {}", selected.name, e
                        );
                        String::new()
                    },
                };
                let parsed_error = if !error_body.is_empty() {
                    serde_json::from_str::<Value>(&error_body)
                        .ok()
                        .and_then(|v| {
                            v.get("error")
                                .and_then(|e| e.get("message"))
                                .and_then(|m| m.as_str().map(String::from))
                        })
                        .unwrap_or(error_body)
                } else {
                    String::new()
                };

                warn!(
                    request_id = %request_id,
                    "Chat completions upstream {} failed with status {}{}",
                    selected.name,
                    status_code,
                    if !parsed_error.is_empty() {
                        format!(": {}", parsed_error)
                    } else {
                        String::new()
                    }
                );

                last_upstream_error = parsed_error;
                last_failed_provider = selected.name.clone();
                last_failed_status = status_code;

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
                telemetry.emit(TelemetryEvent::UpdateStatus {
                    provider: selected.name.clone(),
                    status: circuit_status(target_provider_state).await,
                });
            },
            Err(e) => {
                warn!(
                    request_id = %request_id,
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
                telemetry.emit(TelemetryEvent::UpdateStatus {
                    provider: selected.name.clone(),
                    status: circuit_status(target_provider_state).await,
                });
            },
        }
    }

    let message = if !last_upstream_error.is_empty() {
        format!(
            "All upstream chat completions providers failed or are rate-limited. Last error from {} ({}): {}",
            last_failed_provider, last_failed_status, last_upstream_error
        )
    } else {
        "All upstream chat completions providers failed or are rate-limited".to_string()
    };
    json_error_response(&message, "server_error", StatusCode::BAD_GATEWAY)
}

/// Reads the current circuit and rate-limit state to produce a status code
/// for telemetry: 0 = Healthy, 1 = Cooldown, 2 = Tripped.
async fn circuit_status(provider: &ProviderState) -> u64 {
    let circuit = *provider.circuit.read().await;
    let now = Instant::now();
    match circuit {
        CircuitState::Closed => {
            let rl = *provider.rate_limited_until.read().await;
            if rl.is_some() && now < rl.unwrap() {
                1 // Cooldown (rate-limited)
            } else {
                0 // Healthy
            }
        },
        CircuitState::HalfOpen => 1, // Cooldown (probing)
        CircuitState::Open { until } => {
            if now < until {
                2 // Tripped
            } else {
                1 // Cooldown expired, about to probe
            }
        },
    }
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

/// Returns a structured JSON error response matching the OpenAI error format.
fn json_error_response(message: &str, error_type: &str, status: StatusCode) -> Response {
    let body = serde_json::json!({
        "error": {
            "message": message,
            "type": error_type,
            "code": status.as_u16()
        }
    });
    let bytes = match serde_json::to_vec(&body) {
        Ok(b) => b,
        Err(_) => return (status, message.to_string()).into_response(),
    };
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
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

/// GET /ui or GET /dashboard
#[allow(dead_code)]
pub async fn ui_dashboard() -> impl IntoResponse {
    axum::response::Html(include_str!("ui.html"))
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
                "/tmp/oxllm.log".to_string()
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


