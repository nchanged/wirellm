use crate::config::VirtualModelTarget;
use reqwest::Url;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open { until: Instant },
    HalfOpen,
}

#[derive(Debug)]
pub struct ProviderState {
    pub name: String,
    pub base_url: Url, // Parsed reqwest::Url to handle safe path joins and trailing slashes
    pub api_key: String,
    pub models: Vec<String>,
    pub headers: HashMap<String, String>,

    // Protects volatile metrics without needing a write lock on the entire pool vector
    pub circuit: Arc<RwLock<CircuitState>>,
    pub consecutive_failures: Arc<RwLock<u32>>,
    pub rate_limited_until: Arc<RwLock<Option<Instant>>>,
    pub last_attempt_time: Arc<RwLock<Option<Instant>>>,

    // Lock-free thundering-herd permit
    pub probe_in_flight: Arc<AtomicBool>,

    // Manual admin override — skips provider in routing regardless of circuit state
    pub manual_disabled: AtomicBool,

    // Local request/token counters (visible via /status without otel collector)
    pub requests: AtomicU64,
    pub successes: AtomicU64,
    pub tokens_input: AtomicU64,
    pub tokens_output: AtomicU64,

    // Proactive Client-Side Rate Limiter (sliding 60s window & burst spacing)
    pub max_rpm: Option<u32>,
    pub min_interval_ms: Option<u64>,
    pub max_concurrency: Option<u32>,
    pub in_flight: Arc<AtomicU32>,
    pub request_timestamps: Arc<RwLock<VecDeque<Instant>>>,
    pub last_request_time: Arc<RwLock<Option<Instant>>>,
}

impl ProviderState {
    /// Returns the current number of requests made within the last 60 seconds,
    /// evicting expired entries from the sliding window.
    pub async fn current_rpm(&self, now: Instant) -> u32 {
        let mut timestamps = self.request_timestamps.write().await;
        let window_start = now - std::time::Duration::from_secs(60);
        while let Some(&ts) = timestamps.front() {
            if ts < window_start {
                timestamps.pop_front();
            } else {
                break;
            }
        }
        timestamps.len() as u32
    }

    /// Checks if the provider has reached its maximum RPM limit in the sliding 60-second window.
    /// If so, returns true indicating the candidate should be skipped proactively.
    pub async fn is_rpm_limited(&self, now: Instant) -> bool {
        if let Some(max_rpm) = self.max_rpm {
            let mut timestamps = self.request_timestamps.write().await;
            let window_start = now - std::time::Duration::from_secs(60);
            while let Some(&ts) = timestamps.front() {
                if ts < window_start {
                    timestamps.pop_front();
                } else {
                    break;
                }
            }
            timestamps.len() >= max_rpm as usize
        } else {
            false
        }
    }

    /// Checks if the provider is currently burst-limited (too fast since last request)
    /// or concurrency-limited (too many active requests in flight).
    pub async fn is_burst_limited(&self, now: Instant) -> bool {
        if let Some(ms) = self.min_interval_ms {
            let last = self.last_request_time.read().await;
            if let Some(last_ts) = *last {
                if now.saturating_duration_since(last_ts) < std::time::Duration::from_millis(ms) {
                    return true;
                }
            }
        }
        if let Some(mc) = self.max_concurrency {
            if self.in_flight.load(std::sync::atomic::Ordering::Relaxed) >= mc {
                return true;
            }
        }
        false
    }

    /// Records a new request timestamp into the sliding 60-second window and updates last_request_time.
    pub async fn record_request_timestamp(&self, now: Instant) {
        {
            let mut timestamps = self.request_timestamps.write().await;
            let window_start = now - std::time::Duration::from_secs(60);
            while let Some(&ts) = timestamps.front() {
                if ts < window_start {
                    timestamps.pop_front();
                } else {
                    break;
                }
            }
            timestamps.push_back(now);
        }
        {
            let mut last_req = self.last_request_time.write().await;
            *last_req = Some(now);
        }
    }
}

#[derive(Clone)]
pub struct SelectedProvider {
    pub name: String,
    pub base_url: Url,
    pub api_key: String,
    pub headers: HashMap<String, String>,
    pub is_probe: bool,
}

pub struct AppState {
    pub providers: Vec<ProviderState>,
    pub virtual_models: HashMap<String, Vec<VirtualModelTarget>>,
    pub http_client: reqwest::Client,
    pub upstream_timeout_secs: u64,
    pub auth: crate::config::AuthConfig,
}

impl AppState {
    /// Resolves the candidate list for a given virtual model or concrete model.
    /// 1. If defined as a virtual model pool (e.g. "coder", "planner"), returns its configured targets.
    /// 2. Otherwise, performs direct model passthrough: matches any provider that supports this model
    ///    either by exact match (e.g. "deepseek-ai/deepseek-v4.1-flash") or short alias match (e.g. "deepseek-v4.1-flash").
    pub fn resolve_candidates(&self, requested_model: &str) -> Vec<(&ProviderState, String)> {
        // 1. Check virtual model pools first
        if let Some(targets) = self.virtual_models.get(requested_model) {
            let mut candidates = Vec::new();
            for target in targets {
                if let Some(provider) = self.providers.iter().find(|p| p.name == target.provider) {
                    candidates.push((provider, target.model.clone()));
                }
            }
            if !candidates.is_empty() {
                return candidates;
            }
        }

        // 2. Direct model passthrough across all configured providers
        let mut direct_candidates = Vec::new();
        for provider in &self.providers {
            for model in &provider.models {
                let matches = model == requested_model
                    || model.ends_with(&format!("/{}", requested_model))
                    || requested_model.ends_with(&format!("/{}", model));
                if matches {
                    direct_candidates.push((provider, model.clone()));
                }
            }
        }
        direct_candidates
    }
}
