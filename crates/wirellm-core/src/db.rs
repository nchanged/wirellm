use crate::config::{Config, ProviderConfig, ServerConfig, VirtualModelTarget};
use crate::error::{OxllmError, Result};
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Resolves the default ~/.wirellm directory.
/// Creates the directory if it does not already exist.
pub fn get_wirellm_home() -> PathBuf {
    if let Ok(dir) = std::env::var("WIRELLM_HOME").or_else(|_| std::env::var("OXLLM_HOME")) {
        let p = PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&p);
        return p;
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let wirellm_dir = PathBuf::from(&home).join(".wirellm");
    if !wirellm_dir.exists() {
        let old_dir = PathBuf::from(&home).join(".oxllm");
        if old_dir.exists() {
            let _ = std::fs::create_dir_all(&wirellm_dir);
            let _ = std::fs::copy(old_dir.join(".vault_salt"), wirellm_dir.join(".vault_salt"));
            let _ = std::fs::copy(old_dir.join("config.yaml"), wirellm_dir.join("config.yaml"));
            let _ = std::fs::copy(old_dir.join("oxllm.db"), wirellm_dir.join("wirellm.db"));
            let _ = std::fs::copy(old_dir.join(".env"), wirellm_dir.join(".env"));
        }
    }
    let _ = std::fs::create_dir_all(&wirellm_dir);
    wirellm_dir
}

/// Backwards-compatibility alias for get_wirellm_home.
pub fn get_oxllm_home() -> PathBuf {
    get_wirellm_home()
}

