use crate::error::{OxllmError, Result};
use reqwest::header::{HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

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

    /// Public origin of the application (e.g. "https://wire.nchanged.eu").
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
}

impl Config {
    /// Loads a YAML or TOML configuration file and expands environment variables of style `${VAR_NAME}`.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let p = path.as_ref();
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

        // 1. Validate that at least one provider is configured
        if self.providers.is_empty() {
            if let Ok(db) = crate::db::DbManager::new(None::<&str>) {
                if let Ok(db_cfg) = db.load_config() {
                    if !db_cfg.providers.is_empty() {
                        return Ok(());
                    }
                }
            }
            return Err(OxllmError::ConfigLoad(
                "At least one provider must be defined in config or database".into(),
            ));
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
}
