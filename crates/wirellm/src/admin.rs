use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use wirellm_core::config::VirtualModelTarget;
use wirellm_core::db::{DailyStats, ProviderSummary, VaultSecretRecord, VirtualModelDetail};
use wirellm_core::state::CircuitState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{info, warn};

use crate::ReloadableState;

#[derive(Serialize)]
pub struct ApiStatusResponse {
    pub uptime_secs: u64,
    pub total_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub providers: Vec<ProviderRuntimeDetail>,
    pub virtual_models: Vec<VirtualModelDetail>,
}

#[derive(Serialize)]
pub struct ProviderRuntimeDetail {
    pub name: String,
    pub enabled: bool,
    pub base_url: String,
    pub api_key_template: String,
    pub models: Vec<String>,
    pub circuit: String,
    pub failures: u32,
    pub rate_limited: bool,
    pub rate_limited_until_secs: Option<u64>,
    pub current_requests: u64,
    pub current_successes: u64,
    pub current_input_tokens: u64,
    pub current_output_tokens: u64,
    pub lifetime_requests: u64,
    pub lifetime_successes: u64,
    pub lifetime_input_tokens: u64,
    pub lifetime_output_tokens: u64,
    pub avg_latency_ms: f64,
    pub smartness_score: u32,
    pub max_rpm: Option<u32>,
    pub current_rpm: u32,
    pub is_rpm_capped: bool,
    pub min_interval_ms: Option<u64>,
    pub is_burst_limited: bool,
    pub in_flight: u32,
    pub recent_faults: u32,
    pub breathing_room_secs: u64,
    pub auto_disabled_reason: Option<String>,
    pub has_encrypted_key: bool,
    pub key_preview: String,
}

/// Normalizes provider headers and auto-populates OpenCode routing headers.
/// Providers pointing at OpenCode Zen / Go gateways get `x-opencode-session`
/// automatically so upstreams can route them efficiently. The value may be a
/// Vault/env template like `${OPENCODE_SESSION_ID}` and is expanded at request time.
pub fn normalize_provider_headers(base_url: &str, headers: &mut HashMap<String, String>) {
    let is_opencode_zen = base_url.contains("opencode.ai/zen");
    if is_opencode_zen
        && !headers.keys().any(|k| k.eq_ignore_ascii_case("x-opencode-session"))
    {
        headers.insert(
            "x-opencode-session".to_string(),
            "${OPENCODE_SESSION_ID}".to_string(),
        );
    }
}