/// Resolves the default SQLite database path at ~/.wirellm/wirellm.db.
pub fn get_default_db_path() -> PathBuf {
    get_wirellm_home().join("wirellm.db")
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DailyStats {
    pub date: String,
    pub provider_name: String,
    pub model_name: String,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub avg_latency_ms: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderSummary {
    pub name: String,
    pub enabled: bool,
    pub base_url: String,
    pub api_key_template: String,
    pub models: Vec<String>,
    pub rpm: Option<u32>,
    pub min_interval_ms: Option<u64>,
    pub max_concurrency: Option<u32>,
    pub smartness_score: u32,
    pub lifetime_requests: u64,
    pub lifetime_successes: u64,
    pub lifetime_input_tokens: u64,
    pub lifetime_output_tokens: u64,
    pub avg_latency_ms: f64,
    pub has_encrypted_key: bool,
    pub key_preview: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VirtualModelDetail {
    pub name: String,
    pub description: String,
    pub weight_smartness: f64,
    pub weight_speed: f64,
    pub weight_reliability: f64,
    pub targets: Vec<VirtualModelTarget>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApiKeyRecord {
    pub id: String,
    pub name: String,
    pub key_preview: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_key: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VaultSecretRecord {
    pub name: String,
    pub key_preview: String,
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct DbManager {
    db_path: PathBuf,
}

impl std::fmt::Debug for DbManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbManager").field("db_path", &self.db_path).finish()
    }
}

impl DbManager {
    /// Opens the SQLite database at the specified path (or default ~/.oxllm/oxllm.db)
    /// and initializes the schema.
    pub fn new<P: AsRef<Path>>(path: Option<P>) -> Result<Self> {
        let db_path = match path {
            Some(p) => p.as_ref().to_path_buf(),
            None => get_default_db_path(),
        };

        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let mgr = Self { db_path };
        mgr.init_schema()?;
        mgr.auto_migrate_if_empty()?;
        Ok(mgr)
    }

    /// Open a connection with WAL mode and foreign keys enabled.
    pub fn connect(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)
            .map_err(|e| OxllmError::Internal(format!("Failed to open SQLite database: {}", e)))?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )
        .map_err(|e| OxllmError::Internal(format!("Failed to configure SQLite PRAGMAs: {}", e)))?;

        Ok(conn)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.connect()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS server_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS providers (
                name TEXT PRIMARY KEY,
                enabled INTEGER NOT NULL DEFAULT 1,
                base_url TEXT NOT NULL,
                api_key_template TEXT NOT NULL,
                models_json TEXT NOT NULL,
                headers_json TEXT NOT NULL DEFAULT '{}',
                rpm INTEGER,
                min_interval_ms INTEGER,
                max_concurrency INTEGER,
                smartness_score INTEGER NOT NULL DEFAULT 80,
                encrypted_api_key TEXT DEFAULT '',
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS virtual_models (
                name TEXT PRIMARY KEY,
                description TEXT DEFAULT '',
                weight_smartness REAL NOT NULL DEFAULT 0.5,
                weight_speed REAL NOT NULL DEFAULT 0.3,
                weight_reliability REAL NOT NULL DEFAULT 0.2,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS virtual_model_targets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                virtual_model_name TEXT NOT NULL REFERENCES virtual_models(name) ON DELETE CASCADE,
                provider_name TEXT NOT NULL REFERENCES providers(name) ON DELETE CASCADE,
                model_name TEXT NOT NULL,
                priority_order INTEGER NOT NULL,
                UNIQUE(virtual_model_name, provider_name, model_name)
            );

            CREATE TABLE IF NOT EXISTS request_metrics (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                provider_name TEXT NOT NULL,
                model_name TEXT NOT NULL,
                duration_ms INTEGER NOT NULL,
                status_code INTEGER NOT NULL,
                input_tokens INTEGER DEFAULT 0,
                output_tokens INTEGER DEFAULT 0,
                is_stream INTEGER DEFAULT 0,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );

            CREATE INDEX IF NOT EXISTS idx_metrics_provider ON request_metrics(provider_name, created_at);

            CREATE TABLE IF NOT EXISTS daily_usage (
                date TEXT NOT NULL,
                provider_name TEXT NOT NULL,
                model_name TEXT NOT NULL,
                total_requests INTEGER DEFAULT 0,
                successful_requests INTEGER DEFAULT 0,
                total_input_tokens INTEGER DEFAULT 0,
                total_output_tokens INTEGER DEFAULT 0,
                total_duration_ms INTEGER DEFAULT 0,
                PRIMARY KEY(date, provider_name, model_name)
            );

            CREATE TABLE IF NOT EXISTS api_keys (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                key_val TEXT NOT NULL,
                key_preview TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS vault_secrets (
                name TEXT PRIMARY KEY,
                encrypted_value TEXT NOT NULL,
                key_preview TEXT NOT NULL,
                description TEXT DEFAULT '',
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );",
        )
        .map_err(|e| OxllmError::Internal(format!("Failed to initialize SQLite schema: {}", e)))?;

        // Auto-migrate column encrypted_api_key for existing databases
        let _ = conn.execute("ALTER TABLE providers ADD COLUMN encrypted_api_key TEXT DEFAULT ''", []);

        // Auto-migrate any existing provider encrypted keys to central vault_secrets table
        let _ = self.auto_migrate_provider_keys_to_vault();

        Ok(())
    }

    /// Automatically migrates existing configuration from ~/.config/oxllm/config.toml
    /// into SQLite if providers table is currently empty.
    fn auto_migrate_if_empty(&self) -> Result<()> {
        let conn = self.connect()?;
        let count: i64 = conn
            .query_row("SELECT count(*) FROM providers", [], |row| row.get(0))
            .unwrap_or(0);

        if count > 0 {
            return Ok(());
        }

        // Try locating existing config.toml
        let candidates = [
            get_oxllm_home().join("config.toml"),
            get_oxllm_home().join("config.backup.toml"),
            dirs_fallback_config_path(),
            PathBuf::from("config.toml"),
        ];

        let mut found_path = None;
        for path in &candidates {
            if path.exists() {
                found_path = Some(path.clone());
                break;
            }
        }

        let config_path = match found_path {
            Some(p) => p,
            None => return Ok(()),
        };

        info!(
            "Database is empty. Automatically migrating existing configuration from {:?}",
            config_path
        );

        // Read raw TOML content (preserving ${VAR} templates)
        let content = match std::fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read config for migration: {}", e);
                return Ok(());
            },
        };

        // Also save a backup into ~/.oxllm/config.backup.toml
        let backup_path = get_oxllm_home().join("config.backup.toml");
        let _ = std::fs::write(&backup_path, &content);

        let parsed: toml::Value = match toml::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to parse TOML during auto-migration: {}", e);
                return Ok(());
            },
        };

        // 1. Server settings
        if let Some(server) = parsed.get("server").and_then(|s| s.as_table()) {
            let host = server.get("host").and_then(|v| v.as_str()).unwrap_or("127.0.0.1");
            let port = server.get("port").and_then(|v| v.as_integer()).unwrap_or(8088);
            let otel = server.get("otel_endpoint").and_then(|v| v.as_str()).unwrap_or("http://127.0.0.1:4318");
            let timeout = server.get("upstream_timeout_secs").and_then(|v| v.as_integer()).unwrap_or(300);
            let bind_family = server.get("bind_family").and_then(|v| v.as_str()).unwrap_or("ipv4");

            let _ = conn.execute("INSERT OR REPLACE INTO server_settings (key, value) VALUES ('host', ?)", [host]);
            let _ = conn.execute("INSERT OR REPLACE INTO server_settings (key, value) VALUES ('port', ?)", [port.to_string()]);
            let _ = conn.execute("INSERT OR REPLACE INTO server_settings (key, value) VALUES ('otel_endpoint', ?)", [otel]);
            let _ = conn.execute("INSERT OR REPLACE INTO server_settings (key, value) VALUES ('upstream_timeout_secs', ?)", [timeout.to_string()]);
            let _ = conn.execute("INSERT OR REPLACE INTO server_settings (key, value) VALUES ('bind_family', ?)", [bind_family]);
        }

        // 2. Providers
        if let Some(providers) = parsed.get("providers").and_then(|p| p.as_array()) {
            for p in providers {
                let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if name.is_empty() {
                    continue;
                }
                let enabled = if p.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true) { 1 } else { 0 };
                let base_url = p.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
                let api_key = p.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
                
                let models: Vec<String> = p.get("models")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|m| m.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let models_json = serde_json::to_string(&models).unwrap_or_else(|_| "[]".to_string());

                let headers_map: HashMap<String, String> = p.get("headers")
                    .and_then(|v| v.as_table())
                    .map(|tbl| tbl.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
                    .unwrap_or_default();
                let headers_json = serde_json::to_string(&headers_map).unwrap_or_else(|_| "{}".to_string());

                let rpm = p.get("rpm").and_then(|v| v.as_integer());
                let min_interval_ms = p.get("min_interval_ms").and_then(|v| v.as_integer());
                let max_concurrency = p.get("max_concurrency").and_then(|v| v.as_integer());

                let smartness = calculate_default_smartness(name, &models);

                let _ = conn.execute(
                    "INSERT OR REPLACE INTO providers 
                    (name, enabled, base_url, api_key_template, models_json, headers_json, rpm, min_interval_ms, max_concurrency, smartness_score) 
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![name, enabled, base_url, api_key, models_json, headers_json, rpm, min_interval_ms, max_concurrency, smartness],
                );
            }
        }

        // 3. Virtual Models (skip 1-to-1 single model shims handled natively by direct passthrough)
        if let Some(vms) = parsed.get("virtual_models").and_then(|v| v.as_table()) {
            for (vm_name, targets) in vms {
                let target_arr = match targets.as_array() {
                    Some(arr) if !arr.is_empty() => arr,
                    _ => continue,
                };

                // Skip 1-to-1 passthrough dummy virtual models (handled natively by direct routing)
                if target_arr.len() == 1 {
                    let single_model = target_arr[0].get("model").and_then(|v| v.as_str()).unwrap_or("");
                    if single_model == vm_name || single_model.ends_with(&format!("/{}", vm_name)) || vm_name.starts_with("nvidia-") || vm_name.starts_with("openrouter-") {
                        continue;
                    }
                }

                let _ = conn.execute(
                    "INSERT OR REPLACE INTO virtual_models (name, description) VALUES (?1, ?2)",
                    params![vm_name, format!("Virtual model group for {}", vm_name)],
                );

                for (order, t) in target_arr.iter().enumerate() {
                    let provider = t.get("provider").and_then(|v| v.as_str()).unwrap_or("");
                    let model = t.get("model").and_then(|v| v.as_str()).unwrap_or("");
                    if !provider.is_empty() && !model.is_empty() {
                        let _ = conn.execute(
                            "INSERT OR REPLACE INTO virtual_model_targets 
                            (virtual_model_name, provider_name, model_name, priority_order) 
                            VALUES (?1, ?2, ?3, ?4)",
                            params![vm_name, provider, model, order as i64],
                        );
                    }
                }
            }
        }

        info!("Successfully migrated configuration to SQLite database at {:?}", self.db_path);
        Ok(())
    }

    /// Loads the active Config struct from SQLite, expanding environment variables.
    pub fn load_config(&self) -> Result<Config> {
        let conn = self.connect()?;

        // 1. Server settings
        let host: String = conn.query_row("SELECT value FROM server_settings WHERE key = 'host'", [], |r| r.get(0)).unwrap_or_else(|_| "127.0.0.1".to_string());
        let port_str: String = conn.query_row("SELECT value FROM server_settings WHERE key = 'port'", [], |r| r.get(0)).unwrap_or_else(|_| "8088".to_string());
        let port = port_str.parse::<u16>().unwrap_or(8088);
        let otel_endpoint: String = conn.query_row("SELECT value FROM server_settings WHERE key = 'otel_endpoint'", [], |r| r.get(0)).unwrap_or_else(|_| "http://127.0.0.1:4318".to_string());
        let timeout_str: String = conn.query_row("SELECT value FROM server_settings WHERE key = 'upstream_timeout_secs'", [], |r| r.get(0)).unwrap_or_else(|_| "300".to_string());
        let upstream_timeout_secs = timeout_str.parse::<u64>().unwrap_or(300);
        let bind_family: String = conn.query_row("SELECT value FROM server_settings WHERE key = 'bind_family'", [], |r| r.get(0)).unwrap_or_else(|_| "ipv4".to_string());

        let server = ServerConfig {
            host,
            port,
            otel_endpoint,
            upstream_timeout_secs,
            bind_family,
        };

        // 2. Providers
        let mut stmt = conn.prepare(
            "SELECT name, enabled, base_url, api_key_template, models_json, headers_json, rpm, min_interval_ms, max_concurrency, COALESCE(encrypted_api_key, '') 
             FROM providers ORDER BY name ASC"
        ).map_err(|e| OxllmError::Internal(e.to_string()))?;

        let provider_rows = stmt.query_map([], |row| {
            let name: String = row.get(0)?;
            let enabled_int: i64 = row.get(1)?;
            let base_url: String = row.get(2)?;
            let api_key_template: String = row.get(3)?;
            let models_json: String = row.get(4)?;
            let headers_json: String = row.get(5)?;
            let rpm: Option<u32> = row.get::<_, Option<i64>>(6)?.map(|v| v as u32);
            let min_interval_ms: Option<u64> = row.get::<_, Option<i64>>(7)?.map(|v| v as u64);
            let max_concurrency: Option<u32> = row.get::<_, Option<i64>>(8)?.map(|v| v as u32);
            let encrypted_api_key: String = row.get(9)?;

            Ok((name, enabled_int != 0, base_url, api_key_template, models_json, headers_json, rpm, min_interval_ms, max_concurrency, encrypted_api_key))
        }).map_err(|e| OxllmError::Internal(e.to_string()))?;

        let mut providers = Vec::new();
        for res in provider_rows {
            let (name, enabled, base_url, api_key_template, models_json, headers_json, rpm, min_interval_ms, max_concurrency, encrypted_api_key) = res
                .map_err(|e| OxllmError::Internal(e.to_string()))?;

            let models: Vec<String> = serde_json::from_str(&models_json).unwrap_or_default();
            let raw_headers: HashMap<String, String> = serde_json::from_str(&headers_json).unwrap_or_default();
            // Expand ${VAR} header values from central Vault secrets or environment variables.
            // Unresolvable headers are skipped (with a warning) instead of disabling the provider.
            let mut headers: HashMap<String, String> = HashMap::new();
            for (hk, hv) in raw_headers {
                match self.resolve_secret_template(&hv) {
                    Ok(v) => { headers.insert(hk, v); }
                    Err(e) => {
                        tracing::warn!("Header '{}' for provider '{}' refers to an unresolvable secret ({}); skipping header", hk, name, e);
                    }
                }
            }

            // Decrypt raw API key from vault if present, otherwise expand environment variable / vault template
            let (api_key, enabled) = if !encrypted_api_key.trim().is_empty() {
                match crate::vault::decrypt_key(&encrypted_api_key) {
                    Ok(k) if !k.trim().is_empty() => (k, enabled),
                    Ok(_) => (String::new(), false),
                    Err(e) => {
                        tracing::warn!("Provider '{}' has encrypted key in vault, but decryption failed ({}). Disabling provider.", name, e);
                        (String::new(), false)
                    }
                }
            } else {
                match self.resolve_secret_template(&api_key_template) {
                    Ok(k) if !k.trim().is_empty() => (k, enabled),
                    Ok(_) => (String::new(), false),
                    Err(e) => {
                        tracing::warn!("Provider '{}' has unresolvable api_key ({}). Disabling provider until configured.", name, e);
                        (String::new(), false)
                    }
                }
            };

            providers.push(ProviderConfig {
                name,
                enabled,
                base_url,
                api_key,
                models,
                headers,
                rpm,
                min_interval_ms,
                max_concurrency,
            });
        }

        // 3. Virtual Models
        let mut vm_stmt = conn.prepare("SELECT name FROM virtual_models ORDER BY name ASC")
            .map_err(|e| OxllmError::Internal(e.to_string()))?;
        let vm_rows = vm_stmt.query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| OxllmError::Internal(e.to_string()))?;

        let mut virtual_models: HashMap<String, Vec<VirtualModelTarget>> = HashMap::new();
        for vm_res in vm_rows {
            let vm_name = vm_res.map_err(|e| OxllmError::Internal(e.to_string()))?;
            let mut target_stmt = conn.prepare(
                "SELECT provider_name, model_name FROM virtual_model_targets 
                 WHERE virtual_model_name = ?1 ORDER BY priority_order ASC"
            ).map_err(|e| OxllmError::Internal(e.to_string()))?;

            let target_rows = target_stmt.query_map([&vm_name], |row| {
                Ok(VirtualModelTarget {
                    provider: row.get(0)?,
                    model: row.get(1)?,
                })
            }).map_err(|e| OxllmError::Internal(e.to_string()))?;

            let mut targets = Vec::new();
            for t_res in target_rows {
                targets.push(t_res.map_err(|e| OxllmError::Internal(e.to_string()))?);
            }
            virtual_models.insert(vm_name, targets);
        }

        Ok(Config {
            server,
            auth: Default::default(),
            providers,
            virtual_models,
            // The routing policy lives in its own server_settings key and is
            // resolved by `build_app_state` (file value takes precedence).
            routing_policy: None,
        })
    }

    /// Records request execution metrics and updates daily usage counters.
    pub fn record_metric(
        &self,
        provider: &str,
        model: &str,
        duration_ms: u64,
        status_code: u16,
        in_tokens: u64,
        out_tokens: u64,
        is_stream: bool,
    ) -> Result<()> {
        let conn = self.connect()?;
        let is_success = if (200..300).contains(&status_code) { 1 } else { 0 };

        // 1. Insert into raw request_metrics
        let _ = conn.execute(
            "INSERT INTO request_metrics 
            (provider_name, model_name, duration_ms, status_code, input_tokens, output_tokens, is_stream) 
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![provider, model, duration_ms as i64, status_code as i64, in_tokens as i64, out_tokens as i64, is_stream as i64],
        );

        // 2. Upsert into daily_usage
        let today = time::OffsetDateTime::now_utc().date().to_string();
        let _ = conn.execute(
            "INSERT INTO daily_usage 
            (date, provider_name, model_name, total_requests, successful_requests, total_input_tokens, total_output_tokens, total_duration_ms) 
            VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?7)
            ON CONFLICT(date, provider_name, model_name) DO UPDATE SET 
                total_requests = total_requests + 1,
                successful_requests = successful_requests + ?4,
                total_input_tokens = total_input_tokens + ?5,
                total_output_tokens = total_output_tokens + ?6,
                total_duration_ms = total_duration_ms + ?7",
            params![today, provider, model, is_success, in_tokens as i64, out_tokens as i64, duration_ms as i64],
        );

        Ok(())
    }

    /// Returns daily usage statistics within the last `days` days, or the
    /// ENTIRE history when `days == 0` (the date filter is real now — no more
    /// "last N rows" approximation).
    pub fn get_daily_stats(&self, days: u32) -> Result<Vec<DailyStats>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT date, provider_name, model_name, total_requests, successful_requests, 
                    total_input_tokens, total_output_tokens,
                    CASE WHEN total_requests > 0 THEN (total_duration_ms * 1.0 / total_requests) ELSE 0.0 END as avg_latency
             FROM daily_usage 
             WHERE (?1 = 0 OR date >= date('now', '-' || (?1 - 1) || ' days'))
             ORDER BY date DESC, total_requests DESC"
        ).map_err(|e| OxllmError::Internal(e.to_string()))?;

        let rows = stmt.query_map([days as i64], |row| {
            Ok(DailyStats {
                date: row.get(0)?,
                provider_name: row.get(1)?,
                model_name: row.get(2)?,
                total_requests: row.get::<_, i64>(3)? as u64,
                successful_requests: row.get::<_, i64>(4)? as u64,
                total_input_tokens: row.get::<_, i64>(5)? as u64,
                total_output_tokens: row.get::<_, i64>(6)? as u64,
                avg_latency_ms: row.get(7)?,
            })
        }).map_err(|e| OxllmError::Internal(e.to_string()))?;

        let mut res = Vec::new();
        for r in rows {
            res.push(r.map_err(|e| OxllmError::Internal(e.to_string()))?);
        }
        Ok(res)
    }

    /// Returns daily usage statistics for an explicit inclusive date range,
    /// or the entire history when both bounds are None. Dates are 'YYYY-MM-DD'.
    pub fn get_daily_stats_range(
        &self,
        from_date: Option<String>,
        to_date: Option<String>,
    ) -> Result<Vec<DailyStats>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT date, provider_name, model_name, total_requests, successful_requests, 
                    total_input_tokens, total_output_tokens,
                    CASE WHEN total_requests > 0 THEN (total_duration_ms * 1.0 / total_requests) ELSE 0.0 END as avg_latency
             FROM daily_usage 
             WHERE (?1 IS NULL OR date >= ?1)
               AND (?2 IS NULL OR date <= ?2)
             ORDER BY date DESC, total_requests DESC"
        ).map_err(|e| OxllmError::Internal(e.to_string()))?;

        let rows = stmt.query_map(params![from_date, to_date], |row| {
            Ok(DailyStats {
                date: row.get(0)?,
                provider_name: row.get(1)?,
                model_name: row.get(2)?,
                total_requests: row.get::<_, i64>(3)? as u64,
                successful_requests: row.get::<_, i64>(4)? as u64,
                total_input_tokens: row.get::<_, i64>(5)? as u64,
                total_output_tokens: row.get::<_, i64>(6)? as u64,
                avg_latency_ms: row.get(7)?,
            })
        }).map_err(|e| OxllmError::Internal(e.to_string()))?;

        let mut res = Vec::new();
        for r in rows {
            res.push(r.map_err(|e| OxllmError::Internal(e.to_string()))?);
        }
        Ok(res)
    }
    pub fn get_provider_summaries(&self) -> Result<Vec<ProviderSummary>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT p.name, p.enabled, p.base_url, p.api_key_template, p.models_json, p.rpm, p.min_interval_ms, p.max_concurrency, p.smartness_score,
                    COALESCE(SUM(d.total_requests), 0) as total_reqs,
                    COALESCE(SUM(d.successful_requests), 0) as total_succ,
                    COALESCE(SUM(d.total_input_tokens), 0) as total_in,
                    COALESCE(SUM(d.total_output_tokens), 0) as total_out,
                    COALESCE(AVG(d.total_duration_ms * 1.0 / NULLIF(d.total_requests, 0)), 0.0) as avg_lat,
                    COALESCE(p.encrypted_api_key, '') as encrypted_api_key
             FROM providers p
             LEFT JOIN daily_usage d ON p.name = d.provider_name
             GROUP BY p.name
              ORDER BY p.name ASC"
        ).map_err(|e| OxllmError::Internal(e.to_string()))?;

        let vault_names: std::collections::HashSet<String> = {
            let mut set = std::collections::HashSet::new();
            if let Ok(secrets) = self.list_vault_secrets() {
                for s in secrets {
                    if s.name.ends_with("_API_KEY") {
                        set.insert(s.name[..s.name.len() - 8].to_string());
                    }
                    set.insert(s.name);
                }
            }
            set
        };

        let rows = stmt.query_map([], |row| {
            let name: String = row.get(0)?;
            let enabled_int: i64 = row.get(1)?;
            let base_url: String = row.get(2)?;
            let api_key_template: String = row.get(3)?;
            let models_json: String = row.get(4)?;
            let rpm: Option<u32> = row.get::<_, Option<i64>>(5)?.map(|v| v as u32);
            let min_interval_ms: Option<u64> = row.get::<_, Option<i64>>(6)?.map(|v| v as u64);
            let max_concurrency: Option<u32> = row.get::<_, Option<i64>>(7)?.map(|v| v as u32);
            let smartness_score: u32 = row.get::<_, i64>(8)? as u32;
            let enc_key: String = row.get(14)?;

            let models: Vec<String> = serde_json::from_str(&models_json).unwrap_or_default();
            let tmpl_var = api_key_template
                .trim()
                .trim_start_matches("${")
                .trim_end_matches('}')
                .trim_start_matches('$')
                .to_uppercase()
                .replace('-', "_");
            let in_vault = vault_names.contains(&tmpl_var);
            let has_encrypted_key = !enc_key.trim().is_empty() || in_vault;
            let key_preview = if has_encrypted_key {
                "••••••••••••".to_string()
            } else {
                api_key_template.clone()
            };

            Ok(ProviderSummary {
                name,
                enabled: enabled_int != 0,
                base_url,
                api_key_template,
                models,
                rpm,
                min_interval_ms,
                max_concurrency,
                smartness_score,
                lifetime_requests: row.get::<_, i64>(9)? as u64,
                lifetime_successes: row.get::<_, i64>(10)? as u64,
                lifetime_input_tokens: row.get::<_, i64>(11)? as u64,
                lifetime_output_tokens: row.get::<_, i64>(12)? as u64,
                avg_latency_ms: row.get(13)?,
                has_encrypted_key,
                key_preview,
            })
        }).map_err(|e| OxllmError::Internal(e.to_string()))?;

        let mut res = Vec::new();
        for r in rows {
            res.push(r.map_err(|e| OxllmError::Internal(e.to_string()))?);
        }
        Ok(res)
    }

    /// Persist a provider's per-UTC-hour fault histogram into server_settings
    /// so the diurnal overload memory survives daemon restarts.
    pub fn save_provider_fault_pattern(&self, provider: &str, buckets: &[u64; 24]) -> Result<()> {
        let conn = self.connect()?;
        let payload = serde_json::to_string(buckets)
            .map_err(|e| OxllmError::Internal(e.to_string()))?;
        conn.execute(
            "INSERT INTO server_settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![format!("diurnal_faults:{}", provider), payload],
        )
        .map_err(|e| OxllmError::Internal(format!("Failed to save fault pattern: {}", e)))?;
        Ok(())
    }

    /// Loads a provider's persisted per-UTC-hour fault histogram, if any.
    pub fn load_provider_fault_pattern(&self, provider: &str) -> Result<Option<[u64; 24]>> {
        let conn = self.connect()?;
        let json: Option<String> = conn
            .query_row(
                "SELECT value FROM server_settings WHERE key = ?1",
                params![format!("diurnal_faults:{}", provider)],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(OxllmError::Internal(other.to_string())),
            })?;

        match json {
            Some(s) => serde_json::from_str::<[u64; 24]>(&s)
                .map(Some)
                .map_err(|e| OxllmError::Internal(format!("Corrupt fault pattern: {}", e))),
            None => Ok(None),
        }
    }

    /// Loads the persisted RoutingPolicy from server_settings (or returns Default).
    pub fn get_routing_policy(&self) -> Result<crate::config::RoutingPolicy> {
        let conn = self.connect()?;
        let json: Option<String> = conn
            .query_row(
                "SELECT value FROM server_settings WHERE key = 'routing_policy_json'",
                [],
                |row| row.get(0),
            )
            .ok();
        match json {
            Some(s) => Ok(serde_json::from_str(&s).unwrap_or_default()),
            None => Ok(crate::config::RoutingPolicy::default()),
        }
    }

    /// Persists the RoutingPolicy into server_settings.
    pub fn save_routing_policy(&self, policy: &crate::config::RoutingPolicy) -> Result<()> {
        let conn = self.connect()?;
        let payload = serde_json::to_string(policy)
            .map_err(|e| OxllmError::Internal(e.to_string()))?;
        conn.execute(
            "INSERT INTO server_settings (key, value) VALUES ('routing_policy_json', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![payload],
        )
        .map_err(|e| OxllmError::Internal(format!("Failed to save routing policy: {}", e)))?;
        Ok(())
    }

    /// Upserts a provider into SQLite with optional encrypted secret key in vault.
    pub fn upsert_provider(
        &self,
        name: &str,
        enabled: bool,
        base_url: &str,
        api_key_template: &str,
        models: &[String],
        headers: &HashMap<String, String>,
        rpm: Option<u32>,
        min_interval_ms: Option<u64>,
        max_concurrency: Option<u32>,
        smartness: Option<u32>,
        raw_api_key: Option<&str>,
    ) -> Result<()> {
        let conn = self.connect()?;
        let models_json = serde_json::to_string(models).unwrap_or_else(|_| "[]".to_string());
        let headers_json = serde_json::to_string(headers).unwrap_or_else(|_| "{}".to_string());
        let smartness_score = smartness.unwrap_or_else(|| calculate_default_smartness(name, models));

        let trimmed_raw = raw_api_key.map(|s| s.trim()).filter(|s| !s.is_empty());
        let trimmed_template = api_key_template.trim();

        let (final_template, encrypted_val): (String, Option<String>) = if let Some(raw) = trimmed_raw {
            if raw.starts_with('$') {
                // User provided an env var reference (e.g. $VAR or ${VAR}) -> clear encrypted key
                (raw.to_string(), Some(String::new()))
            } else {
                // User provided a literal secret string -> encrypt it and register in central Vault
                let enc = crate::vault::encrypt_key(raw)?;
                let secret_name = format!("{}_API_KEY", name.to_uppercase().replace('-', "_"));
                let _ = self.upsert_vault_secret(&secret_name, raw, &format!("API key for provider '{}'", name));
                let templ = if trimmed_template.starts_with('$') {
                    trimmed_template.to_string()
                } else {
                    format!("${{{}}}", secret_name)
                };
                (templ, Some(enc))
            }
        } else if !trimmed_template.is_empty() && !trimmed_template.starts_with('$') {
            // Direct key entered in api_key_template field -> encrypt and register in central Vault
            let enc = crate::vault::encrypt_key(trimmed_template)?;
            let secret_name = format!("{}_API_KEY", name.to_uppercase().replace('-', "_"));
            let _ = self.upsert_vault_secret(&secret_name, trimmed_template, &format!("API key for provider '{}'", name));
            let templ = format!("${{{}}}", secret_name);
            (templ, Some(enc))
        } else if trimmed_template.starts_with('$') {
            (trimmed_template.to_string(), Some(String::new()))
        } else {
            (trimmed_template.to_string(), None)
        };

        if let Some(enc) = encrypted_val {
            conn.execute(
                "INSERT INTO providers 
                (name, enabled, base_url, api_key_template, models_json, headers_json, rpm, min_interval_ms, max_concurrency, smartness_score, encrypted_api_key, updated_at) 
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, CURRENT_TIMESTAMP)
                ON CONFLICT(name) DO UPDATE SET 
                    enabled = ?2,
                    base_url = ?3,
                    api_key_template = ?4,
                    models_json = ?5,
                    headers_json = ?6,
                    rpm = ?7,
                    min_interval_ms = ?8,
                    max_concurrency = ?9,
                    smartness_score = ?10,
                    encrypted_api_key = ?11,
                    updated_at = CURRENT_TIMESTAMP",
                params![
                    name,
                    if enabled { 1 } else { 0 },
                    base_url,
                    final_template,
                    models_json,
                    headers_json,
                    rpm.map(|v| v as i64),
                    min_interval_ms.map(|v| v as i64),
                    max_concurrency.map(|v| v as i64),
                    smartness_score as i64,
                    enc,
                ],
            ).map_err(|e| OxllmError::Internal(format!("Failed to upsert provider: {}", e)))?;
        } else {
            conn.execute(
                "INSERT INTO providers 
                (name, enabled, base_url, api_key_template, models_json, headers_json, rpm, min_interval_ms, max_concurrency, smartness_score, updated_at) 
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, CURRENT_TIMESTAMP)
                ON CONFLICT(name) DO UPDATE SET 
                    enabled = ?2,
                    base_url = ?3,
                    api_key_template = ?4,
                    models_json = ?5,
                    headers_json = ?6,
                    rpm = ?7,
                    min_interval_ms = ?8,
                    max_concurrency = ?9,
                    smartness_score = ?10,
                    updated_at = CURRENT_TIMESTAMP",
                params![
                    name,
                    if enabled { 1 } else { 0 },
                    base_url,
                    final_template,
                    models_json,
                    headers_json,
                    rpm.map(|v| v as i64),
                    min_interval_ms.map(|v| v as i64),
                    max_concurrency.map(|v| v as i64),
                    smartness_score as i64,
                ],
            ).map_err(|e| OxllmError::Internal(format!("Failed to upsert provider: {}", e)))?;
        }

        Ok(())
    }

    /// Deletes a provider and cascades targets.
    pub fn delete_provider(&self, name: &str) -> Result<()> {
        let conn = self.connect()?;
        conn.execute("DELETE FROM providers WHERE name = ?1", params![name])
            .map_err(|e| OxllmError::Internal(format!("Failed to delete provider: {}", e)))?;
        Ok(())
    }

    /// Upserts a virtual model and replaces its target candidates.
    pub fn upsert_virtual_model(
        &self,
        name: &str,
        description: &str,
        targets: &[VirtualModelTarget],
        weights: Option<(f64, f64, f64)>,
    ) -> Result<()> {
        let mut conn = self.connect()?;
        let tx = conn.transaction().map_err(|e| OxllmError::Internal(e.to_string()))?;

        // When weights are omitted we must PRESERVE the existing values on
        // update (an agent editing only the target ladder must not silently
        // reset operator-tuned weights). A brand-new row still gets the
        // documented defaults below.
        match weights {
            Some((w_smart, w_speed, w_rel)) => {
                tx.execute(
                    "INSERT INTO virtual_models (name, description, weight_smartness, weight_speed, weight_reliability, updated_at) 
                     VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP)
                     ON CONFLICT(name) DO UPDATE SET 
                        description = ?2,
                        weight_smartness = ?3,
                        weight_speed = ?4,
                        weight_reliability = ?5,
                        updated_at = CURRENT_TIMESTAMP",
                    params![name, description, w_smart, w_speed, w_rel],
                ).map_err(|e| OxllmError::Internal(e.to_string()))?;
            },
            None => {
                // Create-with-defaults (0.5 / 0.3 / 0.2) but never touch the
                // weight columns of an existing row.
                tx.execute(
                    "INSERT INTO virtual_models (name, description, weight_smartness, weight_speed, weight_reliability, updated_at) 
                     VALUES (?1, ?2, 0.5, 0.3, 0.2, CURRENT_TIMESTAMP)
                     ON CONFLICT(name) DO UPDATE SET 
                        description = ?2,
                        updated_at = CURRENT_TIMESTAMP",
                    params![name, description],
                ).map_err(|e| OxllmError::Internal(e.to_string()))?;
            },
        }

        tx.execute("DELETE FROM virtual_model_targets WHERE virtual_model_name = ?1", params![name])
            .map_err(|e| OxllmError::Internal(e.to_string()))?;

        for (idx, target) in targets.iter().enumerate() {
            tx.execute(
                "INSERT INTO virtual_model_targets (virtual_model_name, provider_name, model_name, priority_order) 
                 VALUES (?1, ?2, ?3, ?4)",
                params![name, target.provider, target.model, idx as i64],
            ).map_err(|e| OxllmError::Internal(e.to_string()))?;
        }

        tx.commit().map_err(|e| OxllmError::Internal(e.to_string()))?;
        Ok(())
    }

    /// Returns list of all virtual models with their targets and weights.
    pub fn get_virtual_models_detail(&self) -> Result<Vec<VirtualModelDetail>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT name, description, weight_smartness, weight_speed, weight_reliability 
             FROM virtual_models ORDER BY name ASC"
        ).map_err(|e| OxllmError::Internal(e.to_string()))?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, f64>(4)?,
            ))
        }).map_err(|e| OxllmError::Internal(e.to_string()))?;

        let mut res = Vec::new();
        for r in rows {
            let (name, description, weight_smartness, weight_speed, weight_reliability) = r
                .map_err(|e| OxllmError::Internal(e.to_string()))?;

            let mut target_stmt = conn.prepare(
                "SELECT provider_name, model_name FROM virtual_model_targets 
                 WHERE virtual_model_name = ?1 ORDER BY priority_order ASC"
            ).map_err(|e| OxllmError::Internal(e.to_string()))?;

            let targets = target_stmt.query_map([&name], |row| {
                Ok(VirtualModelTarget {
                    provider: row.get(0)?,
                    model: row.get(1)?,
                })
            }).map_err(|e| OxllmError::Internal(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

            res.push(VirtualModelDetail {
                name,
                description,
                weight_smartness,
                weight_speed,
                weight_reliability,
                targets,
            });
        }

        Ok(res)
    }

    /// Deletes a virtual model and its targets.
    pub fn delete_virtual_model(&self, name: &str) -> Result<()> {
        let conn = self.connect()?;
        conn.execute("DELETE FROM virtual_models WHERE name = ?1", params![name])
            .map_err(|e| OxllmError::Internal(format!("Failed to delete virtual model: {}", e)))?;
        Ok(())
    }

    /// List all registered API keys (without full secret value)
    pub fn list_api_keys(&self) -> Result<Vec<ApiKeyRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT id, name, key_preview, created_at FROM api_keys ORDER BY created_at DESC")
            .map_err(|e| OxllmError::Internal(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(ApiKeyRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    key_preview: row.get(2)?,
                    full_key: None,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|e| OxllmError::Internal(e.to_string()))?;

        let mut res = Vec::new();
        for r in rows {
            if let Ok(item) = r {
                res.push(item);
            }
        }
        Ok(res)
    }

    /// Create and persist a new API key, returning the record with the full secret key
    pub fn create_api_key(&self, name: &str, raw_key: Option<&str>) -> Result<ApiKeyRecord> {
        let conn = self.connect()?;
        let id = format!("key_{:012x}", rand::random::<u64>());
        let key_str = match raw_key {
            Some(k) if !k.trim().is_empty() => k.trim().to_string(),
            _ => format!("sk-wire-{:016x}{:016x}", rand::random::<u64>(), rand::random::<u64>()),
        };

        let key_preview = if key_str.len() <= 12 {
            key_str.clone()
        } else {
            let prefix = &key_str[..8];
            let suffix = &key_str[key_str.len() - 4..];
            format!("{}...{}", prefix, suffix)
        };

        conn.execute(
            "INSERT INTO api_keys (id, name, key_val, key_preview) VALUES (?1, ?2, ?3, ?4)",
            params![id, name, key_str, key_preview],
        )
        .map_err(|e| OxllmError::Internal(format!("Failed to insert API key: {}", e)))?;

        let created_at: String = conn
            .query_row("SELECT created_at FROM api_keys WHERE id = ?1", params![id], |row| row.get(0))
            .unwrap_or_else(|_| "Just now".to_string());

        Ok(ApiKeyRecord {
            id,
            name: name.to_string(),
            key_preview,
            full_key: Some(key_str),
            created_at,
        })
    }

    /// Deletes an API key by ID
    pub fn delete_api_key(&self, id: &str) -> Result<()> {
        let conn = self.connect()?;
        conn.execute("DELETE FROM api_keys WHERE id = ?1", params![id])
            .map_err(|e| OxllmError::Internal(format!("Failed to delete API key: {}", e)))?;
        Ok(())
    }

    /// Returns all active secret keys stored in SQLite
    pub fn get_active_api_keys(&self) -> Result<Vec<String>> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT key_val FROM api_keys")
            .map_err(|e| OxllmError::Internal(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| OxllmError::Internal(e.to_string()))?;

        let mut res = Vec::new();
        for r in rows {
            if let Ok(key) = r {
                res.push(key);
            }
        }
        Ok(res)
    }

    // =========================================================================
    // Central Vault Secrets Management (AES-256-GCM Encrypted at Rest)
    // =========================================================================

    /// Lists all secrets stored in the central encrypted Vault (masked preview).
    /// Plaintext secrets are never returned.
    pub fn list_vault_secrets(&self) -> Result<Vec<VaultSecretRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT name, key_preview, description, created_at, updated_at FROM vault_secrets ORDER BY name ASC")
            .map_err(|e| OxllmError::Internal(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(VaultSecretRecord {
                    name: row.get(0)?,
                    key_preview: row.get(1)?,
                    description: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })
            .map_err(|e| OxllmError::Internal(e.to_string()))?;

        let mut res = Vec::new();
        for r in rows {
            res.push(r.map_err(|e| OxllmError::Internal(e.to_string()))?);
        }
        Ok(res)
    }

    /// Stores or updates an encrypted secret in the central Vault.
    /// Plaintext is encrypted with AES-256-GCM before writing to SQLite.
    pub fn upsert_vault_secret(&self, name: &str, raw_value: &str, description: &str) -> Result<()> {
        let clean_name = name.trim().to_uppercase().replace('-', "_");
        if clean_name.is_empty() {
            return Err(OxllmError::Internal("Vault secret name cannot be empty".to_string()));
        }
        let clean_val = raw_value.trim();
        if clean_val.is_empty() {
            return Err(OxllmError::Internal("Vault secret value cannot be empty".to_string()));
        }

        let enc = crate::vault::encrypt_key(clean_val)?;
        let preview = crate::vault::mask_key_preview(clean_val);
        let conn = self.connect()?;

        conn.execute(
            "INSERT INTO vault_secrets (name, encrypted_value, key_preview, description, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
             ON CONFLICT(name) DO UPDATE SET
                 encrypted_value = ?2,
                 key_preview = ?3,
                 description = ?4,
                 updated_at = CURRENT_TIMESTAMP",
            params![clean_name, enc, preview, description.trim()],
        )
        .map_err(|e| OxllmError::Internal(format!("Failed to save secret to vault: {}", e)))?;

        Ok(())
    }

    /// Deletes a secret from the central Vault.
    pub fn delete_vault_secret(&self, name: &str) -> Result<()> {
        let clean_name = name.trim().to_uppercase().replace('-', "_");
        let conn = self.connect()?;
        conn.execute("DELETE FROM vault_secrets WHERE name = ?1", params![clean_name])
            .map_err(|e| OxllmError::Internal(format!("Failed to delete vault secret: {}", e)))?;
        Ok(())
    }

    /// Retrieves a single vault secret record (masked preview and metadata) by name or alias.
    pub fn get_vault_secret_record(&self, name: &str) -> Result<Option<VaultSecretRecord>> {
        let clean = name.trim().to_uppercase().replace('-', "_");
        let conn = self.connect()?;

        // Candidates: direct name, with/without _API_KEY, with/without _TOKEN
        let mut candidates = vec![clean.clone()];
        if clean.ends_with("_API_KEY") {
            candidates.push(clean[..clean.len() - 8].to_string());
        } else {
            candidates.push(format!("{}_API_KEY", clean));
        }
        if clean.ends_with("_TOKEN") {
            candidates.push(clean[..clean.len() - 6].to_string());
        } else {
            candidates.push(format!("{}_TOKEN", clean));
        }

        for cand in candidates {
            let mut stmt = conn
                .prepare("SELECT name, key_preview, description, created_at, updated_at FROM vault_secrets WHERE name = ?1")
                .map_err(|e| OxllmError::Internal(e.to_string()))?;

            let mut rows = stmt
                .query_map(params![cand], |row| {
                    Ok(VaultSecretRecord {
                        name: row.get(0)?,
                        key_preview: row.get(1)?,
                        description: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                })
                .map_err(|e| OxllmError::Internal(e.to_string()))?;

            if let Some(r) = rows.next() {
                return Ok(Some(r.map_err(|e| OxllmError::Internal(e.to_string()))?));
            }
        }

        Ok(None)
    }

    /// Decrypts and retrieves a secret by exact name from the Vault.
    pub fn get_vault_secret_decrypted(&self, name: &str) -> Result<Option<String>> {
        let clean_name = name.trim().to_uppercase().replace('-', "_");
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT encrypted_value FROM vault_secrets WHERE name = ?1")
            .map_err(|e| OxllmError::Internal(e.to_string()))?;

        let mut rows = stmt
            .query_map(params![clean_name], |row| row.get::<_, String>(0))
            .map_err(|e| OxllmError::Internal(e.to_string()))?;

        if let Some(r) = rows.next() {
            let enc: String = r.map_err(|e| OxllmError::Internal(e.to_string()))?;
            let decrypted = crate::vault::decrypt_key(&enc)?;
            Ok(Some(decrypted))
        } else {
            Ok(None)
        }
    }

    /// Retrieves a secret from the central Vault checking common alias variations.
    pub fn get_vault_secret_with_aliases(&self, name: &str) -> Result<Option<String>> {
        let clean = name.trim().to_uppercase().replace('-', "_");

        // 1. Direct name match (e.g. NVIDIA_API_KEY)
        if let Some(val) = self.get_vault_secret_decrypted(&clean)? {
            return Ok(Some(val));
        }

        // 2. If name ends with _API_KEY, check base name (e.g. NVIDIA_API_KEY -> NVIDIA)
        if clean.ends_with("_API_KEY") {
            let base = &clean[..clean.len() - 8];
            if let Some(val) = self.get_vault_secret_decrypted(base)? {
                return Ok(Some(val));
            }
        } else {
            // Check with _API_KEY suffix (e.g. NVIDIA -> NVIDIA_API_KEY)
            let with_suffix = format!("{}_API_KEY", clean);
            if let Some(val) = self.get_vault_secret_decrypted(&with_suffix)? {
                return Ok(Some(val));
            }
        }

        // 3. If name ends with _TOKEN, check base name and with _API_KEY
        if clean.ends_with("_TOKEN") {
            let base = &clean[..clean.len() - 6];
            if let Some(val) = self.get_vault_secret_decrypted(base)? {
                return Ok(Some(val));
            }
            let with_api_key = format!("{}_API_KEY", base);
            if let Some(val) = self.get_vault_secret_decrypted(&with_api_key)? {
                return Ok(Some(val));
            }
        } else {
            let with_token = format!("{}_TOKEN", clean);
            if let Some(val) = self.get_vault_secret_decrypted(&with_token)? {
                return Ok(Some(val));
            }
        }

        // 4. Common provider synonym pairs
        let synonym = match clean.as_str() {
            "OPENROUTER_API_KEY" | "OPENROUTER" => Some("OPENROUTER_TOKEN"),
            "OPENROUTER_TOKEN" => Some("OPENROUTER_API_KEY"),
            "GEMINI_API_KEY" | "GEMINI" => Some("GOOGLE_API_KEY"),
            "GOOGLE_API_KEY" => Some("GEMINI_API_KEY"),
            "OPENCODE_GO_KEY" | "OPENCODE_ZEN_KEY" => Some("OPENCODE_API_KEY"),
            _ => None,
        };

        if let Some(syn) = synonym {
            if let Some(val) = self.get_vault_secret_decrypted(syn)? {
                return Ok(Some(val));
            }
        }

        Ok(None)
    }

    /// Checks if a template string references a valid secret in the central Vault.
    pub fn has_vault_secret(&self, template: &str) -> bool {
        let trimmed = template.trim();
        if trimmed.starts_with("${") && trimmed.ends_with('}') {
            let var_name = &trimmed[2..trimmed.len() - 1];
            self.get_vault_secret_with_aliases(var_name).unwrap_or(None).is_some()
        } else if trimmed.starts_with('$') {
            let var_name = &trimmed[1..];
            self.get_vault_secret_with_aliases(var_name).unwrap_or(None).is_some()
        } else {
            self.get_vault_secret_with_aliases(trimmed).unwrap_or(None).is_some()
        }
    }

    /// Resolves an API key template such as `${NVIDIA_API_KEY}` by first querying
    /// the central encrypted `vault_secrets` store, and falling back to `std::env::var`.
    pub fn resolve_secret_template(&self, raw_content: &str) -> Result<String> {
        let mut expanded = String::new();
        let mut chars = raw_content.char_indices().peekable();
        let mut found_template = false;

        while let Some((idx, ch)) = chars.next() {
            if ch == '$' {
                if let Some(&(_, '{')) = chars.peek() {
                    chars.next(); // Consume '{'
                    found_template = true;

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

                    // 1. Try to fetch from central Vault secrets
                    if let Ok(Some(secret)) = self.get_vault_secret_with_aliases(&var_name) {
                        expanded.push_str(&secret);
                        continue;
                    }

                    // 2. Fall back to standard environment variable resolution
                    match crate::config::expand_env_vars(&format!("${{{}}}", var_name)) {
                        Ok(val) => expanded.push_str(&val),
                        Err(e) => return Err(e),
                    }
                    continue;
                }
            }
            expanded.push(ch);
        }

        if !found_template {
            // Check if raw string is a known secret in the Vault
            if let Ok(Some(secret)) = self.get_vault_secret_with_aliases(raw_content.trim()) {
                return Ok(secret);
            }
        }

        Ok(expanded)
    }

    /// Auto-migrates existing provider encrypted keys into central vault_secrets table
    /// if vault_secrets is currently empty.
    pub fn auto_migrate_provider_keys_to_vault(&self) -> Result<()> {
        let conn = self.connect()?;
        let count: i64 = conn
            .query_row("SELECT count(*) FROM vault_secrets", [], |row| row.get(0))
            .unwrap_or(0);

        if count > 0 {
            return Ok(());
        }

        let mut stmt = conn
            .prepare("SELECT name, encrypted_api_key FROM providers WHERE encrypted_api_key IS NOT NULL AND encrypted_api_key != ''")
            .map_err(|e| OxllmError::Internal(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| OxllmError::Internal(e.to_string()))?;

        for r in rows {
            if let Ok((prov_name, enc_key)) = r {
                if let Ok(decrypted) = crate::vault::decrypt_key(&enc_key) {
                    let secret_name = format!("{}_API_KEY", prov_name.to_uppercase().replace('-', "_"));
                    let preview = crate::vault::mask_key_preview(&decrypted);
                    let _ = conn.execute(
                        "INSERT OR IGNORE INTO vault_secrets (name, encrypted_value, key_preview, description, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
                        params![secret_name, enc_key, preview, format!("Migrated from provider '{}'", prov_name)],
                    );
                }
            }
        }

        Ok(())
    }
}

/// Fallback path to ~/.config/oxllm/config.toml
fn dirs_fallback_config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config").join("oxllm").join("config.toml")
}

/// Automatic quality/smartness estimation based on model parameter count & brand.
fn calculate_default_smartness(provider: &str, models: &[String]) -> u32 {
    let target = format!("{} {}", provider, models.join(" ")).to_lowercase();
    if target.contains("ultra") || target.contains("550b") || target.contains("reasoner") || target.contains("r1") {
        96
    } else if target.contains("chat") || target.contains("v3") || target.contains("v4") || target.contains("deepseek") {
        92
    } else if target.contains("glm-5") || target.contains("longcat") || target.contains("1.6t") {
        88
    } else if target.contains("lightning") || target.contains("north") || target.contains("cohere") {
        85
    } else if target.contains("120b") || target.contains("70b") {
        84
    } else if target.contains("flash") || target.contains("27b") {
        80
    } else {
        75
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DEFAULT_ROUTING_POLICY, RoutingPolicy};

    fn temp_db_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "wirellm_db_test_{}_{}.sqlite",
            tag,
            std::process::id()
        ))
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn test_routing_policy_roundtrip_and_corruption_fallback() {
        let path = temp_db_path("routing_policy");
        cleanup(&path);
        let db = DbManager::new(Some(&path)).unwrap();

        // No persisted row yet → the canonical default.
        assert_eq!(db.get_routing_policy().unwrap(), DEFAULT_ROUTING_POLICY);

        // A custom policy survives a save/load cycle verbatim (restart-safe).
        let mut custom = RoutingPolicy {
            stream_header_timeout_secs: 45,
            nonstream_header_timeout_secs: 90,
            fault_window_secs: 120,
            overload_base_cooldown_secs: 20,
            overload_max_cooldown_secs: 60,
            circuit_failure_threshold: 5,
            circuit_base_cooldown_secs: 45,
            circuit_max_cooldown_secs: 240,
            rate_limit_default_cooldown_secs: 7,
            decay_interval_secs: 300,
        };
        db.save_routing_policy(&custom).unwrap();
        assert_eq!(db.get_routing_policy().unwrap(), custom);

        // Saving twice upserts (no duplicate-key blow-up).
        custom.fault_window_secs = 200;
        db.save_routing_policy(&custom).unwrap();
        assert_eq!(db.get_routing_policy().unwrap(), custom);

        // Malformed JSON must degrade to the default, never error the daemon.
        {
            let conn = db.connect().unwrap();
            conn.execute(
                "UPDATE server_settings SET value = 'not-json{' WHERE key = 'routing_policy_json'",
                [],
            )
            .unwrap();
        }
        assert_eq!(db.get_routing_policy().unwrap(), DEFAULT_ROUTING_POLICY);

        cleanup(&path);
    }

    #[test]
    fn test_upsert_virtual_model_preserves_weights_when_omitted() {
        let path = temp_db_path("vm_weights");
        cleanup(&path);
        let db = DbManager::new(Some(&path)).unwrap();

        // Targets reference providers by name (FK), so create the providers first.
        let hdrs = HashMap::new();
        db.upsert_provider("p", true, "https://x/v1", "${K}", &["m".to_string()], &hdrs, None, None, None, None, None).unwrap();
        db.upsert_provider("p2", true, "https://x/v1", "${K}", &["m2".to_string()], &hdrs, None, None, None, None, None).unwrap();

        let t1 = vec![VirtualModelTarget { provider: "p".into(), model: "m".into() }];
        // Create with explicit weights.
        db.upsert_virtual_model("coder", "desc", &t1, Some((0.9, 0.05, 0.05)))
            .unwrap();
        let vm = db
            .get_virtual_models_detail()
            .unwrap()
            .into_iter()
            .find(|v| v.name == "coder")
            .unwrap();
        assert!((vm.weight_smartness - 0.9).abs() < 1e-9);

        // Updating targets only (weights omitted) must PRESERVE the weights.
        let t2 = vec![VirtualModelTarget { provider: "p2".into(), model: "m2".into() }];
        db.upsert_virtual_model("coder", "desc2", &t2, None).unwrap();
        let vm2 = db
            .get_virtual_models_detail()
            .unwrap()
            .into_iter()
            .find(|v| v.name == "coder")
            .unwrap();
        assert!(
            (vm2.weight_smartness - 0.9).abs() < 1e-9,
            "omitted weights must be preserved, got {}",
            vm2.weight_smartness
        );
        assert_eq!(vm2.targets.len(), 1);
        assert_eq!(vm2.targets[0].provider, "p2");

        // A brand-new row with weights omitted still gets the documented defaults.
        db.upsert_virtual_model("planner", "d", &t1, None).unwrap();
        let vm3 = db
            .get_virtual_models_detail()
            .unwrap()
            .into_iter()
            .find(|v| v.name == "planner")
            .unwrap();
        assert!((vm3.weight_smartness - 0.5).abs() < 1e-9);

        cleanup(&path);
    }
}
