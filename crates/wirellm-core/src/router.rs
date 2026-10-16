use crate::state::{CircuitState, ProviderState, SelectedProvider};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tracing::info;

/// Modular strategy trait for selecting providers and updating their health state.
#[allow(async_fn_in_trait)]
pub trait RoutingStrategy: Send + Sync {
    /// Selects a candidate provider from the candidate list for a given virtual model.
    /// Performs dynamic idle-based penalty decay check prior to candidate evaluation.
    async fn select(&self, candidates: &[&ProviderState]) -> Option<SelectedProvider>;

    /// Records routing feedback (success or failure) to adjust candidate backoffs and circuits.
    async fn feedback(
        &self,
        provider: &ProviderState,
        success: bool,
        is_probe: bool,
        status_code: Option<u16>,
        retry_after: Option<Duration>,
    );
}

/// The default routing strategy: orders by priority, failover with exponential backoffs,
/// strict HalfOpen single-probe guards, and dynamic idle-based penalty decay.
pub struct AdaptivePriorityStrategy;

impl RoutingStrategy for AdaptivePriorityStrategy {
    async fn select(&self, candidates: &[&ProviderState]) -> Option<SelectedProvider> {
        let now = Instant::now();

        for provider in candidates {
            // Check for manual admin override
            if provider.manual_disabled.load(Ordering::Acquire) {
                continue;
            }

            // Check for auto-disabled (severed upstream: model removed/EOL/404)
            if provider.auto_disabled_reason.read().await.is_some() {
                continue;
            }

            // 1. Dynamic Idle Penalty Decay (Aging)
            let policy = crate::state::current_routing_policy();
            {
                let mut failures = provider.consecutive_failures.write().await;
                let mut last_attempt = provider.last_attempt_time.write().await;
                if *failures > 0 {
                    if let Some(last_time) = *last_attempt {
                        let elapsed = now.saturating_duration_since(last_time);
                        let decay_secs = policy.decay_interval_secs.max(15);
                        let decay_steps = elapsed.as_secs() / decay_secs;
                        if decay_steps > 0 {
                            *failures = failures.saturating_sub(decay_steps as u32);
                            *last_attempt =
                                Some(last_time + Duration::from_secs(decay_steps * decay_secs));

                            // If failures age below threshold, rehabilitate open circuits
                            if *failures < policy.circuit_failure_threshold {
                                let mut circ = provider.circuit.write().await;
                                if let CircuitState::Open { .. } = *circ {
                                    *circ = CircuitState::Closed;
                                }
                            }
                        }
                    }
                }
            }

            // 2. Proactive Client-Side Rate Limit Check (Sliding 60s Window)
            if provider.is_rpm_limited(now).await {
                info!(
                    target: "wirellm_core::router",
                    "Provider '{}' reached proactive rate limit ({} RPM). Proactively failing over to next candidate.",
                    provider.name, provider.max_rpm.unwrap_or(0)
                );
                continue;
            }

            // 2b. Proactive Burst & Concurrency Limit Check
            if provider.is_burst_limited(now).await {
                info!(
                    target: "wirellm_core::router",
                    "Provider '{}' is burst spacing or concurrency limited. Proactively failing over to next candidate.",
                    provider.name
                );
                continue;
            }

            // 3. Reactive Rate Limit Window Check (from upstream 429 responses)
            {
                let mut rl = provider.rate_limited_until.write().await;
                if let Some(until) = *rl {
                    if now >= until {
                        *rl = None;
                    } else {
                        continue; // Still rate-limited, skip candidate
                    }
                }
            }

            // 4. Circuit Breaker Checks
            let current_state = *provider.circuit.read().await;
            match current_state {
                CircuitState::Closed => {
                    // Record request for sliding RPM tracking
                    provider.record_request_timestamp(now).await;
                    // Fully healthy: Route immediately
                    return Some(SelectedProvider {
                        name: provider.name.clone(),
                        base_url: provider.base_url.clone(),
                        api_key: provider.api_key.clone(),
                        headers: provider.headers.clone(),
                        is_probe: false,
                    });
                },
                CircuitState::Open { until } => {
                    if now >= until {
                        // Cooldown expired: transition circuit state to HalfOpen
                        {
                            let mut state_guard = provider.circuit.write().await;
                            *state_guard = CircuitState::HalfOpen;
                        }

                        // Attempt to acquire the single flight probe slot lock-free
                        if provider
                            .probe_in_flight
                            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok()
                        {
                            provider.record_request_timestamp(now).await;
                            return Some(SelectedProvider {
                                name: provider.name.clone(),
                                base_url: provider.base_url.clone(),
                                api_key: provider.api_key.clone(),
                                headers: provider.headers.clone(),
                                is_probe: true,
                            });
                        }
                    }
                    // Still cooling down or another thread stole the HalfOpen probe permit
                    continue;
                },
                CircuitState::HalfOpen => {
                    // Lock-free check: If false, we flip it to true and claim the probe slot.
                    // If true, another concurrent thread is already probing this upstream.
                    // We instantly pass through to protect the upstream from a thundering herd!
                    if provider
                        .probe_in_flight
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        provider.record_request_timestamp(now).await;
                        return Some(SelectedProvider {
                            name: provider.name.clone(),
                            base_url: provider.base_url.clone(),
                            api_key: provider.api_key.clone(),
                            headers: provider.headers.clone(),
                            is_probe: true,
                        });
                    }
                    continue;
                },
            }
        }

