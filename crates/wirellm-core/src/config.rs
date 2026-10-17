use crate::error::{OxllmError, Result};
use reqwest::header::{HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub otel_endpoint: String,
    #[serde(default = "default_upstream_timeout")]
    pub upstream_timeout_secs: u64,
    #[serde(default = "default_bind_family")]
    pub bind_family: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutingPolicy {
    /// Per-attempt HTTP response-header timeout for streaming requests (seconds).
    pub stream_header_timeout_secs: u64,
    /// Per-attempt HTTP response-header timeout for non-streaming requests (seconds).
    pub nonstream_header_timeout_secs: u64,
    /// Rolling window used to count recent overload faults (seconds).
    pub fault_window_secs: u64,
    /// Base breathing room applied after the 1st overload fault (seconds).
    pub overload_base_cooldown_secs: u64,
    /// Maximum cap on any single overload breathing window (seconds).
    pub overload_max_cooldown_secs: u64,
    /// Consecutive non-429 failures required to trip the circuit breaker.
    pub circuit_failure_threshold: u32,
    /// Base circuit breaker open cooldown on initial trip (seconds).
    pub circuit_base_cooldown_secs: u64,
    /// Maximum cap on circuit breaker open cooldown (seconds).
    pub circuit_max_cooldown_secs: u64,
    /// Default fallback cooldown on HTTP 429 when upstream omits Retry-After (seconds).
    pub rate_limit_default_cooldown_secs: u64,
    /// Idle duration required to decay 1 consecutive failure (seconds).
    pub decay_interval_secs: u64,
    /// First auto-recovery delay after a severed (HTTP 404/410) model
    /// auto-disables a provider (seconds). The provider is skipped for this
    /// window, then lazily re-admitted for a fresh probe — so a "listed but
    /// unroutable" model (e.g. a temporarily over-capacity pool) rejoins on its
    /// own instead of waiting for a human.
    #[serde(default = "default_severed_quarantine_secs")]
    pub severed_quarantine_secs: u64,
    /// Ceiling on the escalating severed quarantine (seconds). Each repeat
    /// severance without an intervening success stretches the window until it
    /// hits this cap, so a genuinely-retired model settles into rare re-probes.
    #[serde(default = "default_severed_quarantine_max_secs")]
    pub severed_quarantine_max_secs: u64,
    /// Multiplier applied to the severed quarantine on each repeat severance
    /// (integer ≥ 1; 2 → base, 2×base, 4×base, … capped at the max).
    #[serde(default = "default_severed_quarantine_backoff")]
    pub severed_quarantine_backoff: u32,
}

fn default_severed_quarantine_secs() -> u64 {
    DEFAULT_ROUTING_POLICY.severed_quarantine_secs
}
fn default_severed_quarantine_max_secs() -> u64 {
    DEFAULT_ROUTING_POLICY.severed_quarantine_max_secs
}
fn default_severed_quarantine_backoff() -> u32 {
    DEFAULT_ROUTING_POLICY.severed_quarantine_backoff
}

/// Canonical, compile-time default routing policy — the single source of truth.
/// The runtime atomics (`state.rs`), the DB fallback, and the dashboard's
/// default form all derive from these numbers; never re-type them elsewhere.
pub const DEFAULT_ROUTING_POLICY: RoutingPolicy = RoutingPolicy {
    stream_header_timeout_secs: 22,
    // Non-streaming upstreams send response headers only AFTER the full
    // completion is generated, so this must cover whole-generation time (the
    // old `upstream_timeout_secs` behaviour), NOT act as a first-byte cap.
    // Default matches the shipped upstream timeout, so it is a no-op unless an
    // operator deliberately lowers it to force faster non-stream cascades.
    nonstream_header_timeout_secs: 300,
    fault_window_secs: 180,
    overload_base_cooldown_secs: 15,
    overload_max_cooldown_secs: 90,
    circuit_failure_threshold: 3,
    circuit_base_cooldown_secs: 30,
    circuit_max_cooldown_secs: 120,
    rate_limit_default_cooldown_secs: 10,
    decay_interval_secs: 180,
    // A severed model is quarantined for 1h, then lazily re-probed. Repeated
    // severances without a recovery double the window (1h → 2h → 4h → 6h cap)
    // so a flaky pool returns quickly while a truly-retired model is only
    // re-probed a handful of times per day.
    severed_quarantine_secs: 3600,
    severed_quarantine_max_secs: 21600,
    severed_quarantine_backoff: 2,
};

impl Default for RoutingPolicy {
    fn default() -> Self {
        DEFAULT_ROUTING_POLICY
    }
}

impl RoutingPolicy {
    /// Enforces safe lower bounds on every field. Call after applying any
    /// external patch so a value of 0 (or a window shorter than a single
    /// request) can never stall or livelock the router. Also the last line of
    /// defence inside `state::apply_routing_policy`.
    pub fn clamp(&mut self) {
        self.stream_header_timeout_secs = self.stream_header_timeout_secs.max(3);
        self.nonstream_header_timeout_secs = self.nonstream_header_timeout_secs.max(5);
        self.fault_window_secs = self.fault_window_secs.max(10);
        self.overload_base_cooldown_secs = self.overload_base_cooldown_secs.max(1);
        self.overload_max_cooldown_secs = self.overload_max_cooldown_secs.max(5);
        self.circuit_failure_threshold = self.circuit_failure_threshold.max(1);
        self.circuit_base_cooldown_secs = self.circuit_base_cooldown_secs.max(5);
        self.circuit_max_cooldown_secs = self.circuit_max_cooldown_secs.max(10);
        self.rate_limit_default_cooldown_secs = self.rate_limit_default_cooldown_secs.max(1);
        self.decay_interval_secs = self.decay_interval_secs.max(15);
        // A severed quarantine of 0s would thrash upstream with 404s; a backoff
        // < 1 would never grow the window. The max is raised to at least the
        // base by `state::severed_quarantine_secs`, so a 1s floor here is safe.
        self.severed_quarantine_secs = self.severed_quarantine_secs.max(1);
        self.severed_quarantine_max_secs = self.severed_quarantine_max_secs.max(1);
        self.severed_quarantine_backoff = self.severed_quarantine_backoff.max(1);
    }
}

/// Marker lines bracketing the auto-managed routing-policy block in the config
/// file. They are `#` comments so they are inert in both YAML and TOML; the
/// block between them is rewritten on every policy change, everything else in
/// the file (including all other comments) is left untouched.
pub const ROUTING_POLICY_MARKER_START: &str =
    "# >>> wirellm:routing_policy (auto-managed — edits inside are overwritten) >>>";
pub const ROUTING_POLICY_MARKER_END: &str = "# <<< wirellm:routing_policy <<<";

fn is_yaml_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map_or(false, |e| e.eq_ignore_ascii_case("yaml") || e.eq_ignore_ascii_case("yml"))
}

