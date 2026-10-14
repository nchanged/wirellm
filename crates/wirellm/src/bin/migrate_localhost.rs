use std::collections::HashMap;
use std::path::PathBuf;
use wirellm_core::config::VirtualModelTarget;
use wirellm_core::db::DbManager;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let db_path = PathBuf::from(&home).join(".wirellm").join("wirellm.db");
    println!("Connecting to SQLite database at {:?}", db_path);
    let db = DbManager::new(Some(db_path.clone()))?;

    // 1. Read keys dynamically from ~/.wirellm/.env or environment variables
    let env_file = PathBuf::from(&home).join(".wirellm").join(".env");
    let mut env_map = HashMap::new();
    if let Ok(content) = std::fs::read_to_string(&env_file) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                env_map.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }

    let get_key = |name: &str, fallback_name: &str| -> String {
        std::env::var(name)
            .or_else(|_| std::env::var(fallback_name))
            .ok()
            .or_else(|| env_map.get(name).cloned())
            .or_else(|| env_map.get(fallback_name).cloned())
            .unwrap_or_default()
    };

    // Clean duplicate alias rows first
    let _ = db.delete_vault_secret("NVIDIA_TOKEN");
    let _ = db.delete_vault_secret("OPENROUTER_TOKEN");

    let secrets = [
        ("NVIDIA_API_KEY", get_key("NVIDIA_API_KEY", "NVIDIA_TOKEN"), "NVIDIA NIM developer API key"),
        ("GROQ_API_KEY", get_key("GROQ_API_KEY", "GROQ_API_KEY"), "GroqCloud production token"),
        ("GEMINI_API_KEY", get_key("GEMINI_API_KEY", "GEMINI_API_KEY"), "Google Gemini Studio API key"),
        ("OPENROUTER_API_KEY", get_key("OPENROUTER_API_KEY", "OPENROUTER_TOKEN"), "OpenRouter multi-model key"),
        ("DEEPSEEK_API_KEY", get_key("DEEPSEEK_API_KEY", "DEEPSEEK_API_KEY"), "DeepSeek Official API key"),
        ("OPENCODE_GO_KEY", get_key("OPENCODE_GO_KEY", "OPENCODE_GO_KEY"), "OpenCode Go API key"),
        ("OPENCODE_ZEN_KEY", get_key("OPENCODE_ZEN_KEY", "OPENCODE_ZEN_KEY"), "OpenCode Zen API key"),
    ];

    for (name, val, desc) in secrets {
        if !val.is_empty() {
            db.upsert_vault_secret(name, &val, desc)?;
            println!("✓ Vault secret configured: {}", name);
        }
    }

    // 2. Clear any old mismatched provider definitions
    // We register both isolated model lanes (for independent rate limits & circuit isolation)
    // and umbrella providers (for direct pass-through).
    let empty_headers = HashMap::new();

    // 2.1 NVIDIA NIM Isolated Model Lanes (all using ${NVIDIA_API_KEY})
    let nvidia_lanes = [
        ("nvidia-nemotron-ultra", "nvidia/nemotron-3-ultra-550b-a55b"),
        ("nvidia-nemotron-lightning", "nvidia/nemotron-3.5-lightning-30b-a3b"),
        ("nvidia-glm-flash", "z-ai/glm-5.3-flash"),
        ("nvidia-deepseek-flash", "deepseek-ai/deepseek-v4.1-flash"),
        ("nvidia-llama-vision", "meta/llama-3.2-11b-vision-instruct"),
    ];
    for (name, model) in nvidia_lanes {
        db.upsert_provider(
            name,
            true,
            "https://integrate.api.nvidia.com/v1/",
            "${NVIDIA_API_KEY}",
            &[model.to_string()],
            &empty_headers,
            Some(40),
            Some(2500),
            Some(1),
            Some(96),
            None,
        )?;
        println!("✓ Configured isolated provider: {} (${{NVIDIA_API_KEY}})", name);
    }

    // 2.2 GroqCloud Isolated Lanes (all using ${GROQ_API_KEY})
    let groq_lanes = [
        ("groq-gpt-oss-120b", "openai/gpt-oss-120b", 30, 1500, 92),
        ("groq-qwen-27b", "qwen/qwen3.8-27b", 30, 1500, 90),
        ("groq-gpt-oss-20b", "openai/gpt-oss-20b", 30, 1500, 88),
    ];
    for (name, model, rpm, min_int, score) in groq_lanes {
        db.upsert_provider(
            name,
            true,
            "https://api.groq.com/openai/v1/",
            "${GROQ_API_KEY}",
            &[model.to_string()],
            &empty_headers,
            Some(rpm),
            Some(min_int),
            Some(2),
            Some(score),
            None,
        )?;
        println!("✓ Configured isolated provider: {} (${{GROQ_API_KEY}})", name);
    }

    // 2.3 Google Gemini Studio Isolated Lane (using ${GEMINI_API_KEY})
    db.upsert_provider(
        "gemini-flash",
        true,
        "https://generativelanguage.googleapis.com/v1beta/openai/",
        "${GEMINI_API_KEY}",
        &["gemini-2.5-flash".to_string()],
        &empty_headers,
        Some(15),
        Some(2000),
        Some(2),
        Some(93),
        None,
    )?;
    println!("✓ Configured isolated provider: gemini-flash (${{GEMINI_API_KEY}})");

    // 2.4 OpenRouter Isolated Lanes (all using ${OPENROUTER_API_KEY})
    let mut openrouter_headers = HashMap::new();
    openrouter_headers.insert("HTTP-Referer".to_string(), "https://oxllm.local".to_string());
    openrouter_headers.insert("X-Title".to_string(), "OxLLM".to_string());

    let openrouter_lanes = [
        ("openrouter-cohere-code", "cohere/north-mini-code:free"),
        ("openrouter-nemotron-ultra", "nvidia/nemotron-3-ultra-550b-a55b:free"),
        ("openrouter-nemotron-lightning", "nvidia/nemotron-3.5-lightning:free"),
        ("openrouter-free-auto", "openrouter/free"),
    ];
    for (name, model) in openrouter_lanes {
        db.upsert_provider(
            name,
            true,
            "https://openrouter.ai/api/v1/",
            "${OPENROUTER_API_KEY}",
            &[model.to_string()],
            &openrouter_headers,
            Some(20),
            Some(2000),
            Some(2),
            Some(85),
            None,
        )?;
        println!("✓ Configured isolated provider: {} (${{OPENROUTER_API_KEY}})", name);
    }

    // 2.5 OpenCode Isolated Lanes (using ${OPENCODE_GO_KEY})
    let mut opencode_headers = HashMap::new();
    opencode_headers.insert("User-Agent".to_string(), "opencode/1.19.0".to_string());
    opencode_headers.insert("x-opencode-session".to_string(), "ses-oxllm-default".to_string());

    let opencode_lanes = [
        ("opencode-go-longcat", "longcat-2.5-preview-free"),
        ("opencode-go-step", "step-5-preview-free"),
    ];
    for (name, model) in opencode_lanes {
        db.upsert_provider(
            name,
            true,
            "https://opencode.ai/zen/go/v1/",
            "${OPENCODE_GO_KEY}",
            &[model.to_string()],
            &opencode_headers,
            Some(20),
            Some(3000),
            Some(2),
            Some(85),
            None,
        )?;
        println!("✓ Configured isolated provider: {} (${{OPENCODE_GO_KEY}})", name);
    }

    // 2.6 DeepSeek Direct Isolated Lanes (using ${DEEPSEEK_API_KEY})
    let deepseek_lanes = [
        ("deepseek-chat", "deepseek-chat"),
        ("deepseek-reasoner", "deepseek-reasoner"),
    ];
    for (name, model) in deepseek_lanes {
        db.upsert_provider(
            name,
            true,
            "https://api.deepseek.com/v1/",
            "${DEEPSEEK_API_KEY}",
            &[model.to_string()],
            &empty_headers,
            Some(30),
            Some(2000),
            Some(2),
            Some(96),
            None,
        )?;
        println!("✓ Configured isolated provider: {} (${{DEEPSEEK_API_KEY}})", name);
    }

    // 2.7 Umbrella / Multi-Model Providers (for direct pass-through requests)
    let nvidia_all = vec![
        "nvidia/nemotron-3-ultra-550b-a55b".to_string(),
        "nvidia/nemotron-3.5-lightning-30b-a3b".to_string(),
        "meta/llama-3.3-70b-instruct".to_string(),
        "deepseek-ai/deepseek-v4.1-flash".to_string(),
        "z-ai/glm-5.3-flash".to_string(),
        "meta/llama-3.2-11b-vision-instruct".to_string(),
    ];
    db.upsert_provider("nvidia", true, "https://integrate.api.nvidia.com/v1/", "${NVIDIA_API_KEY}", &nvidia_all, &empty_headers, Some(40), Some(1200), Some(3), Some(96), None)?;

    let groq_all = vec!["llama-3.3-70b-versatile".into(), "openai/gpt-oss-120b".into(), "openai/gpt-oss-20b".into(), "qwen/qwen3.8-27b".into()];
    db.upsert_provider("groq", true, "https://api.groq.com/openai/v1/", "${GROQ_API_KEY}", &groq_all, &empty_headers, Some(30), Some(1500), Some(2), Some(91), None)?;

    let gemini_all = vec!["gemini-2.5-flash".into(), "gemini-2.0-flash".into()];
    db.upsert_provider("gemini", true, "https://generativelanguage.googleapis.com/v1beta/openai/", "${GEMINI_API_KEY}", &gemini_all, &empty_headers, Some(15), Some(2000), Some(2), Some(93), None)?;

    let openrouter_all = vec!["openrouter/free".into(), "cohere/north-mini-code:free".into(), "nvidia/nemotron-3-ultra-550b-a55b:free".into(), "nvidia/nemotron-3.5-lightning:free".into(), "step-5-preview-free".into()];
    db.upsert_provider("openrouter", true, "https://openrouter.ai/api/v1/", "${OPENROUTER_API_KEY}", &openrouter_all, &openrouter_headers, Some(20), Some(2000), Some(2), Some(85), None)?;

    let opencode_all = vec!["longcat-2.5-preview-free".into(), "step-5-preview-free".into(), "minimax-01".into()];
    db.upsert_provider("opencode", true, "https://opencode.ai/zen/go/v1/", "${OPENCODE_GO_KEY}", &opencode_all, &opencode_headers, Some(20), Some(3000), Some(2), Some(85), None)?;

    let deepseek_all = vec!["deepseek-chat".into(), "deepseek-reasoner".into()];
    db.upsert_provider("deepseek", true, "https://api.deepseek.com/v1/", "${DEEPSEEK_API_KEY}", &deepseek_all, &empty_headers, Some(30), Some(2000), Some(2), Some(96), None)?;

    // 3. Virtual Models with Isolated Target Lanes (Each model has its own circuit and rate-limit state)
    let coder_targets = vec![
        VirtualModelTarget { provider: "nvidia-nemotron-ultra".into(), model: "nvidia/nemotron-3-ultra-550b-a55b".into() },
        VirtualModelTarget { provider: "nvidia-nemotron-lightning".into(), model: "nvidia/nemotron-3.5-lightning-30b-a3b".into() },
        VirtualModelTarget { provider: "nvidia-glm-flash".into(), model: "z-ai/glm-5.3-flash".into() },
        VirtualModelTarget { provider: "nvidia-deepseek-flash".into(), model: "deepseek-ai/deepseek-v4.1-flash".into() },
        VirtualModelTarget { provider: "openrouter-cohere-code".into(), model: "cohere/north-mini-code:free".into() },
        VirtualModelTarget { provider: "opencode-go-longcat".into(), model: "longcat-2.5-preview-free".into() },
        VirtualModelTarget { provider: "opencode-go-step".into(), model: "step-5-preview-free".into() },
        VirtualModelTarget { provider: "openrouter-nemotron-ultra".into(), model: "nvidia/nemotron-3-ultra-550b-a55b:free".into() },
        VirtualModelTarget { provider: "openrouter-nemotron-lightning".into(), model: "nvidia/nemotron-3.5-lightning:free".into() },
        VirtualModelTarget { provider: "groq-gpt-oss-120b".into(), model: "openai/gpt-oss-120b".into() },
        VirtualModelTarget { provider: "groq-qwen-27b".into(), model: "qwen/qwen3.8-27b".into() },
        VirtualModelTarget { provider: "gemini-flash".into(), model: "gemini-2.5-flash".into() },
        VirtualModelTarget { provider: "openrouter-free-auto".into(), model: "openrouter/free".into() },
        VirtualModelTarget { provider: "deepseek-chat".into(), model: "deepseek-chat".into() },
        VirtualModelTarget { provider: "deepseek-reasoner".into(), model: "deepseek-reasoner".into() },
    ];
    db.upsert_virtual_model("coder", "High-performance coding chain with auto rate-limit failover", &coder_targets, None)?;
    println!("✓ Updated virtual model: coder ({} targets, fully isolated)", coder_targets.len());

    let planner_targets = vec![
        VirtualModelTarget { provider: "nvidia-nemotron-ultra".into(), model: "nvidia/nemotron-3-ultra-550b-a55b".into() },
        VirtualModelTarget { provider: "nvidia-nemotron-lightning".into(), model: "nvidia/nemotron-3.5-lightning-30b-a3b".into() },
        VirtualModelTarget { provider: "nvidia-glm-flash".into(), model: "z-ai/glm-5.3-flash".into() },
        VirtualModelTarget { provider: "nvidia-deepseek-flash".into(), model: "deepseek-ai/deepseek-v4.1-flash".into() },
        VirtualModelTarget { provider: "opencode-go-longcat".into(), model: "longcat-2.5-preview-free".into() },
        VirtualModelTarget { provider: "openrouter-cohere-code".into(), model: "cohere/north-mini-code:free".into() },
        VirtualModelTarget { provider: "openrouter-nemotron-ultra".into(), model: "nvidia/nemotron-3-ultra-550b-a55b:free".into() },
        VirtualModelTarget { provider: "openrouter-nemotron-lightning".into(), model: "nvidia/nemotron-3.5-lightning:free".into() },
        VirtualModelTarget { provider: "opencode-go-step".into(), model: "step-5-preview-free".into() },
        VirtualModelTarget { provider: "gemini-flash".into(), model: "gemini-2.5-flash".into() },
        VirtualModelTarget { provider: "groq-gpt-oss-120b".into(), model: "openai/gpt-oss-120b".into() },
        VirtualModelTarget { provider: "groq-qwen-27b".into(), model: "qwen/qwen3.8-27b".into() },
        VirtualModelTarget { provider: "openrouter-free-auto".into(), model: "openrouter/free".into() },
        VirtualModelTarget { provider: "deepseek-reasoner".into(), model: "deepseek-reasoner".into() },
        VirtualModelTarget { provider: "deepseek-chat".into(), model: "deepseek-chat".into() },
    ];
    db.upsert_virtual_model("planner", "Deep reasoning & architecture analysis ladder", &planner_targets, None)?;
    println!("✓ Updated virtual model: planner ({} targets, fully isolated)", planner_targets.len());

    let reviewer_targets = vec![
        VirtualModelTarget { provider: "nvidia-nemotron-ultra".into(), model: "nvidia/nemotron-3-ultra-550b-a55b".into() },
        VirtualModelTarget { provider: "nvidia-nemotron-lightning".into(), model: "nvidia/nemotron-3.5-lightning-30b-a3b".into() },
        VirtualModelTarget { provider: "nvidia-glm-flash".into(), model: "z-ai/glm-5.3-flash".into() },
        VirtualModelTarget { provider: "nvidia-deepseek-flash".into(), model: "deepseek-ai/deepseek-v4.1-flash".into() },
        VirtualModelTarget { provider: "openrouter-cohere-code".into(), model: "cohere/north-mini-code:free".into() },
        VirtualModelTarget { provider: "opencode-go-longcat".into(), model: "longcat-2.5-preview-free".into() },
        VirtualModelTarget { provider: "openrouter-nemotron-ultra".into(), model: "nvidia/nemotron-3-ultra-550b-a55b:free".into() },
        VirtualModelTarget { provider: "openrouter-nemotron-lightning".into(), model: "nvidia/nemotron-3.5-lightning:free".into() },
        VirtualModelTarget { provider: "opencode-go-step".into(), model: "step-5-preview-free".into() },
        VirtualModelTarget { provider: "groq-gpt-oss-120b".into(), model: "openai/gpt-oss-120b".into() },
        VirtualModelTarget { provider: "groq-qwen-27b".into(), model: "qwen/qwen3.8-27b".into() },
        VirtualModelTarget { provider: "gemini-flash".into(), model: "gemini-2.5-flash".into() },
        VirtualModelTarget { provider: "openrouter-free-auto".into(), model: "openrouter/free".into() },
        VirtualModelTarget { provider: "deepseek-reasoner".into(), model: "deepseek-reasoner".into() },
        VirtualModelTarget { provider: "deepseek-chat".into(), model: "deepseek-chat".into() },
    ];
    db.upsert_virtual_model("code-reviewer", "Multi-provider automated code review ladder", &reviewer_targets, None)?;
    println!("✓ Updated virtual model: code-reviewer ({} targets, fully isolated)", reviewer_targets.len());

    let orchestrator_targets = vec![
        VirtualModelTarget { provider: "nvidia-nemotron-ultra".into(), model: "nvidia/nemotron-3-ultra-550b-a55b".into() },
        VirtualModelTarget { provider: "nvidia-nemotron-lightning".into(), model: "nvidia/nemotron-3.5-lightning-30b-a3b".into() },
        VirtualModelTarget { provider: "nvidia-glm-flash".into(), model: "z-ai/glm-5.3-flash".into() },
        VirtualModelTarget { provider: "nvidia-deepseek-flash".into(), model: "deepseek-ai/deepseek-v4.1-flash".into() },
        VirtualModelTarget { provider: "opencode-go-longcat".into(), model: "longcat-2.5-preview-free".into() },
        VirtualModelTarget { provider: "openrouter-cohere-code".into(), model: "cohere/north-mini-code:free".into() },
        VirtualModelTarget { provider: "openrouter-nemotron-ultra".into(), model: "nvidia/nemotron-3-ultra-550b-a55b:free".into() },
        VirtualModelTarget { provider: "openrouter-nemotron-lightning".into(), model: "nvidia/nemotron-3.5-lightning:free".into() },
        VirtualModelTarget { provider: "opencode-go-step".into(), model: "step-5-preview-free".into() },
        VirtualModelTarget { provider: "gemini-flash".into(), model: "gemini-2.5-flash".into() },
        VirtualModelTarget { provider: "groq-gpt-oss-120b".into(), model: "openai/gpt-oss-120b".into() },
        VirtualModelTarget { provider: "groq-qwen-27b".into(), model: "qwen/qwen3.8-27b".into() },
        VirtualModelTarget { provider: "openrouter-free-auto".into(), model: "openrouter/free".into() },
        VirtualModelTarget { provider: "deepseek-chat".into(), model: "deepseek-chat".into() },
        VirtualModelTarget { provider: "deepseek-reasoner".into(), model: "deepseek-reasoner".into() },
    ];
    db.upsert_virtual_model("orchestrator", "Ultra low-latency tool dispatch and summarization ladder", &orchestrator_targets, None)?;
    println!("✓ Updated virtual model: orchestrator ({} targets, fully isolated)", orchestrator_targets.len());

    println!("Precise migration completed: 7 Vault keys, isolated provider lanes, 4 complete 15-target ladders.");
    Ok(())
}
