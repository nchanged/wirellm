use axum::{
    extract::State,
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use std::sync::Arc;
use std::time::Instant;
use wirellm_core::state::{AppState, CircuitState};

#[derive(Serialize)]
pub struct ModelObject {
    pub id: String,
    pub object: &'static str,
    pub created: u64,
    pub owned_by: &'static str,
}

#[derive(Serialize)]
pub struct ModelsResponse {
    pub object: &'static str,
    pub data: Vec<ModelObject>,
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
                if provider.auto_disabled_reason.read().await.is_some() {
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
        if provider.auto_disabled_reason.read().await.is_some() {
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
                        owned_by: "wirellm-provider",
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