#[derive(Deserialize)]
pub struct ProviderInput {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub base_url: String,
    pub api_key_template: String,
    pub models: Vec<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    pub rpm: Option<u32>,
    pub min_interval_ms: Option<u64>,
    pub max_concurrency: Option<u32>,
    pub smartness_score: Option<u32>,
    #[serde(default)]
    pub raw_api_key: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
pub struct VirtualModelInput {
    pub description: Option<String>,
    pub weight_smartness: Option<f64>,
    pub weight_speed: Option<f64>,
    pub weight_reliability: Option<f64>,
    pub targets: Vec<VirtualModelTarget>,
}

#[derive(Deserialize)]
pub struct AnalyticsQuery {
    pub days: Option<u32>,
    /// Inclusive UTC date lower bound ('YYYY-MM-DD'). Overrides `days` when set.
    pub from: Option<String>,
    /// Inclusive UTC date upper bound ('YYYY-MM-DD'). Overrides `days` when set.
    pub to: Option<String>,
}

#[derive(Serialize)]
pub struct AnalyticsResponse {
    pub days: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    pub stats: Vec<DailyStats>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_requests: u64,
    pub estimated_cost_saved_usd: f64,
}

#[derive(Serialize)]
pub struct TestConnectionResponse {
    pub success: bool,
    pub status_code: u16,
    pub latency_ms: u64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub async fn build_status_response(
    reloadable: &ReloadableState,
) -> Result<ApiStatusResponse, String> {
    let app_state = reloadable.app_state.borrow().clone();
    let uptime_secs = reloadable.start_time.elapsed().as_secs();
    let now = Instant::now();

    // Query DB provider summaries
    let db_summaries = reloadable
        .db
        .get_provider_summaries()
        .map_err(|e| format!("Database error: {}", e))?;

    // Header totals are computed from FULL daily_usage history (days=0),
    // so they match /api/analytics/daily exactly — per-provider summaries can
    // only count existing providers, which would silently drop the history of
    // deleted/retired provider circuits from the totals.
    let lifetime_totals = reloadable
        .db
        .get_daily_stats(0)
        .map_err(|e| format!("Database error: {}", e))?;
    let mut total_requests = 0u64;
    let mut total_input_tokens = 0u64;
    let mut total_output_tokens = 0u64;
    for s in &lifetime_totals {
        total_requests += s.total_requests;
        total_input_tokens += s.total_input_tokens;
        total_output_tokens += s.total_output_tokens;
    }

    let mut provider_details = Vec::new();

    for summary in db_summaries {
        let rt = app_state.providers.iter().find(|p| p.name == summary.name);

        let (
            circuit_str,
            failures,
            rate_limited,
            rate_limited_until_secs,
            cur_reqs,
            cur_succs,
            cur_in_tokens,
            cur_out_tokens,
            cur_rpm,
            is_rpm_capped,
            is_burst,
            in_fl,
            recent_faults,
            auto_disabled_reason,
        ) = if let Some(p) = rt {
            let circ = *p.circuit.read().await;
            let c_str = match circ {
                CircuitState::Closed => "Closed (Healthy)".to_string(),
                CircuitState::HalfOpen => "Half-Open (Probing)".to_string(),
                CircuitState::Open { until } => {
                    if now >= until {
                        "Half-Open (Ready to Probe)".to_string()
                    } else {
                        let left = until.saturating_duration_since(now).as_secs();
                        format!("Open (Cooldown: {}s left)", left)
                    }
                },
            };

            let rl = *p.rate_limited_until.read().await;
            let (is_lim, rl_secs) = match rl {
                Some(until) if now < until => (
                    true,
                    Some(until.saturating_duration_since(now).as_secs()),
                ),
                _ => (false, None),
            };

            let fails = *p.consecutive_failures.read().await;
            let reqs = p.requests.load(std::sync::atomic::Ordering::Relaxed);
            let succs = p.successes.load(std::sync::atomic::Ordering::Relaxed);
            let in_tok = p.tokens_input.load(std::sync::atomic::Ordering::Relaxed);
            let out_tok = p.tokens_output.load(std::sync::atomic::Ordering::Relaxed);

            let (rpm_val, capped) = {
                let timestamps = p.request_timestamps.read().await;
                let one_minute_ago = now.checked_sub(Duration::from_secs(60)).unwrap_or(now);
                let count = timestamps.iter().filter(|&&t| t >= one_minute_ago).count() as u32;
                let is_capped = p.max_rpm.map(|m| count >= m).unwrap_or(false);
                (count, is_capped)
            };

            let burst = if let Some(min_int) = p.min_interval_ms {
                let last = p.last_request_time.read().await;
                match *last {
                    Some(t) => now.saturating_duration_since(t) < Duration::from_millis(min_int),
                    None => false,
                }
            } else {
                false
            };

            let inflight = p.in_flight.load(std::sync::atomic::Ordering::Relaxed);

            (
                c_str,
                fails,
                is_lim,
                rl_secs,
                reqs,
                succs,
                in_tok,
                out_tok,
                rpm_val,
                capped,
                burst,
                inflight,
                p.fault_history.read().await.len() as u32,
                p.auto_disabled().await,
            )
        } else {
            (
                "Offline".to_string(),
                0,
                false,
                None,
                0,
                0,
                0,
                0,
                0,
                false,
                false,
                0,
                0,
                None,
            )
        };

        provider_details.push(ProviderRuntimeDetail {
            name: summary.name,
            enabled: summary.enabled,
            base_url: summary.base_url,
            api_key_template: summary.api_key_template,
            models: summary.models,
            circuit: circuit_str,
            failures,
            rate_limited,
            rate_limited_until_secs,
            current_requests: cur_reqs,
            current_successes: cur_succs,
            current_input_tokens: cur_in_tokens,
            current_output_tokens: cur_out_tokens,
            lifetime_requests: summary.lifetime_requests,
            lifetime_successes: summary.lifetime_successes,
            lifetime_input_tokens: summary.lifetime_input_tokens,
            lifetime_output_tokens: summary.lifetime_output_tokens,
            avg_latency_ms: summary.avg_latency_ms,
            smartness_score: summary.smartness_score,
            max_rpm: summary.rpm,
            current_rpm: cur_rpm,
            is_rpm_capped,
            min_interval_ms: summary.min_interval_ms,
            is_burst_limited: is_burst,
            in_flight: in_fl,
            recent_faults,
            breathing_room_secs: rate_limited_until_secs.unwrap_or(0),
            auto_disabled_reason,
            has_encrypted_key: summary.has_encrypted_key,
            key_preview: summary.key_preview,
        });
    }

    let virtual_models = reloadable
        .db
        .get_virtual_models_detail()
        .unwrap_or_default();

    Ok(ApiStatusResponse {
        uptime_secs,
        total_requests,
        total_input_tokens,
        total_output_tokens,
        providers: provider_details,
        virtual_models,
    })
}

/// GET /api/status - Comprehensive status overview with DB and runtime metrics
pub async fn get_api_status(
    State(reloadable): State<ReloadableState>,
) -> Result<Json<ApiStatusResponse>, (StatusCode, Json<serde_json::Value>)> {
    build_status_response(&reloadable)
        .await
        .map(Json)
        .map_err(|e| internal_error(e))
}

/// GET /api/providers - List all providers from DB
pub async fn list_providers(
    State(reloadable): State<ReloadableState>,
) -> Result<Json<Vec<ProviderSummary>>, (StatusCode, Json<serde_json::Value>)> {
    let summaries = reloadable
        .db
        .get_provider_summaries()
        .map_err(|e| internal_error(e.to_string()))?;
    Ok(Json(summaries))
}

/// POST /api/providers - Add a new provider and hot-reload
pub async fn create_provider(
    State(reloadable): State<ReloadableState>,
    Json(mut input): Json<ProviderInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    normalize_provider_headers(&input.base_url, &mut input.headers);
    reloadable
        .db
        .upsert_provider(
            &input.name,
            input.enabled,
            &input.base_url,
            &input.api_key_template,
            &input.models,
            &input.headers,
            input.rpm,
            input.min_interval_ms,
            input.max_concurrency,
            input.smartness_score,
            input.raw_api_key.as_deref(),
        )
        .map_err(|e| internal_error(format!("Failed to save provider: {}", e)))?;

    if let Err(e) = reloadable.reloader.reload() {
        warn!("Provider saved to DB but hot-reload returned: {}", e);
    }

    info!("Provider '{}' successfully created and saved to DB", input.name);
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "success": true,
            "message": format!("Provider '{}' created successfully", input.name),
            "name": input.name
        })),
    ))
}

