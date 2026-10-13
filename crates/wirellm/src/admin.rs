use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use wirellm_core::config::VirtualModelTarget;
use wirellm_core::db::{DailyStats, ProviderSummary, VirtualModelDetail};
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
    pub has_encrypted_key: bool,
    pub key_preview: String,
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
}

#[derive(Serialize)]
pub struct AnalyticsResponse {
    pub days: u32,
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

    let mut provider_details = Vec::new();
    let mut total_requests = 0u64;
    let mut total_input_tokens = 0u64;
    let mut total_output_tokens = 0u64;

    for summary in db_summaries {
        total_requests += summary.lifetime_requests;
        total_input_tokens += summary.lifetime_input_tokens;
        total_output_tokens += summary.lifetime_output_tokens;

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
        ) = if let Some(p) = rt {
            let circ = *p.circuit.read().await;
            let c_str = match circ {
                CircuitState::Closed => "Closed (Healthy)".to_string(),
                CircuitState::HalfOpen => "Half-Open (Probing)".to_string(),
                CircuitState::Open { until } => {
                    let left = until.saturating_duration_since(now).as_secs();
                    format!("Open (Cooldown: {}s left)", left)
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
    Json(input): Json<ProviderInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
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
pub async fn get_daily_analytics(
    State(reloadable): State<ReloadableState>,
    Query(query): Query<AnalyticsQuery>,
) -> Result<Json<AnalyticsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let days = query.days.unwrap_or(30);
    let stats = reloadable
        .db
        .get_daily_stats(days)
        .map_err(|e| internal_error(e.to_string()))?;

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
        days,
        stats,
        total_input_tokens,
        total_output_tokens,
        total_requests,
        estimated_cost_saved_usd,
    }))
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

