use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{
    sse::{Event, KeepAlive, Sse},
    IntoResponse, Json,
};
use futures_util::stream;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;
use wirellm_core::config::VirtualModelTarget;
use wirellm_core::state::CircuitState;

use crate::ReloadableState;

pub static RECOMMENDED_SETUP_MD: &str = include_str!("../resources/recommended_setup.md");

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct McpQuery {
    pub session_id: Option<String>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Serialize, Debug)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

/// GET /mcp/sse - Standard Model Context Protocol SSE transport endpoint
pub async fn mcp_sse_handler() -> impl IntoResponse {
    let session_id = format!("wirellm-mcp-{:016x}", rand::random::<u64>());
    let endpoint_uri = format!("/mcp/messages?sessionId={}", session_id);

    // Initial event advertises the message endpoint
    let initial_event = Event::default().event("endpoint").data(endpoint_uri);

    let stream = stream::once(async move { Ok::<Event, Infallible>(initial_event) });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// Core JSON-RPC dispatch shared by the legacy (/mcp/messages) and streamable (/mcp) transports.
/// Returns None for notifications (JSON-RPC messages without an id).
async fn process_jsonrpc(
    reloadable: ReloadableState,
    payload: JsonRpcRequest,
) -> Option<JsonRpcResponse> {
    let req_id = payload.id.clone();

    let response = match payload.method.as_str() {
        "initialize" => {
            let res = json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "wirellm",
                    "version": "0.2.0"
                }
            });
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req_id,
                result: Some(res),
                error: None,
            }
        },
        "notifications/initialized" => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: req_id,
            result: Some(json!({"status": "acknowledged"})),
            error: None,
        },
        "tools/list" => {
            let tools = json!({
                "tools": [
                    {
                        "name": "wirellm_get_status",
                        "description": "Get current WireLLM runtime status, circuit states, active providers, and lifetime token counts.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "wirellm_list_providers",
                        "description": "List all configured LLM providers, their base URLs, active models, and rate limits.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "wirellm_add_provider",
                        "description": "Add or update an LLM provider in WireLLM. Connect an API key via 'vault_secret' (e.g. 'NVIDIA_API_KEY'), 'api_key_template' (e.g. '${NVIDIA_API_KEY}'), or pass 'raw_api_key' to automatically encrypt and store it in the central Vault. Changes take effect immediately.",
                        "inputSchema": {
                            "type": "object",
                            "required": ["name", "base_url", "models"],
                            "properties": {
                                "name": { "type": "string", "description": "Unique provider identifier (e.g. nvidia, groq, cerebras)" },
                                "base_url": { "type": "string", "description": "OpenAI-compatible base URL" },
                                "vault_secret": { "type": "string", "description": "Name of an existing Vault secret to bind (e.g. NVIDIA_API_KEY, GROQ, OPENROUTER)" },
                                "api_key_template": { "type": "string", "description": "API key or env var template like ${NVIDIA_API_KEY}" },
                                "raw_api_key": { "type": "string", "description": "Raw API key to automatically encrypt (AES-256-GCM) and save into the central Vault" },
                                "models": { "type": "array", "items": { "type": "string" }, "description": "List of model IDs supported by this provider" },
                                "rpm": { "type": "integer", "description": "Optional rate limit in requests per minute" },
                                "min_interval_ms": { "type": "integer", "description": "Optional proactive burst spacing in milliseconds" },
                                "max_concurrency": { "type": "integer", "description": "Optional maximum concurrent requests" },
                                "smartness_score": { "type": "integer", "description": "Optional model intelligence score (0-100)" },
                                "headers": { "type": "object", "description": "Optional custom HTTP headers sent to the upstream (values may be Vault/env templates like ${VAR}). Providers on opencode.ai/zen automatically get x-opencode-session." }
                            }
                        }
                    },
                    {
                        "name": "wirellm_discover_provider_models",
                        "description": "Explore and discover live models supported by an upstream provider (queries GET /v1/models using API key from central Vault or parameters). Returns sorted model list and top recommendations for coding (coder), reasoning (planner), and fast dispatch (orchestrator).",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "provider": { "type": "string", "description": "Optional name of a configured provider (e.g. nvidia, groq, openrouter, deepseek, gemini)" },
                                "base_url": { "type": "string", "description": "Optional OpenAI-compatible base URL (e.g. https://integrate.api.nvidia.com/v1)" },
                                "vault_secret": { "type": "string", "description": "Optional name of the Vault secret to use for authentication (e.g. NVIDIA_API_KEY)" },
                                "api_key": { "type": "string", "description": "Optional raw API key (if not using Vault secret)" }
                            }
                        }
                    },
                    {
                        "name": "wirellm_delete_provider",
                        "description": "Delete a provider from WireLLM.",
                        "inputSchema": {
                            "type": "object",
                            "required": ["name"],
                            "properties": {
                                "name": { "type": "string", "description": "Name of the provider to remove" }
                            }
                        }
                    },
                    {
                        "name": "wirellm_reset_circuit",
                        "description": "Reset tripped circuit breakers and rate limits for a provider, bringing it back online immediately.",
                        "inputSchema": {
                            "type": "object",
                            "required": ["name"],
                            "properties": {
                                "name": { "type": "string", "description": "Provider name" }
                            }
                        }
                    },
                    {
                        "name": "wirellm_get_token_stats",
                        "description": "Get daily token consumption, request counts, average latency, and estimated cost savings.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "days": { "type": "integer", "description": "Number of days of history (default 30)" }
                            }
                        }
                    },
                    {
                        "name": "wirellm_list_virtual_models",
                        "description": "List virtual model router aliases and their prioritized fallback chains.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "wirellm_update_virtual_model",
                        "description": "Configure candidate targets and dynamic ranking weights for a virtual model.",
                        "inputSchema": {
                            "type": "object",
                            "required": ["name", "targets"],
                            "properties": {
                                "name": { "type": "string", "description": "Virtual model alias (e.g. coder, planner, orchestrator, code-reviewer)" },
                                "description": { "type": "string", "description": "Description of model tier" },
                                "targets": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "required": ["provider", "model"],
                                        "properties": {
                                            "provider": { "type": "string" },
                                            "model": { "type": "string" }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    {
                        "name": "wirellm_test_virtual_model",
                        "description": "Fire a probe chat completion through a virtual model ladder (or direct model ID) via the gateway itself. Reports which provider served the request, latency, finish reason, and reply text without modifying any state.",
                        "inputSchema": {
                            "type": "object",
                            "required": ["name"],
                            "properties": {
                                "name": { "type": "string", "description": "Virtual model alias (e.g. coder, openviking) or concrete model ID (e.g. gemini-2.5-flash)" },
                                "prompt": { "type": "string", "description": "Optional probe prompt (default 'Say OK')" },
                                "max_tokens": { "type": "integer", "description": "Optional probe max_tokens (default 64)" }
                            }
                        }
                    },
                    {
                        "name": "wirellm_test_provider",
                        "description": "Fire a probe chat completion DIRECTLY against one provider (not a ladder) to verify its configuration and measure real generation latency. Reports status, latency, reply text, and errors. State-safe.",
                        "inputSchema": {
                            "type": "object",
                            "required": ["name"],
                            "properties": {
                                "name": { "type": "string", "description": "Provider name (e.g. gemini-flash, nvidia-nemotron-lightning)" },
                                "model": { "type": "string", "description": "Optional concrete model ID (default: provider's first model)" },
                                "prompt": { "type": "string", "description": "Optional probe prompt (default 'Say OK')" },
                                "max_tokens": { "type": "integer", "description": "Optional probe max_tokens (default 16)" }
                            }
                        }
                    },
                    {
                        "name": "wirellm_get_setup_recommendations",
                        "description": "Get recommended setup configurations, always-free providers (NVIDIA NIM, Groq, OpenRouter), budget options (DeepSeek), model capability ratings, rate limit pacing, and virtual model fallback ladders (coder, planner, orchestrator) as structured markdown and guidance.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "wirellm_set_secret",
                        "description": "Store or update an encrypted API key or token in the WireLLM central Vault. AES-256-GCM encrypted at rest. Any provider can reference it via '${NAME}' without re-entering the secret.",
                        "inputSchema": {
                            "type": "object",
                            "required": ["name", "value"],
                            "properties": {
                                "name": { "type": "string", "description": "Secret identifier (e.g. NVIDIA, NVIDIA_API_KEY, GROQ, OPENROUTER, DEEPSEEK)" },
                                "value": { "type": "string", "description": "Raw secret API key or Bearer token" },
                                "description": { "type": "string", "description": "Optional human-readable label or description" }
                            }
                        }
                    },
                    {
                        "name": "wirellm_set_secrets",
                        "description": "Batch store or update multiple encrypted API keys or tokens in the WireLLM central Vault in a single call. AES-256-GCM encrypted at rest.",
                        "inputSchema": {
                            "type": "object",
                            "required": ["secrets"],
                            "properties": {
                                "secrets": {
                                    "type": "array",
                                    "description": "List of secret objects with name, value, and optional description",
                                    "items": {
                                        "type": "object",
                                        "required": ["name", "value"],
                                        "properties": {
                                            "name": { "type": "string", "description": "Secret identifier (e.g. NVIDIA_API_KEY, GROQ_API_KEY)" },
                                            "value": { "type": "string", "description": "Raw secret token or key" },
                                            "description": { "type": "string", "description": "Optional human-readable label" }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    {
                        "name": "wirellm_get_secret",
                        "description": "Retrieve metadata and masked preview for a specific Vault secret by name or alias. Plaintext secrets are never returned.",
                        "inputSchema": {
                            "type": "object",
                            "required": ["name"],
                            "properties": {
                                "name": { "type": "string", "description": "Name or alias of the secret to inspect" }
                            }
                        }
                    },
                    {
                        "name": "wirellm_list_secrets",
                        "description": "List all secrets currently stored in the central Vault with their masked previews and descriptions. Plaintext secrets are never returned.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "wirellm_delete_secret",
                        "description": "Delete a secret from the central Vault.",
                        "inputSchema": {
                            "type": "object",
                            "required": ["name"],
                            "properties": {
                                "name": { "type": "string", "description": "Name of secret to delete" }
                            }
                        }
                    },
                    {
                        "name": "wirellm_get_routing_policy",
                        "description": "Get the active timeout, overload breathing room, and circuit breaker cooldown settings.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "wirellm_set_routing_policy",
                        "description": "Update and persist timeout, overload breathing room, and circuit breaker cooldown settings.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "stream_header_timeout_secs": { "type": "integer", "description": "Per-attempt response-header timeout for streaming requests in seconds (default 22)" },
                                "nonstream_header_timeout_secs": { "type": "integer", "description": "Full non-stream generation budget in seconds — non-stream upstreams send headers only after generating (default 300)" },
                                "fault_window_secs": { "type": "integer", "description": "Sliding window to count recent overload faults in seconds (default 180)" },
                                "overload_base_cooldown_secs": { "type": "integer", "description": "Base cooldown after 1st overload fault in seconds (default 15)" },
                                "overload_max_cooldown_secs": { "type": "integer", "description": "Maximum cap on overload cooldown in seconds (default 90)" },
                                "circuit_failure_threshold": { "type": "integer", "description": "Consecutive failures required to open circuit breaker (default 3)" },
                                "circuit_base_cooldown_secs": { "type": "integer", "description": "Initial circuit breaker open duration in seconds (default 30)" },
                                "circuit_max_cooldown_secs": { "type": "integer", "description": "Maximum cap on circuit breaker open duration in seconds (default 120)" },
                                "rate_limit_default_cooldown_secs": { "type": "integer", "description": "Fallback cooldown on HTTP 429 when Retry-After header is absent (default 10)" },
                                "decay_interval_secs": { "type": "integer", "description": "Idle time required to decay 1 consecutive failure in seconds (default 180)" }
                            }
                        }
                    }
                ]
            });
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req_id,
                result: Some(tools),
                error: None,
            }
        },
        "tools/call" => {
            let params = payload.params.unwrap_or(json!({}));
            let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));

            let tool_output = handle_tool_call(tool_name, args, &reloadable).await;
            match tool_output {
                Ok(content_str) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req_id,
                    result: Some(json!({
                        "content": [
                            {
                                "type": "text",
                                "text": content_str
                            }
                        ]
                    })),
                    error: None,
                },
                Err(err_msg) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req_id,
                    result: None,
                    error: Some(json!({
                        "code": -32000,
                        "message": err_msg
                    })),
                },
            }
        },
        other => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: req_id,
            result: None,
            error: Some(json!({
                "code": -32601,
                "message": format!("Method not found: {}", other)
            })),
        },
    };

    if payload.id.is_some() {
        Some(response)
    } else {
        None
    }
}