/// PUT /api/providers/:name - Update an existing provider and hot-reload
pub async fn update_provider(
    State(reloadable): State<ReloadableState>,
    Path(name): Path<String>,
    Json(mut input): Json<ProviderInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    input.name = name.clone();
    normalize_provider_headers(&input.base_url, &mut input.headers);
    reloadable
        .db
        .upsert_provider(
            &input.name,
            input.enabled,
            &input.base_url,
            &input.api_key_template,
            &input.models,
            &input.headers,
            input.rpm,
            input.min_interval_ms,
            input.max_concurrency,
            input.smartness_score,
            input.raw_api_key.as_deref(),
        )
        .map_err(|e| internal_error(format!("Failed to update provider: {}", e)))?;

    if let Err(e) = reloadable.reloader.reload() {
        warn!("Provider updated in DB but hot-reload returned: {}", e);
    }

    info!("Provider '{}' successfully updated", name);
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Provider '{}' updated successfully", name),
        "name": name
    })))
}

/// DELETE /api/providers/:name - Delete a provider and hot-reload
pub async fn delete_provider(
    State(reloadable): State<ReloadableState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    reloadable
        .db
        .delete_provider(&name)
        .map_err(|e| internal_error(format!("Failed to delete provider: {}", e)))?;

    if let Err(e) = reloadable.reloader.reload() {
        warn!("Provider deleted from DB but hot-reload returned: {}", e);
    }

    info!("Provider '{}' deleted successfully", name);
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Provider '{}' deleted successfully", name)
    })))
}

