use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Resolves the config file path using standard conventions.
///
/// If the given path exists, returns it as-is.
/// Otherwise, checks: `~/.wirellm/config.yaml`, `~/.wirellm/config.yml`, `~/.wirellm/config.toml`
/// and XDG path: `~/.config/wirellm/config.yaml`
pub fn resolve_config_path(given: PathBuf) -> PathBuf {
    let is_default_name = given == PathBuf::from("config.yaml")
        || given == PathBuf::from("config.yml")
        || given == PathBuf::from("config.toml");

    if given.exists() && !is_default_name {
        return given;
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let wirellm_dir = PathBuf::from(&home).join(".wirellm");

    // 1. ~/.wirellm/config.yaml or ~/.wirellm/config.yml
    let home_yaml = wirellm_dir.join("config.yaml");
    if home_yaml.exists() {
        return home_yaml;
    }
    let home_yml = wirellm_dir.join("config.yml");
    if home_yml.exists() {
        return home_yml;
    }

    // 2. ~/.wirellm/config.toml
    let home_toml = wirellm_dir.join("config.toml");
    if home_toml.exists() {
        return home_toml;
    }

    // 3. Fallback to legacy ~/.oxllm/config.yaml if not yet migrated
    let legacy_yaml = PathBuf::from(&home).join(".oxllm").join("config.yaml");
    if legacy_yaml.exists() {
        return legacy_yaml;
    }

    // 4. ~/.config/wirellm/config.yaml / yml / toml
    let xdg_dir = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(&home).join(".config"))
        .join("wirellm");
    for ext in &["config.yaml", "config.yml", "config.toml"] {
        let p = xdg_dir.join(ext);
        if p.exists() {
            return p;
        }
    }

    // 5. Current working directory config.yaml / config.yml / config.toml
    for ext in &["config.yaml", "config.yml", "config.toml"] {
        let p = PathBuf::from(ext);
        if p.exists() {
            return p;
        }
    }

    // Default target: ~/.wirellm/config.yaml
    home_yaml
}

#[derive(Parser, Debug)]
#[command(
    name = "wirellm",
    version = env!("CARGO_PKG_VERSION"),
    author = "nchanged",
    about = "High-performance, secure, local-first LLM router, vault & MCP server in Rust"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Starts the gateway Axum server and telemetry worker
    Serve {
        /// Path to the configuration YAML file (default: ~/.wirellm/config.yaml)
        #[arg(short, long, default_value = "config.yaml", env = "WIRELLM_CONFIG")]
        config: PathBuf,

        /// Port of the gateway server (defaults to config file or database)
        #[arg(short, long, env = "WIRELLM_PORT")]
        port: Option<u16>,

        /// Verbosity level. Repeat for more output: -v (debug), -vv (trace)
        #[arg(short, long, action = clap::ArgAction::Count, default_value_t = 0)]
        verbose: u8,
    },
    /// Parses and validates the configuration syntax and cross-references
    Validate {
        /// Path to the configuration YAML file
        #[arg(short, long, default_value = "config.yaml", env = "WIRELLM_CONFIG")]
        config: PathBuf,
    },
    /// Fetches and displays the active health and circuit status from localhost
    Status {
        /// Port of the running gateway server (defaults to config file or database)
        #[arg(short, long, env = "WIRELLM_PORT")]
        port: Option<u16>,
    },
    /// Triggers hot-reload by sending SIGHUP to the running gateway daemon
    Reload {
        /// PID of the running wirellm process (optional, reads from ~/.wirellm/wirellm.pid by default)
        #[arg(short, long)]
        pid: Option<u32>,
    },
    /// Gracefully stops the running wirellm daemon (sends SIGTERM)
    Stop {
        /// PID of the running wirellm process (optional, reads from ~/.wirellm/wirellm.pid by default)
        #[arg(short, long)]
        pid: Option<u32>,
    },
    /// Starts wirellm in the background as a service daemon
    Start {
        /// Path to the configuration YAML file
        #[arg(short, long, default_value = "config.yaml", env = "WIRELLM_CONFIG")]
        config: PathBuf,

        /// Port of the gateway server (defaults to config file or database)
        #[arg(short, long, env = "WIRELLM_PORT")]
        port: Option<u16>,
    },
    /// Restarts the wirellm service daemon
    Restart {
        /// Path to the configuration YAML file
        #[arg(short, long, default_value = "config.yaml", env = "WIRELLM_CONFIG")]
        config: PathBuf,

        /// Port of the gateway server (defaults to config file or database)
        #[arg(short, long, env = "WIRELLM_PORT")]
        port: Option<u16>,
    },
    /// Streams live daemon logs (~/.wirellm/wirellm.log)
    Logs {
        /// Number of recent lines to display
        #[arg(short, long, default_value_t = 50)]
        lines: usize,

        /// Follow log output (like tail -f, default: true)
        #[arg(short, long, default_value_t = true, action = clap::ArgAction::Set)]
        follow: bool,

        /// Print recent lines and exit immediately without following
        #[arg(long, default_value_t = false)]
        no_follow: bool,
    },
    /// Manage providers (list, offline, online, reset)
    #[command(subcommand)]
    Provider(ProviderCommand),
}

#[derive(Subcommand, Debug)]
pub enum ProviderCommand {
    /// List all providers and their circuit state
    List {
        /// Port of the running gateway server (defaults to config file or database)
        #[arg(short, long, env = "WIRELLM_PORT")]
        port: Option<u16>,
    },
    /// Take a provider offline (circuit breaker + manual disabled)
    Offline {
        /// Name of the provider to take offline
        name: String,
        /// Port of the running gateway server (defaults to config file or database)
        #[arg(short, long, env = "WIRELLM_PORT")]
        port: Option<u16>,
    },
    /// Bring a provider back online
    Online {
        /// Name of the provider to bring online
        name: String,
        /// Port of the running gateway server (defaults to config file or database)
        #[arg(short, long, env = "WIRELLM_PORT")]
        port: Option<u16>,
    },
    /// Reset a provider's circuit breaker, failures, and rate limit
    Reset {
        /// Name of the provider to reset
        name: String,
        /// Port of the running gateway server (defaults to config file or database)
        #[arg(short, long, env = "WIRELLM_PORT")]
        port: Option<u16>,
    },
}
