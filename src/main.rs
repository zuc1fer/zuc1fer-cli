use zuc1fer_core::agent::Agent;
use zuc1fer_core::config::Config;
use zuc1fer_core::session::Session;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("ZUC1FER_LOG").unwrap_or_else(|_| "info".into()))
        .init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage(&args[0]);
        return Ok(());
    }

    match args[1].as_str() {
        "chat" | "run" => run_interactive(&args)?,
        "models" => list_models()?,
        "config" => show_config()?,
        "--version" | "-V" => println!("zuc1fer v{VERSION}"),
        "--help" | "-h" => print_usage(&args[0]),
        cmd => {
            eprintln!("Unknown command: {cmd}");
            print_usage(&args[0]);
        }
    }

    Ok(())
}

fn run_interactive(args: &[String]) -> anyhow::Result<()> {
    let working_dir = std::env::current_dir()?;
    let mut config = Config::load()?;

    let mut one_shot_prompt: Option<String> = None;

    for arg in args.iter().skip(2) {
        if arg == "--safe" {
            config.safe_mode = true;
        } else if let Some(model) = arg.strip_prefix("--model=") {
            config.model = model.to_string();
        } else if let Some(prompt) = arg.strip_prefix("--prompt=") {
            one_shot_prompt = Some(prompt.to_string());
        }
    }

    let model = config.model.clone();
    let provider = model.split_once('/').map(|(p, _)| p).unwrap_or("unknown");

    let provider_config = config
        .providers
        .get(provider)
        .ok_or_else(|| anyhow::anyhow!("No config for provider '{provider}'. Set {provider}_API_KEY or add to ~/.config/zuc1fer/config.toml"))?;

    if provider_config.api_key.is_empty() {
        anyhow::bail!(
            "No API key for provider '{provider}'. Set {}_API_KEY environment variable.",
            provider.to_uppercase()
        );
    }

    let agent = Agent::new(config, working_dir.clone());
    let mut session = Session::new(
        uuid::Uuid::new_v4().to_string(),
        working_dir.display().to_string(),
        model.clone(),
    );

    let rt = tokio::runtime::Runtime::new()?;

    if let Some(prompt) = one_shot_prompt {
        println!("zuc1fer v{VERSION}  |  model: {}", session.model);
        let result = rt.block_on(agent.run(&mut session, &prompt));
        match result {
            Ok(response) => {
                if let Some(usage) = &response.usage {
                    let cache_hit = usage
                        .cache_read_tokens
                        .map(|t| format!(" ({} cached)", t))
                        .unwrap_or_default();
                    tracing::debug!(
                        "Tokens: {} in + {} out = {}{}",
                        usage.prompt_tokens,
                        usage.completion_tokens,
                        usage.total_tokens,
                        cache_hit,
                    );
                }
            }
            Err(e) => {
                eprintln!("Error: {e}");
            }
        }
        return Ok(());
    }

    println!("zuc1fer v{VERSION}  |  model: {model}");
    println!("Type /help for commands, /quit to exit.\n");

    loop {
        print!("> ");
        use std::io::Write;
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_string();

        if input.is_empty() {
            continue;
        }

        match input.as_str() {
            "/quit" | "/exit" | "/q" => {
                println!("Bye.");
                break;
            }
            "/help" => {
                print_session_help();
                continue;
            }
            "/models" => {
                println!("Available models:");
                for m in agent.list_models() {
                    println!("  - {m}");
                }
                continue;
            }
            "/clear" => {
                session = Session::new(
                    uuid::Uuid::new_v4().to_string(),
                    working_dir.display().to_string(),
                    session.model.clone(),
                );
                println!("Session cleared.");
                continue;
            }
            "/model" => {
                println!("Current model: {}", session.model);
                continue;
            }
            s if s.starts_with("/model ") => {
                let new_model = s[7..].trim().to_string();
                session.model = new_model;
                println!("Switched to model: {}", session.model);
                continue;
            }
            _ => {}
        }

        let result = rt.block_on(agent.run(&mut session, &input));

        match result {
            Ok(response) => {
                if let Some(usage) = &response.usage {
                    let cache_hit = usage
                        .cache_read_tokens
                        .map(|t| format!(" ({} cached)", t))
                        .unwrap_or_default();
                    tracing::debug!(
                        "Tokens: {} in + {} out = {}{}",
                        usage.prompt_tokens,
                        usage.completion_tokens,
                        usage.total_tokens,
                        cache_hit,
                    );
                }
            }
            Err(e) => {
                eprintln!("Error: {e}");
            }
        }
    }

    Ok(())
}

fn list_models() -> anyhow::Result<()> {
    let config = Config::load()?;
    let working_dir = std::env::current_dir()?;
    let agent = Agent::new(config, working_dir);

    println!("Available models:");
    for m in agent.list_models() {
        println!("  - {m}");
    }
    Ok(())
}

fn show_config() -> anyhow::Result<()> {
    let config_dir = zuc1fer_core::default_config_dir()?;
    let config_path = config_dir.join("config.toml");

    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        println!("Config: {}", config_path.display());
        println!("{content}");
    } else {
        println!("No config file found at {}", config_path.display());
        println!("A default config will be created on first run.");
    }

    Ok(())
}

fn print_usage(bin: &str) {
    println!("zuc1fer v{VERSION} — the coding agent CLI\n");
    println!("Usage:");
    println!("  {bin} chat [--model=provider/model] [--safe]");
    println!("  {bin} models");
    println!("  {bin} config");
    println!("  {bin} --version");
    println!();
    println!("Examples:");
    println!("  {bin} chat --model=deepseek/deepseek-chat");
    println!("  {bin} chat --model=anthropic/claude-sonnet-4-20250514");
    println!("  {bin} chat --model=openai/gpt-4o");
    println!();
    println!("Environment variables:");
    println!("  DEEPSEEK_API_KEY   DeepSeek API key");
    println!("  ANTHROPIC_API_KEY  Anthropic API key");
    println!("  OPENAI_API_KEY     OpenAI API key");
    println!("  ZUC1FER_LOG        Log level (trace, debug, info, warn, error)");
}

fn print_session_help() {
    println!();
    println!("Session commands:");
    println!("  /help              Show this help");
    println!("  /quit, /exit, /q   Exit");
    println!("  /models            List available models");
    println!("  /model             Show current model");
    println!("  /model <model>     Switch model (e.g. /model openai/gpt-4o)");
    println!("  /clear             Clear session history");
    println!();
}