/// POST /api/providers/:name/test - Ping provider with lightweight test request
pub async fn test_provider(
    State(reloadable): State<ReloadableState>,
    Path(name): Path<String>,
) -> Result<Json<TestConnectionResponse>, (StatusCode, Json<serde_json::Value>)> {
    let summaries = reloadable
        .db
        .get_provider_summaries()
        .map_err(|e| internal_error(e.to_string()))?;

    let provider = summaries
        .into_iter()
        .find(|p| p.name == name)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("Provider '{}' not found", name)})),
            )
        })?;

    // Load API key from DB
    let full_config = reloadable
        .db
        .load_config()
        .map_err(|e| internal_error(e.to_string()))?;

    let p_cfg = full_config
        .providers
        .into_iter()
        .find(|p| p.name == name)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Provider not configured in DB"})),
            )
        })?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| internal_error(e.to_string()))?;

    let start = Instant::now();
    let models_url = format!("{}/models", provider.base_url.trim_end_matches('/'));

    let mut req = client.get(&models_url);
    if !p_cfg.api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", p_cfg.api_key));
    }
    for (k, v) in &p_cfg.headers {
        req = req.header(k, v);
    }

    let result = req.send().await;
    let latency_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(res) => {
            let status = res.status().as_u16();
            if status >= 200 && status < 300 {
                Ok(Json(TestConnectionResponse {
                    success: true,
                    status_code: status,
                    latency_ms,
                    message: format!("Connection successful (HTTP {})", status),
                    error: None,
                }))
            } else if status == 404 && !provider.models.is_empty() {
                // Some providers do not support GET /models; try 1-token chat completion
                let chat_url = format!("{}/chat/completions", provider.base_url.trim_end_matches('/'));
                let model = &provider.models[0];
                let body = serde_json::json!({
                    "model": model,
                    "messages": [{"role": "user", "content": "ping"}],
                    "max_tokens": 1
                });
                let start2 = Instant::now();
                let mut req2 = client.post(&chat_url).json(&body);
                if !p_cfg.api_key.is_empty() {
                    req2 = req2.header("Authorization", format!("Bearer {}", p_cfg.api_key));
                }
                for (k, v) in &p_cfg.headers {
                    req2 = req2.header(k, v);
                }
                match req2.send().await {
                    Ok(chat_res) => {
                        let c_status = chat_res.status().as_u16();
                        let chat_latency = start2.elapsed().as_millis() as u64;
                        if c_status >= 200 && c_status < 300 {
                            Ok(Json(TestConnectionResponse {
                                success: true,
                                status_code: c_status,
                                latency_ms: chat_latency,
                                message: format!("Chat test successful (HTTP {})", c_status),
                                error: None,
                            }))
                        } else {
                            Ok(Json(TestConnectionResponse {
                                success: false,
                                status_code: c_status,
                                latency_ms: chat_latency,
                                message: format!("Chat test returned HTTP {}", c_status),
                                error: Some(format!("Upstream returned status code {}", c_status)),
                            }))
                        }
                    },
                    Err(e) => Ok(Json(TestConnectionResponse {
                        success: false,
                        status_code: 0,
                        latency_ms: start2.elapsed().as_millis() as u64,
                        message: "Connection failed".to_string(),
                        error: Some(e.to_string()),
                    })),
                }
            } else {
                Ok(Json(TestConnectionResponse {
                    success: false,
                    status_code: status,
                    latency_ms,
                    message: format!("HTTP {} returned from provider", status),
                    error: Some(format!("Status code {}", status)),
                }))
            }
        },
        Err(e) => Ok(Json(TestConnectionResponse {
            success: false,
            status_code: 0,
            latency_ms,
            message: "Connection failed".to_string(),
            error: Some(e.to_string()),
        })),
    }
}

/// GET /api/virtual-models - List virtual models
pub async fn list_virtual_models(
    State(reloadable): State<ReloadableState>,
) -> Result<Json<Vec<VirtualModelDetail>>, (StatusCode, Json<serde_json::Value>)> {
    let models = reloadable
        .db
        .get_virtual_models_detail()
        .map_err(|e| internal_error(e.to_string()))?;
    Ok(Json(models))
}

