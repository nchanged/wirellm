use axum::{
    body::Body,
    extract::{Extension, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, warn};

use wirellm_core::router::{AdaptivePriorityStrategy, RoutingStrategy};
use wirellm_core::state::{AppState, ProviderState};
use wirellm_core::telemetry::{TelemetryClient, TelemetryEvent};

use super::common::{
    attach_request_headers, circuit_status, copy_response_headers, extract_retry_after,
    extract_traceparent, json_error_response,
};

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
