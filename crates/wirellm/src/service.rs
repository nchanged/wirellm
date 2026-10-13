use std::path::PathBuf;
use wirellm_core::config::Config;
use wirellm_core::db::get_wirellm_home;
use crate::cli::resolve_config_path;

pub fn write_pid_file() -> std::io::Result<()> {
    let pid = std::process::id();
    let wirellm_pid = get_wirellm_home().join("wirellm.pid");
    let _ = std::fs::write(wirellm_pid, pid.to_string());

    let can_write_tmp = if let Ok(content) = std::fs::read_to_string("/tmp/wirellm.pid") {
        if let Ok(old_pid) = content.trim().parse::<u32>() {
            let running = std::process::Command::new("kill")
                .args(["-0", &old_pid.to_string()])
                .output()
                .map(|out| out.status.success())
                .unwrap_or(false);
            !running
        } else {
            true
        }
    } else {
        true
    };

    if can_write_tmp {
        let _ = std::fs::write("/tmp/wirellm.pid", pid.to_string());
    }
    Ok(())
}

pub fn read_pid() -> Option<u32> {
    let wirellm_pid = get_wirellm_home().join("wirellm.pid");
    if let Ok(content) = std::fs::read_to_string(&wirellm_pid) {
        if let Ok(p) = content.trim().parse::<u32>() {
            return Some(p);
        }
    }
    if let Ok(content) = std::fs::read_to_string("/tmp/wirellm.pid") {
        if let Ok(p) = content.trim().parse::<u32>() {
            return Some(p);
        }
    }
    // Check legacy oxllm.pid during transition
    let oxllm_pid = get_wirellm_home().join("oxllm.pid");
    if let Ok(content) = std::fs::read_to_string(&oxllm_pid) {
        if let Ok(p) = content.trim().parse::<u32>() {
            return Some(p);
        }
    }
    if let Ok(content) = std::fs::read_to_string("/tmp/oxllm.pid") {
        if let Ok(p) = content.trim().parse::<u32>() {
            return Some(p);
        }
    }
    None
}

pub fn send_sighup(pid: u32) -> std::io::Result<()> {
    let status = std::process::Command::new("kill")
        .args(["-HUP", &pid.to_string()])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "kill command failed with exit code: {:?}",
            status.code()
        )))
    }
}

pub fn resolve_service_port(port: Option<u16>, config_path: Option<&PathBuf>) -> u16 {
    if let Some(p) = port {
        return p;
    }
    if let Ok(db) = wirellm_core::db::DbManager::new(None::<&str>) {
        if let Ok(cfg) = db.load_config() {
            return cfg.server.port;
        }
    }
    let path = if let Some(cp) = config_path {
        resolve_config_path(cp.clone())
    } else {
        resolve_config_path(PathBuf::from("config.yaml"))
    };
    if let Ok(cfg) = Config::load_from_file(&path) {
        return cfg.server.port;
    }
    8765
}

pub fn run_validate(config_path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = resolve_config_path(config_path);
    let config = Config::load_from_file(&config_path)?;
    config.validate()?;
    println!("Configuration file at '{:?}' is VALID!", config_path);
    Ok(())
}

pub async fn run_status(port: Option<u16>) -> Result<(), Box<dyn std::error::Error>> {
    let port = resolve_service_port(port, None);
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/status", port);
    let res = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) if e.is_connect() => {
            println!("wirellm is not running on http://127.0.0.1:{}", port);
            println!("Start it with: wirellm serve  (or wirellm start)");
            return Ok(());
        },
        Err(e) => return Err(e.into()),
    };
    if !res.status().is_success() {
        return Err(format!("Server returned error status: {}", res.status()).into());
    }

    #[derive(serde::Deserialize)]
    struct ProviderStatus {
        name: String,
        models: String,
        circuit: String,
        failures: u32,
        rate_limited: bool,
        requests: u64,
        successes: u64,
        tokens_input: u64,
        tokens_output: u64,
        last_request: String,
    }

    #[derive(serde::Deserialize)]
    struct RouteEntry {
        provider: String,
        model: String,
        circuit: String,
        requests: u64,
        successes: u64,
    }

    #[derive(serde::Deserialize)]
    struct StatusResponse {
        uptime_secs: u64,
        total_requests: u64,
        providers: Vec<ProviderStatus>,
        virtual_models: std::collections::HashMap<String, Vec<RouteEntry>>,
    }

    let status: StatusResponse = res.json().await?;

    let uptime_mins = status.uptime_secs / 60;
    let uptime_secs_rem = status.uptime_secs % 60;
    println!(
        "\nUptime: {}m {}s  |  Total Requests: {}",
        uptime_mins, uptime_secs_rem, status.total_requests
    );

    // Virtual model routing tables
    for (vm_name, routes) in &status.virtual_models {
        println!("\nVirtual Model: {}", vm_name);
        println!("{}", "-".repeat(127));
        println!(
            "| {:<20} | {:<45} | {:<30} | {:>8} | {:>8} |",
            "Provider", "Model", "Circuit", "Requests", "Success"
        );
        println!("{}", "-".repeat(127));
        for entry in routes {
            println!(
                "| {:<20} | {:<45} | {:<30} | {:>8} | {:>8} |",
                entry.provider, entry.model, entry.circuit, entry.requests, entry.successes
            );
        }
    }

    // Per-provider table
    println!(
        "\n+--------------------+-----------------------------------------------+--------------------------------+----------+---------------+----------+-----------+--------------+---------------+--------------+"
    );
    println!("| Provider Name      | Models                                                | Circuit Breaker State          | Failures | Rate Limited? | Requests | Successes | Tokens Input | Tokens Output | Last Request|");
    println!("+--------------------+-----------------------------------------------+--------------------------------+----------+---------------+----------+-----------+--------------+---------------+--------------+");
    for s in &status.providers {
        println!(
            "| {:<18} | {:<45} | {:<30} | {:<8} | {:<13} | {:<8} | {:<9} | {:<12} | {:<13} | {:<12} |",
            s.name,
            s.models,
            s.circuit,
            s.failures,
            if s.rate_limited { "Yes" } else { "No" },
            s.requests,
            s.successes,
            s.tokens_input,
            s.tokens_output,
            s.last_request,
        );
    }
    println!(
        "+--------------------+-----------------------------------------------+--------------------------------+----------+---------------+----------+-----------+--------------+---------------+--------------+\n"
    );

    Ok(())
}