/// PUT /api/virtual-models/:name - Upsert virtual model targets and weights
pub async fn upsert_virtual_model(
    State(reloadable): State<ReloadableState>,
    Path(name): Path<String>,
    Json(input): Json<VirtualModelInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let desc = input.description.unwrap_or_default();
    let weights = match (
        input.weight_smartness,
        input.weight_speed,
        input.weight_reliability,
    ) {
        (Some(sm), Some(sp), Some(rel)) => Some((sm, sp, rel)),
        _ => None,
    };

    reloadable
        .db
        .upsert_virtual_model(&name, &desc, &input.targets, weights)
        .map_err(|e| internal_error(format!("Failed to save virtual model: {}", e)))?;

    if let Err(e) = reloadable.reloader.reload() {
        warn!("Virtual model saved but hot-reload returned: {}", e);
    }

    info!("Virtual model '{}' upserted successfully", name);
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Virtual model '{}' updated successfully", name),
        "name": name
    })))
}

/// DELETE /api/virtual-models/:name - Delete a virtual model
pub async fn delete_virtual_model(
    State(reloadable): State<ReloadableState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    reloadable
        .db
        .delete_virtual_model(&name)
        .map_err(|e| internal_error(format!("Failed to delete virtual model: {}", e)))?;

    if let Err(e) = reloadable.reloader.reload() {
        warn!("Virtual model deleted but hot-reload returned: {}", e);
    }

    info!("Virtual model '{}' deleted successfully", name);
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Virtual model '{}' deleted successfully", name)
    })))
}

/// GET /api/analytics/daily - Get daily token usage and cost savings
/// Range resolution: `from`/`to` (inclusive, 'YYYY-MM-DD') when provided;
/// otherwise `days` (0 = all-time, default 30) relative to today (UTC).
pub async fn get_daily_analytics(
    State(reloadable): State<ReloadableState>,
    Query(query): Query<AnalyticsQuery>,
) -> Result<Json<AnalyticsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let date_shape_ok = |v: &str| {
        v.len() == 10
            && v.as_bytes()[4] == b'-'
            && v.as_bytes()[7] == b'-'
            && v.split('-').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
    };
    for (label, v) in [("from", &query.from), ("to", &query.to)] {
        if let Some(v) = v {
            if !date_shape_ok(v) {
                return Err(bad_request(format!(
                    "Invalid '{}' date '{}': expected 'YYYY-MM-DD'",
                    label, v
                )));
            }
        }
    }

    let explicitly_ranged = query.from.is_some() || query.to.is_some();
    let from_reported = query.from.clone();
    let to_reported = query.to.clone();
    let (stats, days_reported) = if explicitly_ranged {
        let mut from = query.from;
        let mut to = query.to;
        // Keep the range sensible: 'to' >= 'from' (string compare == date compare)
        if let (Some(f), Some(t)) = (&from, &to) {
            if f > t {
                std::mem::swap(&mut from, &mut to);
            }
        }
        let stats = reloadable
            .db
            .get_daily_stats_range(from.clone(), to.clone())
            .map_err(|e| internal_error(e.to_string()))?;
        (stats, 0)
    } else {
        let days = query.days.unwrap_or(30); // days=0 -> all-time
        let stats = reloadable
            .db
            .get_daily_stats(days)
            .map_err(|e| internal_error(e.to_string()))?;
        (stats, days)
    };

    let mut total_input_tokens = 0u64;
    let mut total_output_tokens = 0u64;
    let mut total_requests = 0u64;

    for s in &stats {
        total_input_tokens += s.total_input_tokens;
        total_output_tokens += s.total_output_tokens;
        total_requests += s.total_requests;
    }

    let estimated_cost_saved_usd = 0.0;

    Ok(Json(AnalyticsResponse {
        days: days_reported,
        stats,
        total_input_tokens,
        total_output_tokens,
        total_requests,
        estimated_cost_saved_usd,
        from: from_reported,
        to: to_reported,
    }))
}

fn bad_request(msg: String) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "error": {
                "message": msg,
                "type": "invalid_request_error",
                "code": 400
            }
        })),
    )
}

fn internal_error(msg: String) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "error": {
                "message": msg,
                "type": "database_error",
                "code": 500
            }
        })),
    )
}

/// GET /api/setup/recommendations - Returns bundled recommendations markdown and metadata
pub async fn get_setup_recommendations() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "markdown": crate::mcp::RECOMMENDED_SETUP_MD,
    }))
}