/// Renders the `routing_policy` block (WITHOUT the surrounding markers) in the
/// file's native syntax.
fn render_routing_policy_block(policy: &RoutingPolicy, yaml: bool) -> Result<String> {
    if yaml {
        let mut map = serde_yaml::Mapping::new();
        map.insert(
            serde_yaml::Value::String("routing_policy".to_string()),
            serde_yaml::to_value(policy)
                .map_err(|e| OxllmError::Internal(format!("YAML render failed: {}", e)))?,
        );
        serde_yaml::to_string(&serde_yaml::Value::Mapping(map))
            .map_err(|e| OxllmError::Internal(format!("YAML render failed: {}", e)))
    } else {
        let body = toml::to_string(policy)
            .map_err(|e| OxllmError::Internal(format!("TOML render failed: {}", e)))?;
        Ok(format!("[routing_policy]\n{}", body))
    }
}

/// True if the file already declares `routing_policy` OUTSIDE our managed block.
/// Appending a second top-level key would be a duplicate key — invalid YAML
/// (serde_yaml errors) / invalid TOML — so we must refuse to write in that case.
fn has_unmanaged_declaration(content: &str) -> bool {
    content.lines().any(|line| {
        let t = line.trim();
        t.starts_with("routing_policy:") || t == "[routing_policy]"
    })
}

