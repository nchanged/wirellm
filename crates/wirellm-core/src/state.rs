use crate::config::VirtualModelTarget;
use crate::db::DbManager;
use reqwest::Url;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// Rolling window used to count recent overload faults (how bursty the upstream is right now).
pub const FAULT_WINDOW_SECS: u64 = 600;
/// Breathing room (seconds) applied after the first fault of a window.
pub const OVERLOAD_BASE_COOLDOWN_SECS: u64 = 10;
/// Upper bound on any single breathing window.
pub const OVERLOAD_MAX_COOLDOWN_SECS: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open { until: Instant },
    HalfOpen,
}

/// Per-provider, per-UTC-hour overload histogram ("diurnal load memory").
/// Bucket counts fade ~5% every hour so stale patterns (e.g. "NVIDIA is busy
/// during US afternoon") stop influencing breathing room after a quiet day.
pub struct FaultHistogram {
    buckets: [AtomicU64; 24],
    last_decay_epoch_hour: AtomicU64,
}

impl std::fmt::Debug for FaultHistogram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let snapshot: Vec<u64> = (0..24).map(|h| {
            self.buckets[(h % 24) as usize].load(std::sync::atomic::Ordering::Relaxed)
        }).collect();
        f.debug_tuple("FaultHistogram").field(&snapshot).finish()
    }
}

impl FaultHistogram {
    pub fn new() -> Self {
        Self {
            buckets: std::array::from_fn(|_| AtomicU64::new(0)),
            last_decay_epoch_hour: AtomicU64::new(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs() / 3600)
                    .unwrap_or(0),
            ),
        }
    }

    /// Fades all buckets by ~5% per elapsed UTC hour (catching up missed hours,
    /// capped at 48h of fade) so old patterns decay. Latches on EPOCH hours.
    pub fn tick_and_decay(&self, epoch_hour: u64) {
        let last = self.last_decay_epoch_hour.load(Ordering::Relaxed);
        if epoch_hour > last
            && self
                .last_decay_epoch_hour
                .compare_exchange(last, epoch_hour, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            // Note: a bucket holding a single fault fades to 0 after one hour
            // (integer decay) — short-lived noise should not endure.
            let missed = (epoch_hour - last).min(48);
            for _ in 0..missed {
                for b in &self.buckets {
                    let v = b.load(Ordering::Relaxed);
                    if v > 0 {
                        // (v * 19 / 20) = 5% fade per hour
                        b.store(v * 19 / 20, Ordering::Relaxed);
                    }
                }
            }
        }
    }

    /// Increments the bucket for `hour` and returns the new value.
    pub fn bump(&self, hour: u64) -> u64 {
        let b = &self.buckets[(hour % 24) as usize];
        b.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn get(&self, hour: u64) -> u64 {
        self.buckets[(hour % 24) as usize].load(Ordering::Relaxed)
    }

    pub fn snapshot(&self) -> [u64; 24] {
        std::array::from_fn(|i| self.buckets[i].load(Ordering::Relaxed))
    }

    /// Restores persisted bucket values (from the server_settings store).
    pub fn restore(&self, vals: &[u64; 24]) {
        for (b, v) in self.buckets.iter().zip(vals) {
            b.store(*v, Ordering::Relaxed);
        }
    }
}

impl Default for FaultHistogram {
    fn default() -> Self {
        Self::new()
    }
}

/// Current unix-epoch hour (latch/decay timeline — total ordering, no wraparound).
pub fn utc_epoch_hour() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 3600)
        .unwrap_or(0)
}