/// POST /mcp/messages - Legacy HTTP+SSE transport message endpoint (responses inline).
pub async fn mcp_messages_handler(
    State(reloadable): State<ReloadableState>,
    Query(_query): Query<McpQuery>,
    Json(payload): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    match process_jsonrpc(reloadable, payload).await {
        Some(resp) => Json(resp),
        None => Json(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: None,
            result: Some(json!({ "status": "acknowledged" })),
            error: None,
        }),
    }
}

/// POST /mcp - Streamable HTTP transport: single JSON response per POST, 202 for notifications.
pub async fn mcp_streamable_post_handler(
    State(reloadable): State<ReloadableState>,
    Query(_query): Query<McpQuery>,
    Json(payload): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    match process_jsonrpc(reloadable, payload).await {
        Some(resp) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            Json(serde_json::to_value(resp).unwrap_or_else(|_| json!({}))),
        ),
        None => (
            StatusCode::ACCEPTED,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({})),
        ),
    }
}

/// GET /mcp - no SSE upgrade on the streamable transport; reply with the allowed methods.
pub async fn mcp_streamable_get_handler() -> impl IntoResponse {
    (StatusCode::METHOD_NOT_ALLOWED, [("Allow", "POST, DELETE")])
}

/// DELETE /mcp - stateless server; session termination is always acknowledged.
pub async fn mcp_streamable_delete_handler() -> impl IntoResponse {
    StatusCode::OK
}

