use zuc1fer_core::agent::Agent;
use zuc1fer_core::config::Config;
use zuc1fer_core::session::Session;
use zuc1fer_core::session_store::SessionStore;
use std::sync::Arc;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("ZUC1FER_LOG").unwrap_or_else(|_| "info,tantivy=warn,notify=warn".into()))
        .init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage(&args[0]);
        return Ok(());
    }

    match args[1].as_str() {
        "chat" | "run" => {
            let use_tui = args.iter().any(|a| a == "--tui");
            if use_tui {
                run_tui(&args)?
            } else {
                run_interactive(&args)?
            }
        }
        "models" => list_models()?,
        "sessions" => list_sessions()?,
        "session" if args.len() >= 3 => {
            match args[2].as_str() {
                "resume" if args.len() >= 4 => resume_session(&args[3])?,
                "delete" if args.len() >= 4 => {
                    let store = open_store()?;
                    store.delete(&args[3])?;
                    println!("Session {} deleted.", args[3]);
                }
                "info" if args.len() >= 4 => {
                    let store = open_store()?;
                    if let Some(s) = store.load(&args[3])? {
                        println!("Session: {}", s.id);
                        println!("Model: {}", s.model);
                        println!("Dir: {}", s.working_dir);
                        println!("Messages: {}", s.messages.len());
                        println!("Tokens: {}", s.total_tokens);
                        println!("Created: {}", s.created_at);
                    } else {
                        println!("Session not found.");
                    }
                }
                cmd => eprintln!("Unknown session command: {cmd}. Use: sessions, session resume <id>, session delete <id>, session info <id>"),
            }
        }
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

fn run_tui(args: &[String]) -> anyhow::Result<()> {
    use crossterm::{
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, Clear, ClearType},
        event::{EnableMouseCapture, DisableMouseCapture},
    };
    use ratatui::backend::CrosstermBackend;
    use ratatui::Terminal;
    use std::sync::Arc;
    use zuc1fer_tui::App;

    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
        orig_hook(info);
    }));

    let working_dir = std::env::current_dir()?;
    let mut config = Config::load()?;

    for arg in args.iter().skip(2) {
        if let Some(model) = arg.strip_prefix("--model=") {
            config.model = model.to_string();
        } else if arg == "--safe" {
            config.safe_mode = true;
        }
    }

    let model = config.model.clone();

    let rt = tokio::runtime::Runtime::new()?;
    let (text_tx, mut text_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (debug_tx, mut debug_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (prompt_tx, mut prompt_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (done_tx, mut done_rx) = tokio::sync::mpsc::unbounded_channel::<bool>();
    let (turn_tx, mut turn_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    let agent = rt.block_on(Agent::new(config, working_dir.clone()))?
        .with_tui(text_tx.clone(), debug_tx.clone(), turn_tx.clone())
        .with_session_store(Arc::new(open_store()?));
    let agent = Arc::new(agent);
    let session = Arc::new(tokio::sync::Mutex::new(Session::new(
        uuid::Uuid::new_v4().to_string(),
        working_dir.display().to_string(),
        model.clone(),
    )));

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, Clear(ClearType::All), EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(&model);

    loop {
        while let Ok(prompt) = prompt_rx.try_recv() {
            let agent_clone = agent.clone();
            let session_clone = session.clone();
            let text_tx_clone = text_tx.clone();
            let done_tx_clone = done_tx.clone();

            app.start_streaming();

            rt.spawn(async move {
                let mut s = session_clone.lock().await;
                let result = agent_clone.run(&mut s, &prompt).await;
                match result {
                    Ok(response) => {
                        if let Some(usage) = &response.usage {
                            let _ = text_tx_clone.send(format!("__TOKENS__:{}:{}", usage.prompt_tokens, usage.completion_tokens));
                        }
                    }
                    Err(e) => {
                        let _ = text_tx_clone.send(format!("__ERROR__:{}", e));
                    }
                }
                let _ = done_tx_clone.send(true);
            });
        }
        while let Ok(text) = text_rx.try_recv() {
            if text.starts_with("__TOKENS__:") {
                if let Some(rest) = text.strip_prefix("__TOKENS__:") {
                    let parts: Vec<&str> = rest.split(':').collect();
                    if parts.len() == 2 {
                        app.tokens_in += parts[0].parse::<u64>().unwrap_or(0);
                        app.tokens_out += parts[1].parse::<u64>().unwrap_or(0);
                        app.update_cost();
                    }
                }
            } else if text.starts_with("__ERROR__:") {
                if let Some(err) = text.strip_prefix("__ERROR__:") {
                    app.add_system_message(format!("Error: {err}"));
                }
            } else if app.streaming {
                app.append_stream(&text);
            } else {
                app.add_message(text);
            }
        }
        while let Ok(dbg) = debug_rx.try_recv() {
            if let Some(rest) = dbg.strip_prefix("__REPO__:") {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(rest) {
                    app.repo_files.clear();
                    if let Some(files) = data["files"].as_array() {
                        for entry in files {
                            if let Some(arr) = entry.as_array() {
                                if arr.len() == 2 {
                                    let path = arr[0].as_str().unwrap_or("").to_string();
                                    let score = arr[1].as_f64().unwrap_or(0.0);
                                    app.repo_files.push((path, score));
                                }
                            }
                        }
                    }
                }
            } else if let Some(rest) = dbg.strip_prefix("__MCP__:") {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(rest) {
                    app.mcp_servers.clear();
                    if let Some(servers) = data["servers"].as_array() {
                        for entry in servers {
                            if let Some(arr) = entry.as_array() {
                                if arr.len() == 2 {
                                    let name = arr[0].as_str().unwrap_or("").to_string();
                                    let connected = arr[1].as_bool().unwrap_or(false);
                                    app.mcp_servers.push((name, connected));
                                }
                            }
                        }
                    }
                }
            } else if let Some(rest) = dbg.strip_prefix("__MODELS__:") {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(rest) {
                    app.available_models.clear();
                    if let Some(models) = data["models"].as_array() {
                        for m in models {
                            if let Some(name) = m.as_str() {
                                app.available_models.push(name.to_string());
                            }
                        }
                    }
                }
            } else if dbg.contains("Running") || dbg.contains("Error") || dbg.contains("retrying") {
                app.add_system_message(dbg);
            }
        }
        while turn_rx.try_recv().is_ok() {
            app.next_turn();
        }
        while let Ok(_) = done_rx.try_recv() {
            app.end_streaming();
            app.status = "Ready".into();
        }

        terminal.draw(|f| zuc1fer_tui::draw(f, &app))?;

        if !app.running {
            break;
        }

        if crossterm::event::poll(std::time::Duration::from_millis(50))? {
            match crossterm::event::read()? {
                crossterm::event::Event::Key(key) => {
                    if key.kind != crossterm::event::KeyEventKind::Press {
                        continue;
                    }
                    if key.code == crossterm::event::KeyCode::Enter && !app.streaming {
                        if app.palette_open || app.show_model_picker {
                            if let Some(cmd) = app.handle_key(key) {
                                if let Some(model) = cmd.strip_prefix("__MODEL_SELECT__:") {
                                    app.add_system_message(format!("Model selected: {model}. Restart with --model={model} to apply."));
                                } else {
                                    handle_palette_command(&mut app, &cmd);
                                }
                            }
                            continue;
                        }
                        let prompt = app.take_input();
                        if prompt.is_empty() {
                            continue;
                        }
                        app.add_user_message(prompt.clone());
                        app.status = "Thinking...".into();
                        let _ = prompt_tx.send(prompt);
                    } else {
                        let _ = app.handle_key(key);
                    }
                }
                crossterm::event::Event::Mouse(mouse) => {
                    use crossterm::event::MouseEventKind;
                    match mouse.kind {
                        MouseEventKind::ScrollUp => app.handle_mouse_scroll(-1),
                        MouseEventKind::ScrollDown => app.handle_mouse_scroll(1),
                        _ => {}
                    }
                }
                _ => {}
            }
            }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), Clear(ClearType::All), LeaveAlternateScreen, DisableMouseCapture)?;

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

    let rt = tokio::runtime::Runtime::new()?;
    let agent = rt.block_on(Agent::new(config, working_dir.clone()))?
        .with_session_store(Arc::new(open_store()?));
    let mut session = Session::new(
        uuid::Uuid::new_v4().to_string(),
        working_dir.display().to_string(),
        model.clone(),
    );

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
    let rt = tokio::runtime::Runtime::new()?;
    let agent = rt.block_on(Agent::new(config, working_dir))?;

    println!("Available models:");
    for m in agent.list_models() {
        println!("  - {m}");
    }
    Ok(())
}

fn open_store() -> anyhow::Result<SessionStore> {
    let dir = zuc1fer_core::default_data_dir()?;
    let db_path = dir.join("sessions.db");
    SessionStore::new(&db_path)
}

fn list_sessions() -> anyhow::Result<()> {
    let store = open_store()?;
    let sessions = store.list()?;
    if sessions.is_empty() {
        println!("No saved sessions.");
        return Ok(());
    }
    println!("{:<36} {:<30} {:>6} {:>8}  {}", "ID", "MODEL", "MSGS", "TOKENS", "UPDATED");
    for s in &sessions {
        println!(
            "{:<36} {:<30} {:>6} {:>8}  {}",
            &s.id[..s.id.len().min(34)],
            &s.model[..s.model.len().min(28)],
            s.message_count,
            s.total_tokens,
            &s.updated_at[..s.updated_at.len().min(19)],
        );
    }
    Ok(())
}

fn resume_session(id: &str) -> anyhow::Result<()> {
    let store = open_store()?;
    let session = match store.load(id)? {
        Some(s) => s,
        None => {
            eprintln!("Session '{id}' not found.");
            return Ok(());
        }
    };
    let config = Config::load()?;
    let rt = tokio::runtime::Runtime::new()?;
    let agent = rt.block_on(Agent::new(config, std::env::current_dir()?))?.with_session_store(Arc::new(store));

    println!("Resumed session: {} ({} messages, {} tokens)", id, session.messages.len(), session.total_tokens);

    let mut session = session;
    loop {
        print!("> ");
        use std::io::Write;
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_string();

        if input.is_empty() { continue; }
        if input == "/quit" || input == "/exit" || input == "/q" { break; }

        let result = rt.block_on(agent.run(&mut session, &input));
        match result {
            Err(e) => eprintln!("Error: {e}"),
            _ => {}
        }
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

fn handle_palette_command(app: &mut zuc1fer_tui::App, cmd: &str) {
    match cmd {
        "/quit" | "/q" => app.running = false,
        "/clear" => {
            app.messages.clear();
            app.tokens_in = 0;
            app.tokens_out = 0;
            app.cost_usd = 0.0;
            app.add_system_message("Session cleared.".into());
        }
        "/toggle-sidebar" => {
            app.show_repo_panel = !app.show_repo_panel;
        }
        "/toggle-repo" => {
            app.show_repo_panel = true;
            app.sidebar_tab = 0;
        }
        "/model" => {
            app.show_model_picker = true;
            app.model_picker_query.clear();
            app.model_picker_selection = 0;
        }
        "/models" => {
            app.show_model_picker = true;
            app.model_picker_query.clear();
            app.model_picker_selection = 0;
        }
        "/help" => {
            app.add_system_message("Commands: /model /models /session /clear /quit /help /config /toggle-sidebar".into());
        }
        "/config" => {
            let dir = zuc1fer_core::default_config_dir().unwrap_or_default();
            app.add_system_message(format!("Config directory: {}", dir.display()));
        }
        "/session" => {
            app.add_system_message("Session management: use 'zuc1fer session list' to see saved sessions.".into());
        }
        _ => {
            app.add_system_message(format!("Unknown command: {cmd}"));
        }
    }
}

fn print_usage(bin: &str) {
    println!("zuc1fer v{VERSION} — the coding agent CLI\n");
    println!("Usage:");
    println!("  {bin} chat [--model=provider/model] [--tui] [--safe]");
    println!("  {bin} models");
    println!("  {bin} config");
    println!("  {bin} --version");
    println!();
    println!("Examples:");
    println!("  {bin} chat --model=deepseek/deepseek-chat");
    println!("  {bin} chat --tui --model=deepseek/deepseek-chat");
    println!("  {bin} chat --prompt=\"explain this project\"");
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
