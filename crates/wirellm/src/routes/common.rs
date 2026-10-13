use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use wirellm_core::state::{CircuitState, ProviderState, SelectedProvider};

/// Guard that automatically decrements active provider concurrency when dropped
pub struct InFlightGuard(pub Arc<AtomicU32>);

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Reads the current circuit and rate-limit state to produce a status code
/// for telemetry: 0 = Healthy, 1 = Cooldown, 2 = Tripped.
pub async fn circuit_status(provider: &ProviderState) -> u64 {
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
pub fn extract_traceparent(headers: &HeaderMap) -> (Option<String>, Option<String>) {
    if let Some(val) = headers.get("traceparent").and_then(|v| v.to_str().ok()) {
        let segments: Vec<&str> = val.split('-').collect();
        if segments.len() >= 3 {
            return (Some(segments[1].to_string()), Some(segments[2].to_string()));
        }
    }
    (None, None)
}

/// Copies upstream headers safely to prevent cross-crate type conflicts
pub fn copy_response_headers(src: &HeaderMap, dest: &mut HeaderMap) {
    for (key, value) in src.iter() {
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
pub fn extract_retry_after(headers: &HeaderMap) -> Option<Duration> {
    if let Some(retry_after) = headers.get("retry-after").and_then(|h| h.to_str().ok()) {
        if let Ok(seconds) = retry_after.parse::<u64>() {
            return Some(Duration::from_secs(seconds));
        }
    }
    None
}

/// Returns a structured JSON error response matching the OpenAI error format.
pub fn json_error_response(message: &str, error_type: &str, status: StatusCode) -> Response {
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

/// Attaches standard headers, client forwarded headers (e.g. x-* headers), and provider custom headers.
pub fn attach_request_headers(
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