#[derive(Deserialize, Default)]
pub struct RoutingPolicyPatch {
    pub stream_header_timeout_secs: Option<u64>,
    pub nonstream_header_timeout_secs: Option<u64>,
    pub fault_window_secs: Option<u64>,
    pub overload_base_cooldown_secs: Option<u64>,
    pub overload_max_cooldown_secs: Option<u64>,
    pub circuit_failure_threshold: Option<u32>,
    pub circuit_base_cooldown_secs: Option<u64>,
    pub circuit_max_cooldown_secs: Option<u64>,
    pub rate_limit_default_cooldown_secs: Option<u64>,
    pub decay_interval_secs: Option<u64>,
}

/// GET /api/routing-policy - Returns the active timeout & cooldown policy
pub async fn get_routing_policy() -> Json<wirellm_core::config::RoutingPolicy> {
    Json(wirellm_core::state::current_routing_policy())
}

/// POST/PUT /api/routing-policy - Updates and persists the timeout & cooldown policy
pub async fn update_routing_policy(
    State(reloadable): State<ReloadableState>,
    Json(patch): Json<RoutingPolicyPatch>,
) -> Result<Json<wirellm_core::config::RoutingPolicy>, (StatusCode, Json<serde_json::Value>)> {
    let mut policy = reloadable
        .db
        .get_routing_policy()
        .unwrap_or_else(|_| wirellm_core::state::current_routing_policy());

    if let Some(v) = patch.stream_header_timeout_secs {
        policy.stream_header_timeout_secs = v;
    }
    if let Some(v) = patch.nonstream_header_timeout_secs {
        policy.nonstream_header_timeout_secs = v;
    }
    if let Some(v) = patch.fault_window_secs {
        policy.fault_window_secs = v;
    }
    if let Some(v) = patch.overload_base_cooldown_secs {
        policy.overload_base_cooldown_secs = v;
    }
    if let Some(v) = patch.overload_max_cooldown_secs {
        policy.overload_max_cooldown_secs = v;
    }
    if let Some(v) = patch.circuit_failure_threshold {
        policy.circuit_failure_threshold = v;
    }
    if let Some(v) = patch.circuit_base_cooldown_secs {
        policy.circuit_base_cooldown_secs = v;
    }
    if let Some(v) = patch.circuit_max_cooldown_secs {
        policy.circuit_max_cooldown_secs = v;
    }
    if let Some(v) = patch.rate_limit_default_cooldown_secs {
        policy.rate_limit_default_cooldown_secs = v;
    }
    if let Some(v) = patch.decay_interval_secs {
        policy.decay_interval_secs = v;
    }
    // Enforce the shared safety floors (single source: RoutingPolicy::clamp)
    policy.clamp();

    reloadable
        .db
        .save_routing_policy(&policy)
        .map_err(|e| internal_error(e.to_string()))?;
    wirellm_core::state::apply_routing_policy(&policy);

    Ok(Json(wirellm_core::state::current_routing_policy()))
}

#[derive(Deserialize)]
pub struct CreateApiKeyInput {
    pub name: String,
    pub key: Option<String>,
}

/// GET /api/keys - List all registered API keys
pub async fn list_api_keys(
    State(reloadable): State<ReloadableState>,
) -> Result<Json<Vec<wirellm_core::db::ApiKeyRecord>>, (StatusCode, Json<serde_json::Value>)> {
    let keys = reloadable.db.list_api_keys().map_err(|e| internal_error(e.to_string()))?;
    Ok(Json(keys))
}

/// POST /api/keys - Create a new API key
pub async fn create_api_key(
    State(reloadable): State<ReloadableState>,
    Json(input): Json<CreateApiKeyInput>,
) -> Result<Json<wirellm_core::db::ApiKeyRecord>, (StatusCode, Json<serde_json::Value>)> {
    if input.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": {
                    "message": "Key name cannot be empty",
                    "type": "invalid_request_error",
                    "code": 400
                }
            })),
        ));
    }

    let record = reloadable
        .db
        .create_api_key(&input.name, input.key.as_deref())
        .map_err(|e| internal_error(e.to_string()))?;

    let _ = reloadable.reloader.reload();

    Ok(Json(record))
}

/// DELETE /api/keys/{id} - Delete/revoke an API key
pub async fn delete_api_key(
    State(reloadable): State<ReloadableState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    reloadable
        .db
        .delete_api_key(&id)
        .map_err(|e| internal_error(e.to_string()))?;

    let _ = reloadable.reloader.reload();

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("API key '{}' revoked successfully", id)
    })))
}