pub fn run_reload(pid_opt: Option<u32>) -> Result<(), Box<dyn std::error::Error>> {
    let pid = match pid_opt.or_else(read_pid) {
        Some(p) => p,
        None => {
            println!("wirellm is not running (no PID file found)");
            println!("Start it with: wirellm serve  (or wirellm start)");
            return Ok(());
        },
    };
    send_sighup(pid)?;
    println!(
        "Successfully sent hot-reload signal (SIGHUP) to process {}",
        pid
    );
    Ok(())
}

pub fn run_stop(pid_opt: Option<u32>) -> Result<(), Box<dyn std::error::Error>> {
    let pid = match pid_opt.or_else(read_pid) {
        Some(p) => p,
        None => {
            println!("wirellm is not running (no PID file found)");
            println!("Start it with: wirellm start");
            return Ok(());
        },
    };
    let status = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()?;
    if status.success() {
        println!("Successfully sent stop signal (SIGTERM) to process {}", pid);
    } else {
        println!("Failed to stop process {}", pid);
    }
    let _ = std::fs::remove_file(get_wirellm_home().join("wirellm.pid"));
    let _ = std::fs::remove_file(get_wirellm_home().join("oxllm.pid"));
    let _ = std::fs::remove_file("/tmp/wirellm.pid");
    let _ = std::fs::remove_file("/tmp/oxllm.pid");
    Ok(())
}

pub async fn run_start(config: PathBuf, port: Option<u16>) -> Result<(), Box<dyn std::error::Error>> {
    let effective_port = resolve_service_port(port, Some(&config));
    if let Some(pid) = read_pid() {
        let running = std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if running {
            println!("🟢 wirellm is already running (PID: {}) on port {}", pid, effective_port);
            println!("   • Dashboard:   http://localhost:{}/ui", effective_port);
            println!("   • MCP Server:  http://localhost:{}/mcp/sse", effective_port);
            println!("   • Stop with:   wirellm stop");
            println!("   • Logs with:   wirellm logs");
            return Ok(());
        }
    }

    let exe = std::env::current_exe()?;
    let wirellm_dir = get_wirellm_home();
    let log_path = wirellm_dir.join("wirellm.log");

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let log_err = log_file.try_clone()?;

    println!("🚀 Starting wirellm daemon on port {}...", effective_port);

    let mut args = vec![
        "serve".to_string(),
        "--config".to_string(),
        config.display().to_string(),
    ];
    if let Some(p) = port {
        args.push("--port".to_string());
        args.push(p.to_string());
    }

    let child = std::process::Command::new(exe)
        .args(&args)
        .stdout(std::process::Stdio::from(log_file))
        .stderr(std::process::Stdio::from(log_err))
        .spawn()?;

    let child_pid = child.id();
    // Allow Axum server half a second to initialize socket
    tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;

    println!("✅ wirellm daemon started successfully (PID: {})", child_pid);
    println!("   • Dashboard:   http://localhost:{}/ui", effective_port);
    println!("   • MCP Server:  http://localhost:{}/mcp/sse", effective_port);
    println!("   • Live Logs:   wirellm logs  (or tail -f {})", log_path.display());
    println!("   • Stop:        wirellm stop");
    Ok(())
}