/// Current UTC hour of day (0..23) — the diurnal bucket index.
pub fn utc_hour_of_day() -> u64 {
    utc_epoch_hour() % 24
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

    // Adaptive overload protection ("breathing room")
    // Recent 5xx/connection faults — used to size the breathing window geometrically
    pub fault_history: Arc<RwLock<VecDeque<Instant>>>,
    // Last computed breathing window (ms), for dashboards
    pub last_overload_cooldown_ms: Arc<AtomicU64>,
    // Per-UTC-hour fault memory — busy hours (e.g. NVIDIA US-afternoon) breathe deeper
    pub diurnal: Arc<FaultHistogram>,
    // Optional DB handle to persist the diurnal histogram across restarts
    pub fault_pattern_store: Option<Arc<DbManager>>,
    // Severed upstream (model removed/EOL/misconfigured, e.g. HTTP 404/410):
    // provider is skipped in routing until an admin clears the reason.
    // Two strikes required so a single transient 404 can't hard-kill a provider.
    pub auto_disabled_reason: Arc<RwLock<Option<String>>>,
    pub severed_strikes: Arc<AtomicU32>,
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

    /// Returns the auto-disable reason when the provider's model is severed
    /// upstream (removed/EOL/misconfigured), making it skip routing entirely.
    pub async fn auto_disabled(&self) -> Option<String> {
        self.auto_disabled_reason.read().await.clone()
    }

    /// Records an overload fault (HTTP 5xx or connection failure) and returns
    /// the adaptive breathing window:
    ///
    ///   breathing = BASE(10s) × 2^(faults_in_10min_window − 1) × diurnal_factor
    ///   diurnal_factor = 1 + min(historical_faults_this_hour, 8) × 0.35
    ///
    /// Rapid overload bursts stretch geometrically toward the 5-minute cap;
    /// a first fault in a typically-busy UTC hour already breathes deeper than
    /// the floor; a first fault in a quiet hour gets exactly the 10s floor
    /// ("historical" excludes the fault being recorded).
    pub async fn record_overload_fault(&self, now: Instant) -> Duration {
        // Evict + push + count under a single guard (the count feeds the
        // geometric exponent — must not race with concurrent burst faults)
        let recent_faults = {
            let window = Duration::from_secs(FAULT_WINDOW_SECS);
            let mut hist = self.fault_history.write().await;
            while let Some(front) = hist.front() {
                if now.saturating_duration_since(*front) > window {
                    hist.pop_front();
                } else {
                    break;
                }
            }
            hist.push_back(now);
            hist.len() as u64
        };

        let epoch_hour = utc_epoch_hour();
        self.diurnal.tick_and_decay(epoch_hour);
        let hour = epoch_hour % 24;
        let historical = self.diurnal.get(hour);
        self.diurnal.bump(hour);

        let recency = 1u64 << (recent_faults.saturating_sub(1)).min(5); // 1×..32×
        let diurnal_factor = 1.0 + (historical.min(8) as f64) * 0.35; // 1×..~3.8×
        let secs =
            ((OVERLOAD_BASE_COOLDOWN_SECS as f64) * recency as f64 * diurnal_factor).round() as u64;
        let secs = secs.max(1).min(OVERLOAD_MAX_COOLDOWN_SECS);

        self.last_overload_cooldown_ms
            .store(secs * 1000, Ordering::Relaxed);

        // Persist the diurnal memory so patterns survive daemon restarts
        if let Some(store) = &self.fault_pattern_store {
            let _ = store.save_provider_fault_pattern(&self.name, &self.diurnal.snapshot());
        }

        Duration::from_secs(secs)
    }

    /// Success-side deflation: evaporate the oldest fault from the window so a
    /// recovered provider stops growing its breathing room (the diurnal
    /// histogram keeps the long-term memory via its own hourly fade).
    pub async fn record_overload_success(&self) {
        let mut hist = self.fault_history.write().await;
        hist.pop_front();
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

#[cfg(test)]
mod tests {
    use super::*;

    /// GUARANTEE: resolve_candidates returns the virtual model's targets
    /// exactly in configured ladder order — smartness/weights must never
    /// reorder the ladder.
    #[test]
    fn test_resolve_candidates_preserves_ladder_order() {
        let mk = |name: &str| ProviderState {
            name: name.to_string(),
            base_url: reqwest::Url::parse("http://127.0.0.1/v1/").unwrap(),
            api_key: "k".to_string(),
            models: vec!["m".to_string()],
            headers: HashMap::new(),
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            manual_disabled: std::sync::atomic::AtomicBool::new(false),
            requests: std::sync::atomic::AtomicU64::new(0),
            successes: std::sync::atomic::AtomicU64::new(0),
            tokens_input: std::sync::atomic::AtomicU64::new(0),
            tokens_output: std::sync::atomic::AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            diurnal: Arc::new(FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };
        // Deliberately index providers so protocol-smartness is anti-ordered —
        // any sort by smartness/brand/latency heuristics would break this test.
        let a = mk("genius-model");
        let b = mk("medium-model");
        let c = mk("tiny-model");
        let app_state = AppState {
            providers: vec![a, b, c],
            virtual_models: [(
                "ladder".to_string(),
                vec![
                    VirtualModelTarget { provider: "tiny-model".to_string(), model: "m".to_string() },
                    VirtualModelTarget { provider: "genius-model".to_string(), model: "m".to_string() },
                    VirtualModelTarget { provider: "medium-model".to_string(), model: "m".to_string() },
                ],
            )]
            .into_iter()
            .collect(),
            http_client: reqwest::Client::new(),
            upstream_timeout_secs: 30,
            auth: Default::default(),
        };

        let resolved = app_state.resolve_candidates("ladder");
        let order: Vec<&str> = resolved.iter().map(|(p, _)| p.name.as_str()).collect();
        assert_eq!(
            order,
            vec!["tiny-model", "genius-model", "medium-model"],
            "ladder order must EXACTLY match the configured target order — no smartness reordering"
        );
    }
}