#[derive(Deserialize)]
pub struct VaultSecretInput {
    pub name: String,
    pub value: String,
    pub description: Option<String>,
}

/// GET /api/vault/secrets - Lists all central Vault secrets with masked previews
pub async fn list_vault_secrets(
    State(reloadable): State<ReloadableState>,
) -> Result<Json<Vec<VaultSecretRecord>>, (StatusCode, Json<serde_json::Value>)> {
    reloadable
        .db
        .list_vault_secrets()
        .map(Json)
        .map_err(|e| internal_error(e.to_string()))
}

/// POST /api/vault/secrets - Adds or updates an encrypted secret in the central Vault
pub async fn create_vault_secret(
    State(reloadable): State<ReloadableState>,
    Json(input): Json<VaultSecretInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if input.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Secret name cannot be empty"
            })),
        ));
    }
    if input.value.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Secret value cannot be empty"
            })),
        ));
    }

    let clean_name = input.name.trim().to_uppercase().replace('-', "_");
    reloadable
        .db
        .upsert_vault_secret(&clean_name, &input.value, input.description.as_deref().unwrap_or(""))
        .map_err(|e| internal_error(e.to_string()))?;

    let _ = reloadable.reloader.reload();

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Secret '{}' saved to central Vault and available for provider templates", clean_name),
        "name": clean_name
    })))
}

/// GET /api/vault/secrets/{name} - Retrieves a single Vault secret by name (masked)
pub async fn get_vault_secret(
    State(reloadable): State<ReloadableState>,
    Path(name): Path<String>,
) -> Result<Json<VaultSecretRecord>, (StatusCode, Json<serde_json::Value>)> {
    match reloadable.db.get_vault_secret_record(&name) {
        Ok(Some(rec)) => Ok(Json(rec)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("Secret '{}' not found in central Vault", name)
            })),
        )),
        Err(e) => Err(internal_error(e.to_string())),
    }
}