async fn handle_tool_call(
    name: &str,
    args: Value,
    reloadable: &ReloadableState,
) -> Result<String, String> {
    match name {
        "wirellm_get_status" | "oxllm_get_status" => {
            let app_state = reloadable.app_state.borrow().clone();
            let summaries = reloadable
                .db
                .get_provider_summaries()
                .map_err(|e| e.to_string())?;

            let mut value = json!(summaries);
            // Merge live routing state (circuit, breathing room, auto-disable) into each row
            if let Some(arr) = value.as_array_mut() {
                let now = std::time::Instant::now();
                for item in arr.iter_mut() {
                    let name: Option<String> = item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let rt = name
                        .as_deref()
                        .and_then(|nm| app_state.providers.iter().find(|p| p.name == nm));
                    if let Some(rt) = rt {
                        let circ = *rt.circuit.read().await;
                        item["circuit"] = json!(match circ {
                            CircuitState::Closed => "Closed (Healthy)".to_string(),
                            CircuitState::HalfOpen => "Half-Open (Probing)".to_string(),
                            CircuitState::Open { until } => {
                                if now >= until {
                                    "Half-Open (Ready to Probe)".to_string()
                                } else {
                                    format!(
                                        "Open ({}s cooldown left)",
                                        until.saturating_duration_since(now).as_secs()
                                    )
                                }
                            },
                        });
                        item["recent_faults"] = json!(rt.fault_history.read().await.len());
                        let rl = *rt.rate_limited_until.read().await;
                        item["breathing_room_secs"] = json!(match rl {
                            Some(until) if now < until => until.saturating_duration_since(now).as_secs(),
                            _ => 0,
                        });
                        item["auto_disabled_reason"] = json!(rt.auto_disabled().await);
                    }
                }
            }

            let mut out = serde_json::Map::new();
            out.insert("uptime_secs".to_string(), json!(reloadable.start_time.elapsed().as_secs()));
            out.insert("active_provider_count".to_string(), json!(app_state.providers.len()));
            // Full-history totals from daily_usage — identical source to the
            // dashboard header and /api/analytics/daily (days=0)
            let lifetime_totals = reloadable
                .db
                .get_daily_stats(0)
                .map_err(|e| format!("Database error: {}", e))?;
            let (mut tr, mut tin, mut tout) = (0u64, 0u64, 0u64);
            for s in &lifetime_totals {
                tr += s.total_requests;
                tin += s.total_input_tokens;
                tout += s.total_output_tokens;
            }
            out.insert("total_requests".to_string(), json!(tr));
            out.insert("total_input_tokens".to_string(), json!(tin));
            out.insert("total_output_tokens".to_string(), json!(tout));
            out.insert("providers".to_string(), value);

            serde_json::to_string_pretty(&out).map_err(|e| e.to_string())
        },
        "wirellm_list_providers" | "oxllm_list_providers" => {
            let summaries = reloadable
                .db
                .get_provider_summaries()
                .map_err(|e| e.to_string())?;
            serde_json::to_string_pretty(&summaries).map_err(|e| e.to_string())
        },
        "wirellm_add_provider" | "oxllm_add_provider" => {
            let prov_name = args.get("name").and_then(|v| v.as_str()).ok_or("Missing 'name'")?;
            let base_url = args.get("base_url").and_then(|v| v.as_str()).ok_or("Missing 'base_url'")?;
            let models_val = args.get("models").and_then(|v| v.as_array()).ok_or("Missing 'models'")?;
            let models: Vec<String> = models_val.iter().filter_map(|m| m.as_str().map(|s| s.to_string())).collect();
            let rpm = args.get("rpm").and_then(|v| v.as_u64()).map(|v| v as u32);
            let min_interval_ms = args.get("min_interval_ms").and_then(|v| v.as_u64());
            let max_concurrency = args.get("max_concurrency").and_then(|v| v.as_u64()).map(|v| v as u32);
            let smartness_score = args.get("smartness_score").and_then(|v| v.as_u64()).map(|v| v as u32);

            let raw_api_key = args.get("raw_api_key").and_then(|v| v.as_str());
            let vault_secret = args.get("vault_secret").and_then(|v| v.as_str());
            let tmpl_opt = args.get("api_key_template").and_then(|v| v.as_str());

            let api_key_template = match (tmpl_opt, vault_secret, raw_api_key) {
                (Some(t), _, _) if !t.trim().is_empty() => t.trim().to_string(),
                (_, Some(vs), _) if !vs.trim().is_empty() => {
                    let clean = vs.trim().to_uppercase().replace('-', "_");
                    if clean.starts_with('$') {
                        clean
                    } else {
                        format!("${{{}}}", clean)
                    }
                },
                (_, _, Some(_)) => {
                    format!("${{{}_API_KEY}}", prov_name.to_uppercase().replace('-', "_"))
                },
                _ => return Err("Missing 'api_key_template', 'vault_secret', or 'raw_api_key'".to_string()),
            };

            let mut provider_headers: HashMap<String, String> = args
                .get("headers")
                .and_then(|v| v.as_object())
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect()
                })
                .unwrap_or_default();
            crate::admin::normalize_provider_headers(base_url, &mut provider_headers);
            reloadable
                .db
                .upsert_provider(
                    prov_name,
                    true,
                    base_url,
                    &api_key_template,
                    &models,
                    &provider_headers,
                    rpm,
                    min_interval_ms,
                    max_concurrency,
                    smartness_score,
                    raw_api_key,
                )
                .map_err(|e| format!("Failed to save provider: {}", e))?;

            let _ = reloadable.reloader.reload();
            Ok(format!("Provider '{}' configured successfully and reloaded", prov_name))
        },
        "wirellm_discover_provider_models"
        | "wirellm_explore_models"
        | "wirellm_fetch_models"
        | "wirellm_list_upstream_models" => {
            let input = crate::admin::DiscoverModelsInput {
                provider: args.get("provider").and_then(|v| v.as_str()).map(|s| s.to_string()),
                base_url: args.get("base_url").and_then(|v| v.as_str()).map(|s| s.to_string()),
                vault_secret: args.get("vault_secret").and_then(|v| v.as_str()).map(|s| s.to_string()),
                api_key: args.get("api_key").and_then(|v| v.as_str()).map(|s| s.to_string()),
            };

            let res = crate::admin::discover_models_impl(&reloadable.db, input).await?;
            serde_json::to_string_pretty(&res).map_err(|e| e.to_string())
        },
        "wirellm_delete_provider" | "oxllm_delete_provider" => {
            let prov_name = args.get("name").and_then(|v| v.as_str()).ok_or("Missing 'name'")?;
            reloadable
                .db
                .delete_provider(prov_name)
                .map_err(|e| format!("Failed to delete provider: {}", e))?;
            let _ = reloadable.reloader.reload();
            Ok(format!("Provider '{}' deleted successfully", prov_name))
        },
        "wirellm_reset_circuit" | "oxllm_reset_circuit" => {
            let prov_name = args.get("name").and_then(|v| v.as_str()).ok_or("Missing 'name'")?;
            let app_state = reloadable.app_state.borrow().clone();
            if let Some(p) = app_state.providers.iter().find(|p| p.name == prov_name) {
                // Canonical lock order: consecutive_failures → circuit → rest
                *p.consecutive_failures.write().await = 0;
                *p.circuit.write().await = wirellm_core::state::CircuitState::Closed;
                *p.rate_limited_until.write().await = None;
                *p.auto_disabled_reason.write().await = None;
                p.severed_strikes
                    .store(0, std::sync::atomic::Ordering::Release);
                p.fault_history.write().await.clear();
                Ok(format!("Provider '{}' circuit breaker reset to Closed (Healthy)", prov_name))
            } else {
                Err(format!("Provider '{}' not found in active providers", prov_name))
            }
        },
        "wirellm_get_token_stats" | "oxllm_get_token_stats" => {
            let days = args.get("days").and_then(|v| v.as_u64()).unwrap_or(30) as u32;
            let stats = reloadable.db.get_daily_stats(days).map_err(|e| e.to_string())?;
            serde_json::to_string_pretty(&stats).map_err(|e| e.to_string())
        },
        "wirellm_list_virtual_models" | "oxllm_list_virtual_models" => {
            let details = reloadable
                .db
                .get_virtual_models_detail()
                .map_err(|e| e.to_string())?;
            serde_json::to_string_pretty(&details).map_err(|e| e.to_string())
        },
        "wirellm_update_virtual_model" | "oxllm_update_virtual_model" => {
            let vm_name = args.get("name").and_then(|v| v.as_str()).ok_or("Missing 'name'")?;
            let desc = args.get("description").and_then(|v| v.as_str()).unwrap_or("");
            let targets_arr = args.get("targets").and_then(|v| v.as_array()).ok_or("Missing 'targets'")?;

            let mut targets = Vec::new();
            for item in targets_arr {
                let provider = item.get("provider").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let model = item.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if !provider.is_empty() && !model.is_empty() {
                    targets.push(VirtualModelTarget { provider, model });
                }
            }

            reloadable
                .db
                .upsert_virtual_model(vm_name, desc, &targets, None)
                .map_err(|e| format!("Failed to save virtual model: {}", e))?;
            let _ = reloadable.reloader.reload();
            Ok(format!("Virtual model '{}' updated with {} targets", vm_name, targets.len()))
        },
        "wirellm_get_setup_recommendations" | "oxllm_get_setup_recommendations" => {
            Ok(RECOMMENDED_SETUP_MD.to_string())
        },
        "wirellm_set_secret"
        | "wirellm_set_vault_secret"
        | "wirellm_create_secret"
        | "wirellm_create_vault_secret"
        | "wirellm_add_secret"
        | "wirellm_add_vault_secret"
        | "oxllm_set_secret" => {
            let secret_name = args.get("name").and_then(|v| v.as_str()).ok_or("Missing 'name'")?;
            let secret_val = args.get("value").and_then(|v| v.as_str()).ok_or("Missing 'value'")?;
            let desc = args.get("description").and_then(|v| v.as_str()).unwrap_or("");

            let clean_name = secret_name.trim().to_uppercase().replace('-', "_");
            reloadable
                .db
                .upsert_vault_secret(&clean_name, secret_val, desc)
                .map_err(|e| format!("Failed to save secret to vault: {}", e))?;

            let _ = reloadable.reloader.reload();
            Ok(format!("Secret '{}' securely encrypted (AES-256-GCM) and saved to Vault. Reference it in any provider with '\\${{{}}}'", clean_name, clean_name))
        },
        "wirellm_set_secrets"
        | "wirellm_batch_set_secrets"
        | "wirellm_create_secrets"
        | "wirellm_set_vault_secrets" => {
            let secrets_arr = args.get("secrets").and_then(|v| v.as_array()).ok_or("Missing 'secrets' array")?;
            let mut saved_names = Vec::new();

            for item in secrets_arr {
                let name = item.get("name").and_then(|v| v.as_str()).ok_or("Missing 'name' in secret entry")?;
                let val = item.get("value").and_then(|v| v.as_str()).ok_or("Missing 'value' in secret entry")?;
                let desc = item.get("description").and_then(|v| v.as_str()).unwrap_or("");
                let clean_name = name.trim().to_uppercase().replace('-', "_");
                reloadable
                    .db
                    .upsert_vault_secret(&clean_name, val, desc)
                    .map_err(|e| format!("Failed to save secret '{}' to vault: {}", clean_name, e))?;
                saved_names.push(clean_name);
            }

            let _ = reloadable.reloader.reload();
            Ok(format!("Successfully saved {} secret(s) to central Vault: {}", saved_names.len(), saved_names.join(", ")))
        },
        "wirellm_get_secret"
        | "wirellm_get_vault_secret" => {
            let secret_name = args.get("name").and_then(|v| v.as_str()).ok_or("Missing 'name'")?;
            if let Some(record) = reloadable.db.get_vault_secret_record(secret_name).map_err(|e| e.to_string())? {
                serde_json::to_string_pretty(&record).map_err(|e| e.to_string())
            } else {
                Err(format!("Secret '{}' not found in central Vault", secret_name))
            }
        },
        "wirellm_list_secrets"
        | "wirellm_list_vault_secrets"
        | "wirellm_get_secrets"
        | "oxllm_list_secrets" => {
            let secrets = reloadable
                .db
                .list_vault_secrets()
                .map_err(|e| e.to_string())?;
            serde_json::to_string_pretty(&secrets).map_err(|e| e.to_string())
        },
        "wirellm_delete_secret"
        | "wirellm_delete_vault_secret"
        | "wirellm_remove_secret"
        | "wirellm_remove_vault_secret"
        | "oxllm_delete_secret" => {
            let secret_name = args.get("name").and_then(|v| v.as_str()).ok_or("Missing 'name'")?;
            let clean_name = secret_name.trim().to_uppercase().replace('-', "_");
            reloadable
                .db
                .delete_vault_secret(&clean_name)
                .map_err(|e| format!("Failed to delete secret from vault: {}", e))?;
            let _ = reloadable.reloader.reload();
            Ok(format!("Secret '{}' removed from Vault", clean_name))
        },
        "wirellm_test_virtual_model" => {
            let model = args.get("name").and_then(|v| v.as_str()).ok_or("Missing 'name' (virtual model alias or model ID)")?.to_string();
            let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("Say OK").to_string();
            let max_tokens = args.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(64);

            // Resolve the gateway's ACTUAL listening port the same way the
            // daemon does: CLI > PORT/WIRELLM_PORT env > server_settings DB
            // value. Dialing the raw DB port breaks where the runtime applied
            // an env override (e.g. Kubernetes PORT).
            let db_port = reloadable.db.load_config().map_err(|e| e.to_string())?.server.port;
            let port = std::env::var("PORT")
                .or_else(|_| std::env::var("WIRELLM_PORT"))
                .ok()
                .and_then(|p_str| crate::cli::parse_port(&p_str).ok())
                .unwrap_or(db_port);
            let url = format!("http://127.0.0.1:{}/v1/chat/completions", port);
            let body = serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": max_tokens
            });

            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(180))
                .build()
                .map_err(|e| e.to_string())?;

            let start = std::time::Instant::now();
            let res = client.post(&url).json(&body).send().await
                .map_err(|e| format!("Gateway call failed: {}", e))?;
            let latency_ms = start.elapsed().as_millis() as u64;
            let status_code = res.status().as_u16();
            let payload: serde_json::Value = res.json().await
                .map_err(|e| format!("Gateway returned non-JSON response: {}", e))?;

            let served_by = payload.get("model").and_then(|m| m.as_str()).unwrap_or("").to_string();
            let content = payload.pointer("/choices/0/message/content")
                .and_then(|c| c.as_str()).unwrap_or("").to_string();
            let finish = payload.pointer("/choices/0/finish_reason")
                .and_then(|c| c.as_str()).unwrap_or("").to_string();
            let error = payload.get("error").cloned();

            serde_json::to_string_pretty(&serde_json::json!({
                "status_code": status_code,
                "served_by": served_by,
                "latency_ms": latency_ms,
                "finish_reason": finish,
                "content": content,
                "error": error,
            })).map_err(|e| e.to_string())
        },
        "wirellm_test_provider" => {
            let prov_name = args.get("name").and_then(|v| v.as_str()).ok_or("Missing 'name' (provider)")?.to_string();
            let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("Say OK").to_string();
            let max_tokens = args.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(16);

            let full_config = reloadable.db.load_config().map_err(|e| e.to_string())?;
            let p_cfg = full_config.providers.into_iter()
                .find(|p| p.name == prov_name)
                .ok_or_else(|| format!("Provider '{}' not found", prov_name))?;
            if !p_cfg.enabled {
                return Err(format!("Provider '{}' is disabled", prov_name));
            }

            let model = match args.get("model").and_then(|v| v.as_str()) {
                Some(m) if !m.trim().is_empty() => m.to_string(),
                _ => p_cfg.models.first().cloned()
                    .ok_or_else(|| format!("Provider '{}' has no models configured", prov_name))?,
            };

            let chat_url = format!("{}/chat/completions", p_cfg.base_url.trim_end_matches('/'));
            let body = serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": max_tokens
            });

            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .map_err(|e| e.to_string())?;

            let start = std::time::Instant::now();
            let mut req = client.post(&chat_url).json(&body);
            if !p_cfg.api_key.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", p_cfg.api_key));
            }
            for (k, v) in &p_cfg.headers {
                req = req.header(k, v);
            }
            let res = req.send().await
                .map_err(|e| format!("Request to '{}' failed: {} (check base_url/network)", prov_name, e))?;
            let latency_ms = start.elapsed().as_millis() as u64;
            let status_code = res.status().as_u16();
            let payload: serde_json::Value = res.json().await
                .map_err(|e| format!("Provider returned non-JSON: {}", e))?;

            let served_by = payload.get("model").and_then(|m| m.as_str()).unwrap_or(&model).to_string();
            let content = payload.pointer("/choices/0/message/content")
                .and_then(|c| c.as_str()).unwrap_or("").to_string();
            let finish = payload.pointer("/choices/0/finish_reason")
                .and_then(|c| c.as_str()).unwrap_or("").to_string();
            let error = payload.get("error").cloned();

            serde_json::to_string_pretty(&serde_json::json!({
                "provider": prov_name,
                "status_code": status_code,
                "served_by": served_by,
                "latency_ms": latency_ms,
                "finish_reason": finish,
                "content": content,
                "error": error,
            })).map_err(|e| e.to_string())
        },
        "wirellm_get_routing_policy" => {
            let policy = wirellm_core::state::current_routing_policy();
            serde_json::to_string_pretty(&policy).map_err(|e| e.to_string())
        },
        "wirellm_set_routing_policy" => {
            let mut policy = reloadable
                .db
                .get_routing_policy()
                .unwrap_or_else(|_| wirellm_core::state::current_routing_policy());

            if let Some(v) = args.get("stream_header_timeout_secs").and_then(|v| v.as_u64()) {
                policy.stream_header_timeout_secs = v;
            }
            if let Some(v) = args.get("nonstream_header_timeout_secs").and_then(|v| v.as_u64()) {
                policy.nonstream_header_timeout_secs = v;
            }
            if let Some(v) = args.get("fault_window_secs").and_then(|v| v.as_u64()) {
                policy.fault_window_secs = v;
            }
            if let Some(v) = args.get("overload_base_cooldown_secs").and_then(|v| v.as_u64()) {
                policy.overload_base_cooldown_secs = v;
            }
            if let Some(v) = args.get("overload_max_cooldown_secs").and_then(|v| v.as_u64()) {
                policy.overload_max_cooldown_secs = v;
            }
            if let Some(v) = args.get("circuit_failure_threshold").and_then(|v| v.as_u64()) {
                policy.circuit_failure_threshold = v as u32;
            }
            if let Some(v) = args.get("circuit_base_cooldown_secs").and_then(|v| v.as_u64()) {
                policy.circuit_base_cooldown_secs = v;
            }
            if let Some(v) = args.get("circuit_max_cooldown_secs").and_then(|v| v.as_u64()) {
                policy.circuit_max_cooldown_secs = v;
            }
            if let Some(v) = args.get("rate_limit_default_cooldown_secs").and_then(|v| v.as_u64()) {
                policy.rate_limit_default_cooldown_secs = v;
            }
            if let Some(v) = args.get("decay_interval_secs").and_then(|v| v.as_u64()) {
                policy.decay_interval_secs = v;
            }
            // Enforce the shared safety floors (single source: RoutingPolicy::clamp)
            policy.clamp();

            reloadable
                .db
                .save_routing_policy(&policy)
                .map_err(|e| format!("Failed to persist routing policy: {}", e))?;
            wirellm_core::state::apply_routing_policy(&policy);

            serde_json::to_string_pretty(&wirellm_core::state::current_routing_policy())
                .map_err(|e| e.to_string())
        },
        unknown => Err(format!("Unknown tool: {}", unknown)),
    }
}