        None
    }

    async fn feedback(
        &self,
        provider: &ProviderState,
        success: bool,
        is_probe: bool,
        status_code: Option<u16>,
        retry_after: Option<Duration>,
    ) {
        let now = Instant::now();

        if success {
            let mut failures = provider.consecutive_failures.write().await;
            *failures = 0;

            let mut state = provider.circuit.write().await;
            *state = CircuitState::Closed;

            let mut last_attempt = provider.last_attempt_time.write().await;
            *last_attempt = Some(now);

            let mut rl = provider.rate_limited_until.write().await;
            *rl = None;

            // Deflate the breathing window: a healthy response evaporates the
            // oldest fault so recovery is picked up quickly. A healthy model
            // also forgives severed-strike history.
            provider.record_overload_success().await;
            provider.severed_strikes.store(0, Ordering::Relaxed);

            if is_probe {
                provider.probe_in_flight.store(false, Ordering::SeqCst);
                info!(
                    target: "wirellm_core::router",
                    "HalfOpen probe succeeded for {} — circuit closed",
                    provider.name
                );
            } else {
                info!(
                    target: "wirellm_core::router",
                    "Circuit closed for {} after successful request",
                    provider.name
                );
            }
        } else {
            let mut failures = provider.consecutive_failures.write().await;
            if status_code != Some(429) {
                *failures += 1;
            }

            let mut last_attempt = provider.last_attempt_time.write().await;
            *last_attempt = Some(now);

            let policy = crate::state::current_routing_policy();

            // Handle Failures
            if status_code == Some(429) {
                // Rate-limited: extract or apply default backoff
                let cooldown = retry_after
                    .unwrap_or_else(|| Duration::from_secs(policy.rate_limit_default_cooldown_secs));
                let candidate = now + cooldown;
                let mut rl = provider.rate_limited_until.write().await;
                // Keep the longest window: a Retry-After must not shorten an
                // outstanding overload breath, and vice versa
                if rl.map_or(true, |until| until < candidate) {
                    *rl = Some(candidate);
                }
                info!(
                    target: "wirellm_core::router",
                    "Rate-limited {} for {}s",
                    provider.name, cooldown.as_secs()
                );
            } else if matches!(status_code, Some(404) | Some(410)) {
                // Severed upstream: the model is gone for good (EOL'd, removed
                // or misconfigured). Auto-disable after TWO strikes so a single
                // transient 404 (deploy blip, relay misroute) can't hard-kill
                // the provider; the dashboard highlights it until an admin
                // clears it.
                let strikes = provider.severed_strikes.fetch_add(1, Ordering::Relaxed) + 1;
                if strikes >= 2 {
                    let reason = format!(
                        "model unavailable upstream (HTTP {}, {} strikes)",
                        status_code.unwrap(),
                        strikes
                    );
                    *provider.auto_disabled_reason.write().await = Some(reason);
                    info!(
                        target: "wirellm_core::router",
                        "Auto-disabled {} — {}. Manual re-enable required.",
                        provider.name,
                        provider.auto_disabled_reason.read().await.clone().unwrap_or_default()
                    );
                } else {
                    info!(
                        target: "wirellm_core::router",
                        "Severed response (HTTP {}) from {} — strike {}/2 before auto-disable",
                        status_code.unwrap(), provider.name, strikes
                    );
                }
            } else {
                // Capacity errors (5xx) and connection failures: adaptive
                // breathing room — stop hammering a saturated upstream while
                // still trying again soon.
                let capacity_error = match status_code {
                    Some(c) => c >= 500,
                    None => true,
                };
                if capacity_error {
                    let cooldown = provider.record_overload_fault(now).await;
                    let candidate = now + cooldown;
                    let mut rl = provider.rate_limited_until.write().await;
                    // Never shorten an active skip window (a 429 Retry-After
                    // may be longer than this breath, and it's upstream-authoritative)
                    if rl.map_or(true, |until| until < candidate) {
                        *rl = Some(candidate);
                    }
                    info!(
                        target: "wirellm_core::router",
                        "Overloaded {}; breathing room {}s ({} recent faults)",
                        provider.name,
                        cooldown.as_secs(),
                        provider.fault_history.read().await.len()
                    );
                }

                // Tripped or failed probe: trigger configurable exponential circuit cooldown
                let threshold = policy.circuit_failure_threshold.max(1);
                if *failures >= threshold || is_probe {
                    let exponent = failures.saturating_sub(threshold).min(4);
                    let multiplier = 2u64.pow(exponent);
                    let base = policy.circuit_base_cooldown_secs.max(5);
                    let max_cap = policy.circuit_max_cooldown_secs.max(base);
                    let cooldown = Duration::from_secs((base * multiplier).min(max_cap));

                    let mut state = provider.circuit.write().await;
                    *state = CircuitState::Open {
                        until: now + cooldown,
                    };
                    info!(
                        target: "wirellm_core::router",
                        "Circuit opened for {} for {}s ({} consecutive failures)",
                        provider.name, cooldown.as_secs(), *failures
                    );
                }
            }

            if is_probe {
                provider.probe_in_flight.store(false, Ordering::SeqCst);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Url;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, AtomicU64};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    fn setup_provider(name: &str) -> ProviderState {
        ProviderState {
            name: name.to_string(),
            base_url: Url::parse("https://api.example.com").unwrap(),
            api_key: "test_key".to_string(),
            models: vec!["gpt-4".to_string()],
            headers: std::collections::HashMap::new(),
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(RwLock::new(0)),
            rate_limited_until: Arc::new(RwLock::new(None)),
            last_attempt_time: Arc::new(RwLock::new(None)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
            manual_disabled: AtomicBool::new(false),
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            max_rpm: None,
            min_interval_ms: None,
            max_concurrency: None,
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            request_timestamps: Arc::new(RwLock::new(VecDeque::new())),
            last_request_time: Arc::new(RwLock::new(None)),
            fault_history: Arc::new(RwLock::new(VecDeque::new())),
            last_overload_cooldown_ms: Arc::new(AtomicU64::new(0)),
            diurnal: Arc::new(crate::state::FaultHistogram::new()),
            fault_pattern_store: None,
            auto_disabled_reason: Arc::new(RwLock::new(None)),
            severed_strikes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        }
    }

    #[tokio::test]
    async fn test_router_select_healthy_first() {
        let p1 = setup_provider("prov1");
        let p2 = setup_provider("prov2");
        let candidates = vec![&p1, &p2];

        let strategy = AdaptivePriorityStrategy;
        let selected = strategy.select(&candidates).await.unwrap();

        assert_eq!(selected.name, "prov1");
        assert!(!selected.is_probe);
    }

    #[tokio::test]
    async fn test_router_feedback_success() {
        let p = setup_provider("prov1");
        {
            let mut failures = p.consecutive_failures.write().await;
            *failures = 2;
            let mut circ = p.circuit.write().await;
            *circ = CircuitState::Open {
                until: Instant::now() + Duration::from_secs(60),
            };
        }

        let strategy = AdaptivePriorityStrategy;
        strategy.feedback(&p, true, false, Some(200), None).await;

        assert_eq!(*p.consecutive_failures.read().await, 0);
        assert_eq!(*p.circuit.read().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_router_feedback_failure_trips_circuit() {
        let p = setup_provider("prov1");
        let strategy = AdaptivePriorityStrategy;

        // First failure
        strategy.feedback(&p, false, false, Some(500), None).await;
        assert_eq!(*p.consecutive_failures.read().await, 1);
        assert_eq!(*p.circuit.read().await, CircuitState::Closed);

        // Second failure
        strategy.feedback(&p, false, false, Some(500), None).await;
        assert_eq!(*p.consecutive_failures.read().await, 2);
        assert_eq!(*p.circuit.read().await, CircuitState::Closed);

        // Third failure - should trip
        strategy.feedback(&p, false, false, Some(500), None).await;
        assert_eq!(*p.consecutive_failures.read().await, 3);
        match *p.circuit.read().await {
            CircuitState::Open { .. } => {},
            _ => panic!("Expected Open circuit state"),
        };
    }

    #[tokio::test]
    async fn test_router_feedback_rate_limited() {
        let p = setup_provider("prov1");
        let strategy = AdaptivePriorityStrategy;

        strategy
            .feedback(&p, false, false, Some(429), Some(Duration::from_secs(10)))
            .await;

        assert!(p.rate_limited_until.read().await.is_some());
    }

    #[tokio::test]
    async fn test_router_half_open_single_probe() {
        let p = setup_provider("prov1");
        {
            let mut failures = p.consecutive_failures.write().await;
            *failures = 3;
            // Cooldown expired
            let mut circ = p.circuit.write().await;
            *circ = CircuitState::Open {
                until: Instant::now() - Duration::from_secs(10),
            };
        }

        let strategy = AdaptivePriorityStrategy;
        let candidates = vec![&p];

        // First request should get a probe
        let selected1 = strategy.select(&candidates).await.unwrap();
        assert_eq!(selected1.name, "prov1");
        assert!(selected1.is_probe);
        assert!(p.probe_in_flight.load(Ordering::SeqCst));

        // Second concurrent request should be bypassed since probe is in flight
        let selected2 = strategy.select(&candidates).await;
        assert!(selected2.is_none());

        // Feedback success on probe resolves the circuit
        strategy.feedback(&p, true, true, Some(200), None).await;
        assert_eq!(*p.circuit.read().await, CircuitState::Closed);
        assert!(!p.probe_in_flight.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_router_decay() {
        let p = setup_provider("prov1");
        let decay_interval = crate::state::current_routing_policy().decay_interval_secs;
        {
            let mut failures = p.consecutive_failures.write().await;
            *failures = 4;
            let mut last_attempt = p.last_attempt_time.write().await;
            *last_attempt = Some(Instant::now() - Duration::from_secs(decay_interval * 2 + 20));
        }

        let strategy = AdaptivePriorityStrategy;
        let candidates = vec![&p];

        // Select triggers decay check
        let _ = strategy.select(&candidates).await;

        // Should decay by 2 failures
        assert_eq!(*p.consecutive_failures.read().await, 2);
    }

    #[tokio::test]
    async fn test_router_proactive_rpm_limit() {
        let mut p1 = setup_provider("prov1");
        p1.max_rpm = Some(2); // Limit to 2 RPM
        let p2 = setup_provider("prov2");
        let candidates = vec![&p1, &p2];
        let strategy = AdaptivePriorityStrategy;

        // Request 1: hits prov1
        let s1 = strategy.select(&candidates).await.expect("Select candidate 1");
        assert_eq!(s1.name, "prov1");

        // Request 2: hits prov1 (at capacity 2/2)
        let s2 = strategy.select(&candidates).await.expect("Select candidate 2");
        assert_eq!(s2.name, "prov1");

        // Request 3: prov1 is at 2/2 RPM, proactively falls over to prov2!
        let s3 = strategy.select(&candidates).await.expect("Select candidate 3");
        assert_eq!(s3.name, "prov2");
    }

    #[tokio::test]
    async fn test_router_proactive_burst_limit() {
        let mut p1 = setup_provider("prov1");
        p1.min_interval_ms = Some(2000); // 2-second spacing required
        let p2 = setup_provider("prov2");
        let candidates = vec![&p1, &p2];
        let strategy = AdaptivePriorityStrategy;

        // Request 1: hits prov1, which records last_request_time
        let s1 = strategy.select(&candidates).await.expect("Select candidate 1");
        assert_eq!(s1.name, "prov1");

        // Request 2 (fired immediately after): prov1 is burst-limited, so it proactively falls over to prov2!
        let s2 = strategy.select(&candidates).await.expect("Select candidate 2");
        assert_eq!(s2.name, "prov2");
    }

    #[tokio::test]
    async fn test_overload_breathing_room_grows_under_burst() {
        let p = setup_provider("prov1");
        let strategy = AdaptivePriorityStrategy;

        // Rapid 503 burst: each fault gets a deeper breathing window than the last
        let mut last = 0u64;
        for expected_min in [10u64, 20, 35] {
            strategy.feedback(&p, false, false, Some(503), None).await;
            let secs = p.last_overload_cooldown_ms.load(Ordering::Relaxed) / 1000;
            assert!(secs >= expected_min, "expected >= {}s, got {}s", expected_min, secs);
            assert!(secs > last, "breathing room must grow under burst: {} -> {}", last, secs);
            assert!(secs <= 300, "must never exceed the 5-minute cap");
            last = secs;
        }

        // Selection must skip the provider while breathing
        let candidates = vec![&p];
        assert!(strategy.select(&candidates).await.is_none());
    }

    #[tokio::test]
    async fn test_diurnal_busy_hour_extends_first_fault() {
        let p = setup_provider("prov1");
        let strategy = AdaptivePriorityStrategy;

        // Simulate a historically busy UTC hour (3 faults already known)
        let hour = crate::state::utc_hour_of_day();
        p.diurnal.bump(hour);
        p.diurnal.bump(hour);
        p.diurnal.bump(hour);

        // First fault in that hour breathes deeper than the 10s floor
        // ("historical" excludes the fault being recorded)
        strategy.feedback(&p, false, false, Some(500), None).await;
        let secs = p.last_overload_cooldown_ms.load(Ordering::Relaxed) / 1000;
        assert!(secs > 10, "busy hour must breathe deeper than the floor: {}s", secs);
        assert!(secs <= 300);
    }

    #[tokio::test]
    async fn test_quiet_hour_first_fault_gets_exact_floor() {
        let p = setup_provider("prov1");
        let strategy = AdaptivePriorityStrategy;

        // No history in this hour → the very first fault gets exactly the base floor
        strategy.feedback(&p, false, false, Some(500), None).await;
        let secs = p.last_overload_cooldown_ms.load(Ordering::Relaxed) / 1000;
        let base = crate::config::DEFAULT_ROUTING_POLICY.overload_base_cooldown_secs;
        assert_eq!(secs, base, "quiet hour first fault must be exactly the base floor");
    }

    #[tokio::test]
    async fn test_diurnal_fades_over_elapsed_hours() {
        let p = setup_provider("prov1");
        let hour_now = crate::state::utc_epoch_hour();
        let bucket = hour_now % 24;

        p.diurnal.bump(bucket);
        p.diurnal.bump(bucket);
        p.diurnal.bump(bucket);
        assert_eq!(p.diurnal.get(bucket), 3);

        // A past epoch hour must NOT rewind or decay
        p.diurnal.tick_and_decay(hour_now.saturating_sub(2));
        assert_eq!(p.diurnal.get(bucket), 3, "past ticks must not decay");

        // A future epoch hour triggers catch-up decay (2 hours: 3 → 2 → 1)
        p.diurnal.tick_and_decay(hour_now + 2);
        assert!(p.diurnal.get(bucket) < 3, "future tick must decay: {}", p.diurnal.get(bucket));
        assert!(p.diurnal.get(bucket) >= 1, "must not over-decay: {}", p.diurnal.get(bucket));
    }

    #[tokio::test]
    async fn test_breathing_never_shortens_auth_window() {
        let p = setup_provider("prov1");
        let strategy = AdaptivePriorityStrategy;

        // Upstream-authoritative 429 with a long Retry-After
        strategy
            .feedback(&p, false, false, Some(429), Some(Duration::from_secs(60)))
            .await;
        let long_window = p.rate_limited_until.read().await.unwrap();

        // A 503 must NOT shorten the outstanding window
        strategy.feedback(&p, false, false, Some(503), None).await;
        let after = p.rate_limited_until.read().await.unwrap();
        assert!(
            after >= long_window,
            "5xx breathing must not shorten an active Retry-After window"
        );
    }

    #[tokio::test]
    async fn test_client_errors_do_not_breathe() {
        let p = setup_provider("prov1");
        let strategy = AdaptivePriorityStrategy;

        // 400 Bad Request is not a capacity signal: no breathing, no faults recorded
        strategy.feedback(&p, false, false, Some(400), None).await;
        assert!(p.rate_limited_until.read().await.is_none());
        assert!(p.fault_history.read().await.is_empty());
        assert!(p.auto_disabled_reason.read().await.is_none());
    }

    #[tokio::test]
    async fn test_severed_requires_two_strikes() {
        let p1 = setup_provider("dead");
        let p2 = setup_provider("fallback");
        let candidates = vec![&p1, &p2];
        let strategy = AdaptivePriorityStrategy;

        // First 404: strike 1 — provider stays routable (transient blips happen)
        strategy.feedback(&p1, false, false, Some(404), None).await;
        assert!(p1.auto_disabled_reason.read().await.is_none());
        assert_eq!(strategy.select(&candidates).await.unwrap().name, "dead");

        // Second 404: strike 2 — hard auto-disable, instant failover
        strategy.feedback(&p1, false, false, Some(404), None).await;
        assert!(p1.auto_disabled_reason.read().await.is_some());
        assert_eq!(strategy.select(&candidates).await.unwrap().name, "fallback");
    }

    #[tokio::test]
    async fn test_severed_model_auto_disables_provider() {
        let p1 = setup_provider("dead");
        let p2 = setup_provider("fallback");
        let candidates = vec![&p1, &p2];
        let strategy = AdaptivePriorityStrategy;

        // Upstream EOL'd the model (HTTP 410)
        strategy.feedback(&p1, false, false, Some(410), None).await;
        strategy.feedback(&p1, false, false, Some(410), None).await;

        let reason = p1.auto_disabled_reason.read().await.clone();
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("410"));

        // Routing skips the severed provider entirely
        let selected = strategy.select(&candidates).await.unwrap();
        assert_eq!(selected.name, "fallback");
    }

    #[tokio::test]
    async fn test_success_clears_fault_window() {
        let p = setup_provider("prov1");
        let strategy = AdaptivePriorityStrategy;

        strategy.feedback(&p, false, false, Some(503), None).await;
        strategy.feedback(&p, false, false, Some(503), None).await;
        assert_eq!(p.fault_history.read().await.len(), 2);

        // A healthy 200 evaporates the WHOLE burst window — the next single
        // blip must be judged on its own, not on stale pre-recovery faults.
        strategy.feedback(&p, true, false, Some(200), None).await;
        assert_eq!(p.fault_history.read().await.len(), 0);
        assert!(p.rate_limited_until.read().await.is_none());
    }

    #[tokio::test]
    async fn test_http_429_never_counts_toward_circuit() {
        let p = setup_provider("prov1");
        let strategy = AdaptivePriorityStrategy;

        // Hammer the provider with rate-limit responses: it must stay CLOSED
        // (429 is upstream throttling, not a health failure) and only carry a
        // rate-limit window.
        for _ in 0..10 {
            strategy
                .feedback(&p, false, false, Some(429), Some(Duration::from_secs(5)))
                .await;
        }

        assert_eq!(
            *p.consecutive_failures.read().await,
            0,
            "429 must never increment the circuit failure counter"
        );
        assert!(
            matches!(*p.circuit.read().await, CircuitState::Closed),
            "429 must never trip the circuit breaker"
        );
        assert!(p.rate_limited_until.read().await.is_some());
    }

    #[tokio::test]
    async fn test_circuit_opens_at_threshold_with_capped_base() {
        let p = setup_provider("prov1");
        let strategy = AdaptivePriorityStrategy;
        let policy = crate::config::DEFAULT_ROUTING_POLICY;

        // Drive enough hard 5xx to trip the breaker.
        for _ in 0..policy.circuit_failure_threshold {
            strategy.feedback(&p, false, false, Some(503), None).await;
        }

        let state = *p.circuit.read().await;
        match state {
            CircuitState::Open { until } => {
                let left = until.saturating_duration_since(Instant::now()).as_secs();
                assert!(
                    left <= policy.circuit_base_cooldown_secs,
                    "initial circuit cooldown must be capped at the base ({}s), got {}s",
                    policy.circuit_base_cooldown_secs,
                    left
                );
                assert!(left > 0, "circuit must actually be open");
            },
            other => panic!("expected Open circuit at threshold, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_auto_disabled_provider_never_selected() {
        let p = setup_provider("prov1");
        *p.auto_disabled_reason.write().await = Some("model unavailable upstream (HTTP 404)".to_string());

        let strategy = AdaptivePriorityStrategy;
        let candidates = vec![&p];
        assert!(strategy.select(&candidates).await.is_none());
    }

    /// GUARANTEE: ladder order is strictly preserved. Selection must walk the
    /// candidate slice exactly top-down; a smart higher-priority provider that
    /// is breathing must be skipped IN INDEX ORDER, never re-sorted ahead by
    /// smartness/latency/any weight.
    #[tokio::test]
    async fn test_strict_ladder_order_under_breathing() {
        // prov_dumb is LAST in the ladder but has the LOWEST smartness — if
        // anything ever reorders by smartness/weights this test fails loud.
        let p1 = setup_provider("rung1");
        let p2 = setup_provider("rung2");
        let p3 = setup_provider("rung3");
        let p4 = setup_provider("rung4_last_smallest");
        let candidates = vec![&p1, &p2, &p3, &p4];
        let strategy = AdaptivePriorityStrategy;

        // Healthy: strictly rung1 first
        assert_eq!(strategy.select(&candidates).await.unwrap().name, "rung1");

        // rung1 gets a breathing window (503)
        strategy.feedback(&p1, false, false, Some(503), None).await;
        assert_eq!(strategy.select(&candidates).await.unwrap().name, "rung2");

        // rung2 cooling too → must land on rung3, exactly in order
        strategy.feedback(&p2, false, false, Some(503), None).await;
        assert_eq!(strategy.select(&candidates).await.unwrap().name, "rung3");

        // Windows expire without a new failure → rung1 is first again
        *p1.rate_limited_until.write().await = None;
        *p2.rate_limited_until.write().await = None;
        assert_eq!(strategy.select(&candidates).await.unwrap().name, "rung1");
    }
}