/// DELETE /api/vault/secrets/{name} - Deletes a secret from the central Vault
pub async fn delete_vault_secret(
    State(reloadable): State<ReloadableState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    reloadable
        .db
        .delete_vault_secret(&name)
        .map_err(|e| internal_error(e.to_string()))?;

    let _ = reloadable.reloader.reload();

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Secret '{}' removed from central Vault", name)
    })))
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct DiscoverModelsInput {
    pub provider: Option<String>,
    pub base_url: Option<String>,
    pub vault_secret: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiscoverModelsResult {
    pub provider: String,
    pub base_url: String,
    pub total_models: usize,
    pub models: Vec<String>,
    pub recommended_coding: Vec<String>,
    pub recommended_reasoning: Vec<String>,
    pub recommended_fast: Vec<String>,
}

/// Query upstream provider's /models endpoint to discover live available models
pub async fn discover_models_impl(
    db: &wirellm_core::db::DbManager,
    input: DiscoverModelsInput,
) -> Result<DiscoverModelsResult, String> {
    let (prov_name, base_url, api_key) = if let Some(ref p_name) = input.provider {
        let full_cfg = db.load_config().map_err(|e| e.to_string())?;
        let prov = full_cfg
            .providers
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(p_name))
            .ok_or_else(|| format!("Provider '{}' not found in database", p_name))?;
        (
            prov.name,
            prov.base_url,
            prov.api_key,
        )
    } else if let Some(ref url) = input.base_url {
        let key = if let Some(ref vs) = input.vault_secret {
            db.get_vault_secret_with_aliases(vs)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Vault secret '{}' not found", vs))?
        } else if let Some(ref raw) = input.api_key {
            raw.clone()
        } else {
            // Attempt automatic vault secret guessing from base_url
            let url_lower = url.to_lowercase();
            let guess_secret = if url_lower.contains("nvidia") {
                Some("NVIDIA_API_KEY")
            } else if url_lower.contains("groq") {
                Some("GROQ_API_KEY")
            } else if url_lower.contains("openrouter") {
                Some("OPENROUTER_TOKEN")
            } else if url_lower.contains("deepseek") {
                Some("DEEPSEEK_API_KEY")
            } else if url_lower.contains("cerebras") {
                Some("CEREBRAS_API_KEY")
            } else if url_lower.contains("generativelanguage") || url_lower.contains("google") {
                Some("GEMINI_API_KEY")
            } else {
                None
            };

            if let Some(s_name) = guess_secret {
                db.get_vault_secret_with_aliases(s_name)
                    .ok()
                    .flatten()
                    .unwrap_or_default()
            } else {
                String::new()
            }
        };
        ("custom".to_string(), url.clone(), key)
    } else {
        return Err("Must provide either 'provider' name or 'base_url'".to_string());
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|e| e.to_string())?;

    let clean_url = base_url.trim_end_matches('/');
    let target_models_url = format!("{}/models", clean_url);

    let mut req = client.get(&target_models_url);
    if !api_key.trim().is_empty() {
        req = req.header("Authorization", format!("Bearer {}", api_key.trim()));
    }

    let resp = req.send().await.map_err(|e| format!("Failed to connect to {}: {}", target_models_url, e))?;
    let status = resp.status();

    let json_val: serde_json::Value = if status.is_success() {
        resp.json().await.map_err(|e| format!("Failed to parse JSON response: {}", e))?
    } else if status.as_u16() == 404 && !clean_url.ends_with("/v1") {
        // Retry with /v1/models
        let v1_url = format!("{}/v1/models", clean_url);
        let mut req2 = client.get(&v1_url);
        if !api_key.trim().is_empty() {
            req2 = req2.header("Authorization", format!("Bearer {}", api_key.trim()));
        }
        let resp2 = req2.send().await.map_err(|e| format!("Failed to connect to {}: {}", v1_url, e))?;
        if resp2.status().is_success() {
            resp2.json().await.map_err(|e| format!("Failed to parse JSON response: {}", e))?
        } else {
            return Err(format!("Upstream returned HTTP {} when querying models at {}", resp2.status(), v1_url));
        }
    } else {
        return Err(format!("Upstream returned HTTP {} when querying models at {}", status, target_models_url));
    };

    // Extract models
    let mut extracted_models = Vec::new();
    if let Some(arr) = json_val.get("data").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                extracted_models.push(id.to_string());
            }
        }
    } else if let Some(arr) = json_val.get("models").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(id) = item.get("id").or_else(|| item.get("name")).and_then(|v| v.as_str()) {
                extracted_models.push(id.to_string());
            }
        }
    } else if let Some(arr) = json_val.as_array() {
        for item in arr {
            if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                extracted_models.push(id.to_string());
            } else if let Some(s) = item.as_str() {
                extracted_models.push(s.to_string());
            }
        }
    }

    extracted_models.sort();
    extracted_models.dedup();

    let mut recommended_coding = Vec::new();
    let mut recommended_reasoning = Vec::new();
    let mut recommended_fast = Vec::new();

    for m in &extracted_models {
        let m_lower = m.to_lowercase();
        if m_lower.contains("nemotron-3-ultra") || m_lower.contains("deepseek-r1") || m_lower.contains("reasoner") || m_lower.contains("o1") || m_lower.contains("o3") {
            recommended_reasoning.push(m.clone());
        }
        if m_lower.contains("coder") || m_lower.contains("code") || m_lower.contains("nemotron") || m_lower.contains("gpt-oss-120b") || m_lower.contains("deepseek-chat") || m_lower.contains("v3") || m_lower.contains("llama-3.3-70b") || m_lower.contains("qwen") {
            recommended_coding.push(m.clone());
        }
        if m_lower.contains("lightning") || m_lower.contains("flash") || m_lower.contains("8b") || m_lower.contains("20b") || m_lower.contains("mini") || m_lower.contains("turbo") {
            recommended_fast.push(m.clone());
        }
    }

    recommended_coding.truncate(8);
    recommended_reasoning.truncate(8);
    recommended_fast.truncate(8);

    Ok(DiscoverModelsResult {
        provider: prov_name,
        base_url,
        total_models: extracted_models.len(),
        models: extracted_models,
        recommended_coding,
        recommended_reasoning,
        recommended_fast,
    })
}

/// POST /api/providers/discover-models - Discovers models from an upstream provider or base_url
pub async fn discover_models_endpoint(
    State(reloadable): State<ReloadableState>,
    Json(input): Json<DiscoverModelsInput>,
) -> Result<Json<DiscoverModelsResult>, (StatusCode, Json<serde_json::Value>)> {
    discover_models_impl(&reloadable.db, input)
        .await
        .map(Json)
        .map_err(|e| internal_error(e))
}



