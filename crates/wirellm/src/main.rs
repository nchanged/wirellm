use clap::Parser;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub mod admin;
pub mod auth;
pub mod cli;
pub mod mcp;
pub mod routes;
pub mod server;
pub mod service;

pub use server::{ReloadableState, Reloader};

#[cfg(test)]
mod tests;

use cli::{Cli, Commands, ProviderCommand};
use server::run_serve;
use service::{
    run_logs, run_provider_list, run_provider_offline, run_provider_online, run_provider_reset,
    run_reload, run_restart, run_start, run_status, run_stop, run_validate,
};

fn load_dot_env() {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let env_paths = [
        format!("{}/.wirellm/.env", home),
        format!("{}/.oxllm/.env", home),
        format!("{}/.config/wirellm/.env", home),
        format!("{}/.config/oxllm/.env", home),
        ".env".to_string(),
    ];
    for p in &env_paths {
        if let Ok(content) = std::fs::read_to_string(p) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                if let Some((k, v)) = trimmed.split_once('=') {
                    let k = k.trim();
                    let v = v.trim().trim_matches('"').trim_matches('\'');
                    if std::env::var(k).is_err() {
                        std::env::set_var(k, v);
                    }
                }
            }
        }
    }

    // Auto-load credentials from OpenCode auth.json if not already defined in env
    let opencode_auth = format!("{}/.local/share/opencode/auth.json", home);
    if let Ok(content) = std::fs::read_to_string(&opencode_auth) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
            if std::env::var("OPENCODE_GO_KEY").is_err() {
                if let Some(key) = val.get("opencode-go").and_then(|o| o.get("key")).and_then(|k| k.as_str()) {
                    if !key.is_empty() {
                        std::env::set_var("OPENCODE_GO_KEY", key);
                    }
                }
            }
            if std::env::var("OPENCODE_ZEN_KEY").is_err() {
                if let Some(key) = val.get("opencode").and_then(|o| o.get("key")).and_then(|k| k.as_str()) {
                    if !key.is_empty() {
                        std::env::set_var("OPENCODE_ZEN_KEY", key);
                    }
                }
            }
            if std::env::var("DEEPSEEK_API_KEY").is_err() {
                if let Some(key) = val.get("deepseek").and_then(|o| o.get("key")).and_then(|k| k.as_str()) {
                    if !key.is_empty() {
                        std::env::set_var("DEEPSEEK_API_KEY", key);
                    }
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_dot_env();
    let cli = Cli::parse();

    // Build the log filter based on verbosity level
    let log_filter = match &cli.command {
        Commands::Serve { verbose, .. } => match verbose {
            0 => "info,wirellm=info,wirellm_core=info",
            1 => "info,wirellm=debug,wirellm_core=debug",
            _ => "trace",
        },
        _ => "info,wirellm=debug,wirellm_core=debug",
    };

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_filter));
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();

    match cli.command {
        Commands::Serve { config, port, .. } => {
            run_serve(config, port).await?;
        },
        Commands::Validate { config } => {
            run_validate(config)?;
        },
        Commands::Status { port } => {
            run_status(port).await?;
        },
        Commands::Reload { pid } => {
            run_reload(pid)?;
        },
        Commands::Stop { pid } => {
            run_stop(pid)?;
        },
        Commands::Start { config, port } => {
            run_start(config, port).await?;
        },
        Commands::Restart { config, port } => {
            run_restart(config, port).await?;
        },
        Commands::Logs { lines, follow, no_follow } => {
            run_logs(lines, follow && !no_follow)?;
        },
        Commands::Provider(cmd) => match cmd {
            ProviderCommand::List { port } => {
                run_provider_list(port).await?;
            },
            ProviderCommand::Offline { name, port } => {
                run_provider_offline(&name, port).await?;
            },
            ProviderCommand::Online { name, port } => {
                run_provider_online(&name, port).await?;
            },
            ProviderCommand::Reset { name, port } => {
                run_provider_reset(&name, port).await?;
            },
        },
    }

    Ok(())
}