/// Replaces the marker-delimited managed block, or appends a fresh one. Returns
/// `None` when the file already has an unmanaged `routing_policy` declaration
/// (caller must not write — it would corrupt the file).
fn upsert_managed_block(content: &str, block: &str) -> Option<String> {
    let managed = format!(
        "{}\n{}\n{}\n",
        ROUTING_POLICY_MARKER_START,
        block.trim_end(),
        ROUTING_POLICY_MARKER_END
    );

    if let (Some(s), Some(e)) = (
        content.find(ROUTING_POLICY_MARKER_START),
        content.find(ROUTING_POLICY_MARKER_END),
    ) {
        if e > s {
            // Guard the REPLACE path too: an operator may have hand-added a
            // second `routing_policy` key outside our block. Splicing ours back
            // in would leave a duplicate top-level key — invalid YAML/TOML that
            // the daemon then fails to parse on the next boot.
            let before = &content[..s];
            let after = &content[e + ROUTING_POLICY_MARKER_END.len()..];
            if has_unmanaged_declaration(before) || has_unmanaged_declaration(after) {
                return None;
            }

            let mut out = String::with_capacity(content.len() + managed.len());
            out.push_str(before);
            out.push_str(&managed);
            // Skip the old block up to and including the end-marker's newline.
            let after = after.strip_prefix('\r').unwrap_or(after);
            out.push_str(after.strip_prefix('\n').unwrap_or(after));
            return Some(out);
        }
    }

    if has_unmanaged_declaration(content) {
        return None;
    }

    let mut out = content.to_string();
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&managed);
    Some(out)
}

/// True if the file declares a server block (required by `Config`). Used to
/// refuse writing a routing-policy-only config that the daemon couldn't parse.
fn has_server_section(content: &str) -> bool {
    content.lines().any(|line| {
        let t = line.trim();
        t == "[server]" || t.starts_with("server:")
    })
}

