use axum::extract::{Query, State};
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

/// POST /mcp/messages - Handles JSON-RPC 2.0 messages from MCP clients
pub async fn mcp_messages_handler(
    State(reloadable): State<ReloadableState>,
    Query(_query): Query<McpQuery>,
    Json(payload): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    let req_id = payload.id.clone();

    match payload.method.as_str() {
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
            Json(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req_id,
                result: Some(res),
                error: None,
            })
        },
        "notifications/initialized" => Json(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: req_id,
            result: Some(json!({"status": "acknowledged"})),
            error: None,
        }),
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
                        "description": "Add or update an LLM provider in WireLLM. The API key can be an environment variable template like '${MY_KEY}'. Changes take effect immediately.",
                        "inputSchema": {
                            "type": "object",
                            "required": ["name", "base_url", "api_key_template", "models"],
                            "properties": {
                                "name": { "type": "string", "description": "Unique provider identifier (e.g. nvidia, groq, cerebras)" },
                                "base_url": { "type": "string", "description": "OpenAI-compatible base URL" },
                                "api_key_template": { "type": "string", "description": "API key or env var template like ${NVIDIA_API_KEY}" },
                                "models": { "type": "array", "items": { "type": "string" }, "description": "List of model IDs supported by this provider" },
                                "rpm": { "type": "integer", "description": "Optional rate limit in requests per minute" },
                                "min_interval_ms": { "type": "integer", "description": "Optional proactive burst spacing in milliseconds" },
                                "max_concurrency": { "type": "integer", "description": "Optional maximum concurrent requests" },
                                "smartness_score": { "type": "integer", "description": "Optional model intelligence score (0-100)" }
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
                    }
                ]
            });
            Json(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req_id,
                result: Some(tools),
                error: None,
            })
        },
        "tools/call" => {
            let params = payload.params.unwrap_or(json!({}));
            let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));

            let tool_output = handle_tool_call(tool_name, args, &reloadable).await;
            match tool_output {
                Ok(content_str) => Json(JsonRpcResponse {
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
                }),
                Err(err_msg) => Json(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req_id,
                    result: None,
                    error: Some(json!({
                        "code": -32000,
                        "message": err_msg
                    })),
                }),
            }
        },
        other => Json(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: req_id,
            result: None,
            error: Some(json!({
                "code": -32601,
                "message": format!("Method not found: {}", other)
            })),
        }),
    }
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

            let mut out = serde_json::Map::new();
            out.insert("uptime_secs".to_string(), json!(reloadable.start_time.elapsed().as_secs()));
            out.insert("active_provider_count".to_string(), json!(app_state.providers.len()));
            out.insert("providers".to_string(), json!(summaries));

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
            let api_key_template = args.get("api_key_template").and_then(|v| v.as_str()).ok_or("Missing 'api_key_template'")?;
            let models_val = args.get("models").and_then(|v| v.as_array()).ok_or("Missing 'models'")?;
            let models: Vec<String> = models_val.iter().filter_map(|m| m.as_str().map(|s| s.to_string())).collect();
            let rpm = args.get("rpm").and_then(|v| v.as_u64()).map(|v| v as u32);
            let min_interval_ms = args.get("min_interval_ms").and_then(|v| v.as_u64());
            let max_concurrency = args.get("max_concurrency").and_then(|v| v.as_u64()).map(|v| v as u32);
            let smartness_score = args.get("smartness_score").and_then(|v| v.as_u64()).map(|v| v as u32);

            let raw_api_key = args.get("raw_api_key").and_then(|v| v.as_str());

            let empty_headers = HashMap::new();
            reloadable
                .db
                .upsert_provider(
                    prov_name,
                    true,
                    base_url,
                    api_key_template,
                    &models,
                    &empty_headers,
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
                *p.circuit.write().await = wirellm_core::state::CircuitState::Closed;
                *p.consecutive_failures.write().await = 0;
                *p.rate_limited_until.write().await = None;
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
        "wirellm_set_secret" | "wirellm_set_vault_secret" | "oxllm_set_secret" => {
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
        "wirellm_list_secrets" | "wirellm_list_vault_secrets" | "oxllm_list_secrets" => {
            let secrets = reloadable
                .db
                .list_vault_secrets()
                .map_err(|e| e.to_string())?;
            serde_json::to_string_pretty(&secrets).map_err(|e| e.to_string())
        },
        "wirellm_delete_secret" | "wirellm_delete_vault_secret" | "oxllm_delete_secret" => {
            let secret_name = args.get("name").and_then(|v| v.as_str()).ok_or("Missing 'name'")?;
            let clean_name = secret_name.trim().to_uppercase().replace('-', "_");
            reloadable
                .db
                .delete_vault_secret(&clean_name)
                .map_err(|e| format!("Failed to delete secret from vault: {}", e))?;
            let _ = reloadable.reloader.reload();
            Ok(format!("Secret '{}' removed from Vault", clean_name))
        },
        unknown => Err(format!("Unknown tool: {}", unknown)),
    }
}