pub async fn run_restart(config: PathBuf, port: Option<u16>) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔄 Restarting wirellm service...");
    let _ = run_stop(None);
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    run_start(config, port).await
}

pub fn run_logs(lines: usize, follow: bool) -> Result<(), Box<dyn std::error::Error>> {
    let wirellm_dir = get_wirellm_home();
    let log_path = wirellm_dir.join("wirellm.log");

    if !log_path.exists() {
        println!("ℹ️  No log file found at {:?}", log_path);
        println!("Start wirellm with: wirellm start");
        return Ok(());
    }

    if follow {
        let mut cmd = std::process::Command::new("tail");
        cmd.args(["-f", "-n", &lines.to_string(), &log_path.to_string_lossy()]);
        let mut child = cmd.spawn()?;
        let _ = child.wait();
    } else {
        let output = std::process::Command::new("tail")
            .args(["-n", &lines.to_string(), &log_path.to_string_lossy()])
            .output()?;
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
    Ok(())
}

pub async fn run_provider_offline(name: &str, port: Option<u16>) -> Result<(), Box<dyn std::error::Error>> {
    let port = resolve_service_port(port, None);
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/admin/providers/{}/offline", port, name);
    let res = match client.post(&url).send().await {
        Ok(r) => r,
        Err(e) if e.is_connect() => {
            println!("wirellm is not running on http://127.0.0.1:{}", port);
            println!("Start it with: wirellm serve");
            return Ok(());
        },
        Err(e) => return Err(e.into()),
    };
    let body = res.text().await?;
    println!("{}", body);
    Ok(())
}

pub async fn run_provider_online(name: &str, port: Option<u16>) -> Result<(), Box<dyn std::error::Error>> {
    let port = resolve_service_port(port, None);
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/admin/providers/{}/online", port, name);
    let res = match client.post(&url).send().await {
        Ok(r) => r,
        Err(e) if e.is_connect() => {
            println!("wirellm is not running on http://127.0.0.1:{}", port);
            println!("Start it with: wirellm serve");
            return Ok(());
        },
        Err(e) => return Err(e.into()),
    };
    let body = res.text().await?;
    println!("{}", body);
    Ok(())
}

pub async fn run_provider_reset(name: &str, port: Option<u16>) -> Result<(), Box<dyn std::error::Error>> {
    let port = resolve_service_port(port, None);
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/admin/providers/{}/reset", port, name);
    let res = match client.post(&url).send().await {
        Ok(r) => r,
        Err(e) if e.is_connect() => {
            println!("wirellm is not running on http://127.0.0.1:{}", port);
            println!("Start it with: wirellm serve");
            return Ok(());
        },
        Err(e) => return Err(e.into()),
    };
    let body = res.text().await?;
    println!("{}", body);
    Ok(())
}

pub async fn run_provider_list(port: Option<u16>) -> Result<(), Box<dyn std::error::Error>> {
    let port = resolve_service_port(port, None);
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/status", port);
    let res = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) if e.is_connect() => {
            println!("wirellm is not running on http://127.0.0.1:{}", port);
            println!("Start it with: wirellm serve");
            return Ok(());
        },
        Err(e) => return Err(e.into()),
    };

    #[derive(serde::Deserialize)]
    struct RouteEntry {
        provider: String,
        model: String,
        circuit: String,
        requests: u64,
        successes: u64,
    }

    #[derive(serde::Deserialize)]
    struct StatusResponse {
        virtual_models: std::collections::HashMap<String, Vec<RouteEntry>>,
    }

    let status: StatusResponse = res.json().await?;

    let mut seen = std::collections::HashSet::new();
    let mut providers: Vec<&RouteEntry> = Vec::new();

    for routes in status.virtual_models.values() {
        for entry in routes {
            if seen.insert(&entry.provider) {
                providers.push(entry);
            }
        }
    }

    println!();
    println!("+----------------------+-----------------------------------------------+--------------------------------+----------+----------+");
    println!("| Provider             | Model                                         | Circuit                        | Requests | Success  |");
    println!("+----------------------+-----------------------------------------------+--------------------------------+----------+----------+");
    for entry in &providers {
        let icon = if entry.circuit.starts_with("Closed") {
            '✓'
        } else if entry.circuit.starts_with("Half") {
            '⧖'
        } else {
            '✗'
        };
        println!(
            "| {:<20} | {:<45} | {:<30} | {:>8} | {:>8} |",
            format!("{} {}", icon, entry.provider),
            entry.model,
            entry.circuit,
            entry.requests,
            entry.successes,
        );
    }
    println!("+----------------------+-----------------------------------------------+--------------------------------+----------+----------+");
    println!();
    println!("Use 'wirellm provider offline <name>' to take a provider out of rotation.");
    println!("Use 'wirellm provider reset <name>' to clear circuit breaker state.");
    println!();

    Ok(())
}