/// Writes `policy` into the managed block of the given config file (YAML or
/// TOML), preserving everything else. Atomic (temp + rename). Returns the path
/// written. Errors (read-only FS, unmanaged duplicate key) are the caller's to
/// log — the live policy is already applied regardless.
pub fn write_routing_policy_to_file(path: &Path, policy: &RoutingPolicy) -> Result<PathBuf> {
    let yaml = is_yaml_path(path);
    let content = if path.exists() {
        fs::read_to_string(path).map_err(|e| {
            OxllmError::ConfigLoad(format!("Failed to read config for write-back: {}", e))
        })?
    } else {
        String::new()
    };

    // Never create a routing-policy-only config: `Config.server` has no serde
    // default, so a file without a server section would fail to parse on boot.
    if !has_server_section(&content) {
        return Err(OxllmError::ConfigLoad(format!(
            "{} has no [server]/server: section — refusing to write a routing-policy-only config",
            path.display()
        )));
    }

    let block = render_routing_policy_block(policy, yaml)?;
    let updated = upsert_managed_block(&content, &block).ok_or_else(|| {
        OxllmError::ConfigLoad(format!(
            "{} already declares an unmanaged `routing_policy`; refusing to append a duplicate. \
             Remove it (or let WireLLM own it) and retry.",
            path.display()
        ))
    })?;

    // Preserve the original file mode (the config can hold secrets like
    // auth.session_secret / Google client_secret — don't widen it to 0644).
    let original_perms = fs::metadata(path).ok().map(|m| m.permissions());

    // Atomic replace so a crash mid-write can never leave a half-written config.
    // The temp name is unique per call (pid + nanos) so concurrent updates can
    // never clobber each other's scratch file.
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = path.with_extension(format!("tmp.{}.{}", std::process::id(), unique));
    fs::write(&tmp, updated)
        .map_err(|e| OxllmError::ConfigLoad(format!("Failed to write config temp file: {}", e)))?;
    if let Some(perms) = original_perms {
        let _ = fs::set_permissions(&tmp, perms);
    }
    fs::rename(&tmp, path)
        .map_err(|e| OxllmError::ConfigLoad(format!("Failed to replace config file: {}", e)))?;
    Ok(path.to_path_buf())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub enabled: bool,
    pub base_url: String,
    pub api_key: String,
    pub models: Vec<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub rpm: Option<u32>,
    #[serde(default)]
    pub min_interval_ms: Option<u64>,
    #[serde(default)]
    pub max_concurrency: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualModelTarget {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GoogleAuthConfig {
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub allowed_emails: Vec<String>,
    #[serde(default)]
    pub redirect_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AuthConfig {
    /// Token access: list of valid API keys for /v1/*, /mcp/*, and /api/*.
    /// If empty, API access is UNPROTECTED.
    #[serde(default)]
    pub api_keys: Vec<String>,

    /// Google OAuth configuration to protect the UI.
    /// If client_id is None or empty, Google Auth is DISABLED (UI is unprotected).
    #[serde(default)]
    pub google: Option<GoogleAuthConfig>,

    /// Secret key used to sign session cookies.
    /// If not provided, a random secret is generated.
    #[serde(default)]
    pub session_secret: Option<String>,

    /// Public origin of the application (e.g. "https://llm.example.com").
    /// Used to build absolute OAuth redirect URIs.
    #[serde(default)]
    pub public_url: Option<String>,
}

impl AuthConfig {
    /// Returns true if API access requires token authentication.
    pub fn is_api_auth_required(&self) -> bool {
        !self.api_keys.is_empty()
    }

    /// Returns true if Google OAuth is configured and enabled for UI protection.
    pub fn is_google_auth_enabled(&self) -> bool {
        self.google
            .as_ref()
            .and_then(|g| g.client_id.as_ref())
            .map(|id| !id.trim().is_empty())
            .unwrap_or(false)
    }

    /// Enriches configuration from standard environment variables if not already set.
    pub fn merge_env_vars(&mut self) {
        if let Ok(keys) = std::env::var("WIRELLM_API_KEYS").or_else(|_| std::env::var("WIRELLM_API_KEY")) {
            for k in keys.split(',') {
                let trimmed = k.trim().to_string();
                if !trimmed.is_empty() && !self.api_keys.contains(&trimmed) {
                    self.api_keys.push(trimmed);
                }
            }
        }
        if let Ok(cid) = std::env::var("WIRELLM_GOOGLE_CLIENT_ID").or_else(|_| std::env::var("GOOGLE_CLIENT_ID")) {
            let google = self.google.get_or_insert_with(Default::default);
            if google.client_id.is_none() && !cid.trim().is_empty() {
                google.client_id = Some(cid.trim().to_string());
            }
        }
        if let Ok(csec) = std::env::var("WIRELLM_GOOGLE_CLIENT_SECRET").or_else(|_| std::env::var("GOOGLE_CLIENT_SECRET")) {
            let google = self.google.get_or_insert_with(Default::default);
            if google.client_secret.is_none() && !csec.trim().is_empty() {
                google.client_secret = Some(csec.trim().to_string());
            }
        }
        if let Ok(emails) = std::env::var("WIRELLM_GOOGLE_ALLOWED_EMAILS") {
            let google = self.google.get_or_insert_with(Default::default);
            for e in emails.split(',') {
                let trimmed = e.trim().to_string();
                if !trimmed.is_empty() && !google.allowed_emails.contains(&trimmed) {
                    google.allowed_emails.push(trimmed);
                }
            }
        }
        if self.session_secret.is_none() {
            if let Ok(sec) = std::env::var("WIRELLM_AUTH_SESSION_SECRET").or_else(|_| std::env::var("AUTH_SESSION_SECRET")) {
                if !sec.trim().is_empty() {
                    self.session_secret = Some(sec.trim().to_string());
                }
            }
        }
        if self.public_url.is_none() {
            if let Ok(url) = std::env::var("WIRELLM_PUBLIC_URL").or_else(|_| std::env::var("APP_ORIGIN")) {
                if !url.trim().is_empty() {
                    self.public_url = Some(url.trim().to_string());
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub virtual_models: HashMap<String, Vec<VirtualModelTarget>>,
    /// Optional routing policy persisted in the config file. When present it is
    /// the authority at startup (and is mirrored into SQLite), so operators can
    /// tune the router from the config file as well as from the dashboard/MCP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing_policy: Option<RoutingPolicy>,
}

impl Config {
    /// Loads a YAML or TOML configuration file and expands environment variables of style `${VAR_NAME}`.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let p = path.as_ref();
        if !p.exists() {
            let default_starter = r#"server:
  host: "0.0.0.0"
  port: 8765
  otel_endpoint: "http://127.0.0.1:4318"
  upstream_timeout_secs: 30
  bind_family: "ipv4"
"#;
            if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    let _ = fs::create_dir_all(parent);
                }
            }
            let _ = fs::write(p, default_starter);
        }

        let content = fs::read_to_string(p)
            .map_err(|e| OxllmError::ConfigLoad(format!("Failed to read config file: {}", e)))?;

        let expanded = expand_env_vars(&content)?;

        let is_yaml = p.extension().map_or(false, |ext| ext == "yaml" || ext == "yml");
        if is_yaml {
            serde_yaml::from_str(&expanded)
                .map_err(|e| OxllmError::ConfigLoad(format!("Failed to parse YAML config: {}", e)))
        } else {
            match toml::from_str(&expanded) {
                Ok(c) => Ok(c),
                Err(toml_err) => {
                    serde_yaml::from_str(&expanded).map_err(|yaml_err| {
                        OxllmError::ConfigLoad(format!(
                            "Failed to parse config as TOML ({}) or YAML ({})",
                            toml_err, yaml_err
                        ))
                    })
                }
            }
        }
    }

    /// Validates the configuration syntax and cross-references virtual models with defined providers.
    pub fn validate(&self) -> Result<()> {
        let provider_map: HashMap<&str, &ProviderConfig> = self
            .providers
            .iter()
            .map(|p| (p.name.as_str(), p))
            .collect();

        // 1. If no providers are configured yet, allow starting so users can configure via UI or API
        if self.providers.is_empty() {
            return Ok(());
        }

        // 2. Validate custom provider headers
        for provider in &self.providers {
            for (name, val) in &provider.headers {
                HeaderName::try_from(name.as_str()).map_err(|e| {
                    OxllmError::ConfigLoad(format!(
                        "Invalid header name '{}' in provider '{}': {}",
                        name, provider.name, e
                    ))
                })?;
                HeaderValue::try_from(val.as_str()).map_err(|e| {
                    OxllmError::ConfigLoad(format!(
                        "Invalid header value for '{}' in provider '{}': {}",
                        name, provider.name, e
                    ))
                })?;
            }
        }

        // 3. Validate that virtual models target existing, enabled providers
        for (vm_name, targets) in &self.virtual_models {
            if targets.is_empty() {
                return Err(OxllmError::ConfigLoad(format!(
                    "Virtual model '{}' has no targets configured",
                    vm_name
                )));
            }

            for target in targets {
                match provider_map.get(target.provider.as_str()) {
                    Some(provider) => {
                        if !provider.enabled {
                            // Warn or skip: we allow referencing disabled providers,
                            // but the virtual model resolution loop will bypass them.
                        }
                    },
                    None => {
                        return Err(OxllmError::ConfigLoad(format!(
                            "Virtual model '{}' targets undefined provider '{}'",
                            vm_name, target.provider
                        )));
                    },
                }
            }
        }

        Ok(())
    }
}

fn default_port() -> u16 {
    8080
}

fn default_upstream_timeout() -> u64 {
    5
}

fn default_bind_family() -> String {
    "ipv4".to_string()
}

/// Helper function to perform Unix shell-style `${VAR_NAME}` environment variable expansions.
#[allow(clippy::while_let_on_iterator)]
pub fn expand_env_vars(raw_content: &str) -> Result<String> {
    let mut expanded = String::new();
    let mut chars = raw_content.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        if ch == '$' {
            if let Some(&(_, '{')) = chars.peek() {
                chars.next(); // Consume '{'

                let mut var_name = String::new();
                let mut found_close = false;

                while let Some((_, var_ch)) = chars.next() {
                    if var_ch == '}' {
                        found_close = true;
                        break;
                    }
                    var_name.push(var_ch);
                }

                if !found_close {
                    return Err(OxllmError::ConfigLoad(format!(
                        "Unclosed environment variable placeholder starting at index {}",
                        idx
                    )));
                }

                // Strictly resolve environment variable with common alias fallbacks
                let val = std::env::var(&var_name)
                    .or_else(|_| match var_name.as_str() {
                        "NVIDIA_API_KEY" => std::env::var("NVIDIA_TOKEN"),
                        "NVIDIA_TOKEN" => std::env::var("NVIDIA_API_KEY"),
                        "OPENROUTER_API_KEY" => std::env::var("OPENROUTER_TOKEN"),
                        "OPENROUTER_TOKEN" => std::env::var("OPENROUTER_API_KEY"),
                        "GEMINI_API_KEY" => std::env::var("GOOGLE_API_KEY"),
                        "GOOGLE_API_KEY" => std::env::var("GEMINI_API_KEY"),
                        _ => Err(std::env::VarError::NotPresent),
                    })
                    .map_err(|_| OxllmError::EnvVarMissing(var_name.clone()))?;

                expanded.push_str(&val);
                continue;
            }
        }
        expanded.push(ch);
    }

    Ok(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_env_vars_success() {
        std::env::set_var("TEST_HOST", "127.0.0.1");
        std::env::set_var("TEST_KEY", "groq-key-123");

        let input = r#"
            host = "${TEST_HOST}"
            api_key = "${TEST_KEY}"
            other = "literal $10 string"
        "#;

        let result = expand_env_vars(input).unwrap();
        assert!(result.contains("host = \"127.0.0.1\""));
        assert!(result.contains("api_key = \"groq-key-123\""));
        assert!(result.contains("other = \"literal $10 string\""));
    }

    #[test]
    fn test_expand_env_vars_missing() {
        std::env::remove_var("MISSING_VAR_XYZ");
        let input = r#"api_key = "${MISSING_VAR_XYZ}""#;
        let result = expand_env_vars(input);
        assert!(
            matches!(result, Err(OxllmError::EnvVarMissing(ref name)) if name == "MISSING_VAR_XYZ")
        );
    }

    #[test]
    fn test_expand_env_vars_unclosed() {
        let input = r#"api_key = "${UNCLOSED"#;
        let result = expand_env_vars(input);
        assert!(matches!(result, Err(OxllmError::ConfigLoad(_))));
    }

    #[test]
    fn test_provider_headers_parsing_and_env_expansion() {
        std::env::set_var("TEST_SESSION_ID", "sess-xyz-789");
        let toml_str = r#"
            [server]
            host = "127.0.0.1"
            port = 8080
            otel_endpoint = "http://localhost:4318"

            [[providers]]
            name = "opencode-go"
            enabled = true
            base_url = "https://opencode.ai/zen/go/v1/"
            api_key = "key123"
            models = ["longcat-2.5-preview-free"]
            headers = { "User-Agent" = "opencode/1.19.0", "x-opencode-session" = "${TEST_SESSION_ID}" }

            [virtual_models]
            "chat" = [{ provider = "opencode-go", model = "longcat-2.5-preview-free" }]
        "#;

        let expanded = expand_env_vars(toml_str).unwrap();
        let config: Config = toml::from_str(&expanded).unwrap();
        assert_eq!(config.providers.len(), 1);
        assert_eq!(
            config.providers[0].headers.get("User-Agent").unwrap(),
            "opencode/1.19.0"
        );
        assert_eq!(
            config.providers[0]
                .headers
                .get("x-opencode-session")
                .unwrap(),
            "sess-xyz-789"
        );
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_provider_headers_invalid_name() {
        let toml_str = r#"
            [server]
            host = "127.0.0.1"
            port = 8080
            otel_endpoint = "http://localhost:4318"

            [[providers]]
            name = "bad-headers"
            enabled = true
            base_url = "https://example.com/v1"
            api_key = "key"
            models = ["model-1"]
            headers = { "Invalid Name Spaces" = "val" }

            [virtual_models]
            "test" = [{ provider = "bad-headers", model = "model-1" }]
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        let val_res = config.validate();
        assert!(val_res.is_err());
        assert!(val_res
            .unwrap_err()
            .to_string()
            .contains("Invalid header name"));
    }

    #[test]
    fn test_routing_policy_default_is_the_canonical_const() {
        // The Default impl and the runtime atomics must both read the single
        // source of truth — this guards against a second, drifting copy.
        assert_eq!(RoutingPolicy::default(), DEFAULT_ROUTING_POLICY);
    }

    #[test]
    fn test_routing_policy_clamp_enforces_floors() {
        // A hostile/zeroed patch must be lifted to safe minimums, never stored
        // as-is (a 0s timeout or 0-fault circuit would deadlock routing).
        let mut p = RoutingPolicy {
            stream_header_timeout_secs: 0,
            nonstream_header_timeout_secs: 0,
            fault_window_secs: 0,
            overload_base_cooldown_secs: 0,
            overload_max_cooldown_secs: 0,
            circuit_failure_threshold: 0,
            circuit_base_cooldown_secs: 0,
            circuit_max_cooldown_secs: 0,
            rate_limit_default_cooldown_secs: 0,
            decay_interval_secs: 0,
            severed_quarantine_secs: 0,
            severed_quarantine_max_secs: 0,
            severed_quarantine_backoff: 0,
        };
        p.clamp();

        assert_eq!(p.stream_header_timeout_secs, 3);
        assert_eq!(p.nonstream_header_timeout_secs, 5);
        assert_eq!(p.fault_window_secs, 10);
        assert_eq!(p.overload_base_cooldown_secs, 1);
        assert_eq!(p.overload_max_cooldown_secs, 5);
        assert_eq!(p.circuit_failure_threshold, 1);
        assert_eq!(p.circuit_base_cooldown_secs, 5);
        assert_eq!(p.circuit_max_cooldown_secs, 10);
        assert_eq!(p.rate_limit_default_cooldown_secs, 1);
        assert_eq!(p.decay_interval_secs, 15);
        assert_eq!(p.severed_quarantine_secs, 1);
        assert_eq!(p.severed_quarantine_max_secs, 1);
        assert_eq!(p.severed_quarantine_backoff, 1);

        // Values ABOVE the floors must be preserved untouched.
        let mut q = DEFAULT_ROUTING_POLICY;
        q.clamp();
        assert_eq!(q, DEFAULT_ROUTING_POLICY);
    }

    #[test]
    fn test_routing_policy_serde_roundtrip() {
        // The policy is persisted as JSON in server_settings; a round-trip must
        // be lossless or a daemon restart would silently reset tuning.
        let json = serde_json::to_string(&DEFAULT_ROUTING_POLICY).unwrap();
        let back: RoutingPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(back, DEFAULT_ROUTING_POLICY);
    }

    #[test]
    fn test_routing_policy_legacy_json_gets_severed_defaults() {
        // A policy row persisted BEFORE the severed-quarantine knobs existed
        // must still parse (new fields default in) — otherwise an upgrade would
        // silently reset an operator's custom tuning to the defaults.
        let legacy = r#"{
            "stream_header_timeout_secs": 11,
            "nonstream_header_timeout_secs": 222,
            "fault_window_secs": 99,
            "overload_base_cooldown_secs": 5,
            "overload_max_cooldown_secs": 55,
            "circuit_failure_threshold": 4,
            "circuit_base_cooldown_secs": 22,
            "circuit_max_cooldown_secs": 111,
            "rate_limit_default_cooldown_secs": 9,
            "decay_interval_secs": 200
        }"#;
        let p: RoutingPolicy = serde_json::from_str(legacy).expect("legacy JSON must parse");
        assert_eq!(p.stream_header_timeout_secs, 11);
        assert_eq!(p.decay_interval_secs, 200);
        assert_eq!(
            p.severed_quarantine_secs,
            DEFAULT_ROUTING_POLICY.severed_quarantine_secs
        );
        assert_eq!(
            p.severed_quarantine_max_secs,
            DEFAULT_ROUTING_POLICY.severed_quarantine_max_secs
        );
        assert_eq!(
            p.severed_quarantine_backoff,
            DEFAULT_ROUTING_POLICY.severed_quarantine_backoff
        );
    }

    #[test]
    fn test_render_routing_policy_block_yaml_and_toml() {
        let yaml = render_routing_policy_block(&DEFAULT_ROUTING_POLICY, true).unwrap();
        assert!(yaml.starts_with("routing_policy:"));
        assert!(yaml.contains("overload_base_cooldown_secs: 15"));

        let toml = render_routing_policy_block(&DEFAULT_ROUTING_POLICY, false).unwrap();
        assert!(toml.starts_with("[routing_policy]"));
        assert!(toml.contains("overload_base_cooldown_secs = 15"));
    }

    #[test]
    fn test_upsert_managed_block_appends_then_replaces() {
        let base = "server:\n  host: \"127.0.0.1\"\n";
        let b1 = render_routing_policy_block(&DEFAULT_ROUTING_POLICY, true).unwrap();
        let first = upsert_managed_block(base, &b1).expect("append must succeed");
        assert!(first.contains("server:"));
        assert!(first.contains(ROUTING_POLICY_MARKER_START));
        assert!(first.contains(ROUTING_POLICY_MARKER_END));

        // Re-writing with a different policy must REPLACE, never duplicate.
        let mut custom = DEFAULT_ROUTING_POLICY;
        custom.fault_window_secs = 42;
        let b2 = render_routing_policy_block(&custom, true).unwrap();
        let second = upsert_managed_block(&first, &b2).expect("replace must succeed");
        assert_eq!(second.matches(ROUTING_POLICY_MARKER_START).count(), 1);
        assert_eq!(second.matches(ROUTING_POLICY_MARKER_END).count(), 1);
        assert!(second.contains("fault_window_secs: 42"));
        assert!(!second.contains("fault_window_secs: 180"));
        assert!(second.contains("host: \"127.0.0.1\""));
    }

    #[test]
    fn test_upsert_refuses_unmanaged_declaration() {
        // A hand-written routing_policy key (no markers) must block the append:
        // a duplicate top-level key would make the config unparseable on boot.
        let content = "server:\n  host: x\nrouting_policy:\n  fault_window_secs: 60\n";
        let b = render_routing_policy_block(&DEFAULT_ROUTING_POLICY, true).unwrap();
        assert!(upsert_managed_block(content, &b).is_none());
    }

    #[test]
    fn test_upsert_replace_refuses_unmanaged_duplicate() {
        // Managed block present AND a hand-added key elsewhere → the REPLACE
        // path must also refuse (splicing would leave a duplicate key).
        let base = "server:\n  host: x\n";
        let b = render_routing_policy_block(&DEFAULT_ROUTING_POLICY, true).unwrap();
        let managed = upsert_managed_block(base, &b).unwrap();
        let dirty = format!("{}routing_policy:\n  fault_window_secs: 60\n", managed);
        assert!(upsert_managed_block(&dirty, &b).is_none());
    }

    #[test]
    fn test_write_back_refuses_config_without_server_section() {
        let dir = std::env::temp_dir().join(format!("wirellm_cfg_nosrv_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);

        // A file with no server section must be refused (would not parse on boot).
        let path = dir.join("config.yaml");
        std::fs::write(&path, "# empty\n").unwrap();
        assert!(write_routing_policy_to_file(&path, &DEFAULT_ROUTING_POLICY).is_err());

        // A missing file is refused too — never implicitly create a server-less config.
        let missing = dir.join("nope.yaml");
        assert!(write_routing_policy_to_file(&missing, &DEFAULT_ROUTING_POLICY).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_write_routing_policy_to_file_roundtrip_yaml() {
        let dir = std::env::temp_dir().join(format!("wirellm_cfg_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.yaml");
        std::fs::write(
            &path,
            "# my config\nserver:\n  host: \"127.0.0.1\"\n  port: 8765\n  otel_endpoint: \"http://x\"\n  upstream_timeout_secs: 300\n  bind_family: \"ipv4\"\n",
        )
        .unwrap();

        let mut policy = DEFAULT_ROUTING_POLICY;
        policy.circuit_max_cooldown_secs = 77;
        let written = write_routing_policy_to_file(&path, &policy).unwrap();
        assert_eq!(written, path);

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# my config"), "existing comments must survive");
        assert!(text.contains("circuit_max_cooldown_secs: 77"));

        // Parses back into the full Config with the policy attached.
        let cfg = Config::load_from_file(&path).unwrap();
        assert_eq!(cfg.routing_policy.unwrap().circuit_max_cooldown_secs, 77);
        assert_eq!(cfg.server.port, 8765);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_write_routing_policy_to_file_roundtrip_toml() {
        let dir = std::env::temp_dir().join(format!("wirellm_cfg_toml_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "[server]\nhost = \"127.0.0.1\"\nport = 8765\notel_endpoint = \"http://x\"\nupstream_timeout_secs = 300\nbind_family = \"ipv4\"\n",
        )
        .unwrap();

        write_routing_policy_to_file(&path, &DEFAULT_ROUTING_POLICY).unwrap();
        let cfg = Config::load_from_file(&path).unwrap();
        assert_eq!(cfg.routing_policy.unwrap(), DEFAULT_ROUTING_POLICY);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
