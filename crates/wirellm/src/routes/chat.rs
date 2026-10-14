use axum::{
    body::Body,
    extract::{Extension, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use futures_util::{stream::unfold, StreamExt};
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, warn};
use reqwest::Url;

use wirellm_core::router::{AdaptivePriorityStrategy, RoutingStrategy};
use wirellm_core::state::{AppState, ProviderState};
use wirellm_core::telemetry::{TelemetryClient, TelemetryEvent};

use super::common::{
    attach_request_headers, circuit_status, copy_response_headers, extract_retry_after,
    extract_traceparent, json_error_response, InFlightGuard,
};

pub fn estimate_prompt_tokens(payload: &Value) -> u64 {
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
    State((app_state, telemetry, live_tx)): State<(
        Arc<AppState>,
        TelemetryClient,
        tokio::sync::broadcast::Sender<crate::server::LiveStreamEvent>,
    )>,
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

    let requested_model = match payload.get("model").and_then(|m| m.as_str()).map(ToString::to_string) {
        Some(m) => m,
        None => {
            return json_error_response(
                "Missing required 'model' field",
                "invalid_request_error",
                StatusCode::BAD_REQUEST,
            )
        },
    };

    let candidates = app_state.resolve_candidates(&requested_model);
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

        let base_str = selected.base_url.as_str().trim_end_matches('/');
        let endpoint_url = match Url::parse(&format!("{}/chat/completions", base_str)) {
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
                    let live_tx_clone = live_tx.clone();
                    let requested_model_clone = requested_model.to_string();

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
                                                                let mut chunk_text = String::new();

                                                                if let Some(delta) = choice.get("delta") {
                                                                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                                                        chunk_text.push_str(content);
                                                                    } else if let Some(reasoning) = delta.get("reasoning_content").and_then(|c| c.as_str()) {
                                                                        chunk_text.push_str(reasoning);
                                                                    }

                                                                    if let Some(tool_calls) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
                                                                        for tc in tool_calls {
                                                                            if let Some(func) = tc.get("function") {
                                                                                if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                                                                                    if !name.is_empty() {
                                                                                        if !chunk_text.is_empty() {
                                                                                            chunk_text.push('\n');
                                                                                        }
                                                                                        chunk_text.push_str(&format!("\n🔧 call: {}(", name));
                                                                                    }
                                                                                }
                                                                                if let Some(args) = func.get("arguments").and_then(|a| a.as_str()) {
                                                                                    chunk_text.push_str(args);
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }

                                                                if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                                                                    if fr == "tool_calls" || fr == "function_call" {
                                                                        chunk_text.push(')');
                                                                    }
                                                                }

                                                                if !chunk_text.is_empty() {
                                                                    accumulated_output_chars += chunk_text.len();
                                                                    if live_tx_clone.receiver_count() > 0 {
                                                                        let _ = live_tx_clone.send(crate::server::LiveStreamEvent {
                                                                            request_id: request_id_clone.clone(),
                                                                            virtual_model: Some(requested_model_clone.clone()),
                                                                            provider: provider_name.clone(),
                                                                            model: model_name.clone(),
                                                                            delta: chunk_text,
                                                                            is_done: false,
                                                                            error: None,
                                                                            tokens_so_far: (accumulated_output_chars + 3) / 4,
                                                                            elapsed_ms: stream_start_time.elapsed().as_millis() as u64,
                                                                        });
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

                        if live_tx_clone.receiver_count() > 0 {
                            let _ = live_tx_clone.send(crate::server::LiveStreamEvent {
                                request_id: request_id_clone.clone(),
                                virtual_model: Some(requested_model_clone.clone()),
                                provider: provider_name.clone(),
                                model: model_name.clone(),
                                delta: String::new(),
                                is_done: true,
                                error: if stream_success {
                                    None
                                } else {
                                    Some("stream_interrupted".to_string())
                                },
                                tokens_so_far: final_output_tokens as usize,
                                elapsed_ms: stream_start_time.elapsed().as_millis() as u64,
                            });
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
                        model: target_model.clone(),
                        input_tokens,
                        output_tokens,
                        duration: start_time.elapsed(),
                        attempts,
                        failure_reason: None,
                        trace_id: trace_id.clone(),
                        parent_span_id: parent_span_id.clone(),
                        request_id: request_id.clone(),
                    });

                    if live_tx.receiver_count() > 0 {
                        let preview_text = serde_json::from_slice::<Value>(&res_body)
                            .ok()
                            .and_then(|v| {
                                v.get("choices")
                                    .and_then(|c| c.as_array())
                                    .and_then(|a| a.first())
                                    .and_then(|c| c.get("message"))
                                    .map(|m| {
                                        let mut text = m.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
                                        if let Some(tool_calls) = m.get("tool_calls").and_then(|tc| tc.as_array()) {
                                            for tc in tool_calls {
                                                if let Some(f) = tc.get("function") {
                                                    let name = f.get("name").and_then(|n| n.as_str()).unwrap_or("tool");
                                                    let args = f.get("arguments").and_then(|a| a.as_str()).unwrap_or("");
                                                    if !text.is_empty() {
                                                        text.push('\n');
                                                    }
                                                    text.push_str(&format!("🔧 call: {}({})", name, args));
                                                }
                                            }
                                        }
                                        text
                                    })
                            })
                            .unwrap_or_default();
                        let _ = live_tx.send(crate::server::LiveStreamEvent {
                            request_id: request_id.clone(),
                            virtual_model: Some(requested_model.to_string()),
                            provider: selected.name.clone(),
                            model: target_model,
                            delta: preview_text,
                            is_done: true,
                            error: None,
                            tokens_so_far: output_tokens as usize,
                            elapsed_ms: start_time.elapsed().as_millis() as u64,
                        });
                    }

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
                telemetry.emit(TelemetryEvent::RecordTransaction {
                    operation: "chat".to_string(),
                    provider: selected.name.clone(),
                    model: target_model.clone(),
                    input_tokens: estimated_prompt_tokens,
                    output_tokens: 0,
                    duration: start_time.elapsed(),
                    attempts,
                    failure_reason: Some(format!("HTTP {}", status_code)),
                    trace_id: trace_id.clone(),
                    parent_span_id: parent_span_id.clone(),
                    request_id: request_id.clone(),
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
                telemetry.emit(TelemetryEvent::RecordTransaction {
                    operation: "chat".to_string(),
                    provider: selected.name.clone(),
                    model: target_model.clone(),
                    input_tokens: estimated_prompt_tokens,
                    output_tokens: 0,
                    duration: start_time.elapsed(),
                    attempts,
                    failure_reason: Some(e.to_string()),
                    trace_id: trace_id.clone(),
                    parent_span_id: parent_span_id.clone(),
                    request_id: request_id.clone(),
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
