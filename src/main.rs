use std::sync::Arc;
use ophis_core::agent::{Agent, AgentEvent, ApprovalDecision, Approver};
use ophis_core::config::Config;
use ophis_core::session::Session;
use ophis_core::session_store::SessionStore;

const VERSION: &str = env!("CARGO_PKG_VERSION");

struct ApprovalRequest {
    tool: String,
    detail: String,
    reply: tokio::sync::oneshot::Sender<ApprovalDecision>,
}

struct TuiApprover {
    tx: tokio::sync::mpsc::UnboundedSender<ApprovalRequest>,
}

#[async_trait::async_trait]
impl Approver for TuiApprover {
    async fn approve(&self, tool: &str, detail: &str) -> ApprovalDecision {
        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        let req = ApprovalRequest {
            tool: tool.to_string(),
            detail: detail.to_string(),
            reply,
        };
        if self.tx.send(req).is_err() {
            return ApprovalDecision::Approve;
        }
        reply_rx.await.unwrap_or(ApprovalDecision::Deny)
    }
}

struct CliApprover;

#[async_trait::async_trait]
impl Approver for CliApprover {
    async fn approve(&self, tool: &str, detail: &str) -> ApprovalDecision {
        let prompt = format!("\nApprove `{tool}`? {detail}\n  [y]es / [n]o / [a]ll this session: ");
        let line = tokio::task::spawn_blocking(move || {
            use std::io::Write;
            print!("{prompt}");
            let _ = std::io::stdout().flush();
            let mut input = String::new();
            let _ = std::io::stdin().read_line(&mut input);
            input
        })
        .await
        .unwrap_or_default();
        match line.trim().to_lowercase().as_str() {
            "a" | "all" => ApprovalDecision::ApproveAll,
            "n" | "no" => ApprovalDecision::Deny,
            _ => ApprovalDecision::Approve,
        }
    }
}

pub fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("OPHIS_LOG").unwrap_or_else(|_| "info,tantivy=warn,notify=warn".into()),
        )
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
        "--version" | "-V" => println!("ophis v{VERSION}"),
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
        event::{DisableMouseCapture, EnableMouseCapture},
        execute,
        terminal::{
            disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
            LeaveAlternateScreen,
        },
    };
    use ratatui::backend::CrosstermBackend;
    use ratatui::Terminal;
    use std::sync::Arc;
    use ophis_tui::App;

    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
        orig_hook(info);
    }));

    let working_dir = std::env::current_dir()?;
    let mut config = Config::load()?;

    let mut _verbose = false;

    let mut args_iter = args.iter().skip(2);
    while let Some(arg) = args_iter.next() {
        if let Some(model) = arg.strip_prefix("--model=") {
            config.model = model.to_string();
        } else if arg == "--model" {
            if let Some(model) = args_iter.next() {
                config.model = model.to_string();
            }
        } else if arg == "--safe" {
            config.safe_mode = true;
        } else if arg == "--confirm" {
            config.require_approval = true;
        } else if arg == "--verbose" {
            _verbose = true;
        } else if arg == "--no-repomap" {
            config.no_repomap = true;
        } else if arg == "--format" || arg == "--format=json" {
            // ignored in TUI mode
        } else if let Some(turns) = arg.strip_prefix("--max-turns=") {
            if let Ok(n) = turns.parse::<u32>() {
                config.max_turns = n;
            }
        }
    }

    let model = config.model.clone();

    let rt = tokio::runtime::Runtime::new()?;
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<AgentEvent>();
    let (prompt_tx, mut prompt_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (approval_tx, mut approval_rx) = tokio::sync::mpsc::unbounded_channel::<ApprovalRequest>();

    let agent = rt
        .block_on(Agent::new(config, working_dir.clone()))?
        .with_tui(event_tx.clone())
        .with_session_store(Arc::new(open_store()?))
        .with_approver(Arc::new(TuiApprover { tx: approval_tx }));
    let agent = Arc::new(agent);
    let session = Arc::new(tokio::sync::Mutex::new(Session::new(
        uuid::Uuid::new_v4().to_string(),
        working_dir.display().to_string(),
        model.clone(),
    )));

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(
        stdout,
        Clear(ClearType::All),
        EnterAlternateScreen,
        EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(&model);
    let mut current_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut pending_reply: Option<tokio::sync::oneshot::Sender<ApprovalDecision>> = None;

    loop {
        while let Ok(prompt) = prompt_rx.try_recv() {
            let agent_clone = agent.clone();
            let session_clone = session.clone();
            let event_tx_clone = event_tx.clone();

            app.start_streaming();

            let handle = rt.spawn(async move {
                let mut s = session_clone.lock().await;
                match agent_clone.run(&mut s, &prompt).await {
                    Ok(response) => {
                        if let Some(usage) = &response.usage {
                            let _ = event_tx_clone.send(AgentEvent::Tokens {
                                input: usage.prompt_tokens,
                                output: usage.completion_tokens,
                                turn: 0,
                            });
                        }
                    }
                    Err(e) => {
                        let _ = event_tx_clone.send(AgentEvent::Error(e.to_string()));
                    }
                }
                let _ = event_tx_clone.send(AgentEvent::Done);
            });
            current_task = Some(handle);
        }
        while let Ok(ev) = event_rx.try_recv() {
            match ev {
                AgentEvent::Text(t) => {
                    if app.streaming {
                        app.append_stream(&t);
                    } else {
                        app.add_message(t);
                    }
                }
                AgentEvent::Reasoning(_) => {}
                AgentEvent::Tool(s) => app.add_system_message(s),
                AgentEvent::Status(s) => app.add_system_message(s),
                AgentEvent::Error(e) => app.add_system_message(format!("Error: {e}")),
                AgentEvent::Tokens { input, output, .. } => {
                    app.tokens_in += input;
                    app.tokens_out += output;
                    app.active_ctx_in = input;
                    app.active_ctx_out = output;
                    app.update_cost();
                }
                AgentEvent::ToolCallInfo { .. } => {}
                AgentEvent::ToolResultInfo { .. } => {}
                AgentEvent::TurnEnd { .. } => app.next_turn(),
                AgentEvent::Done => {
                    app.end_streaming();
                    app.status = "Ready".into();
                    current_task = None;
                }
                AgentEvent::Repo(files) => app.repo_files = files,
                AgentEvent::Mcp(servers) => app.mcp_servers = servers,
                AgentEvent::Models(models) => app.available_models = models,
                AgentEvent::Sessions(metas) => {
                    app.sessions = metas
                        .into_iter()
                        .map(|m| ophis_tui::SessionInfo {
                            id: m.id,
                            model: m.model,
                            message_count: m.message_count,
                            total_tokens: m.total_tokens,
                            updated_at: m.updated_at,
                        })
                        .collect();
                }
            }
        }

        while let Ok(req) = approval_rx.try_recv() {
            app.pending_approval = Some((req.tool, req.detail));
            pending_reply = Some(req.reply);
        }

        terminal.draw(|f| ophis_tui::draw(f, &app))?;

        if !app.running {
            break;
        }

        let poll_ms = if app.streaming { 80 } else { 250 };
        if crossterm::event::poll(std::time::Duration::from_millis(poll_ms))? {
            match crossterm::event::read()? {
                crossterm::event::Event::Key(key) => {
                    if key.kind != crossterm::event::KeyEventKind::Press {
                        continue;
                    }
                    if app.pending_approval.is_some() {
                        let decision = match key.code {
                            crossterm::event::KeyCode::Char('y')
                            | crossterm::event::KeyCode::Char('Y')
                            | crossterm::event::KeyCode::Enter => Some(ApprovalDecision::Approve),
                            crossterm::event::KeyCode::Char('a')
                            | crossterm::event::KeyCode::Char('A') => {
                                Some(ApprovalDecision::ApproveAll)
                            }
                            crossterm::event::KeyCode::Char('n')
                            | crossterm::event::KeyCode::Char('N')
                            | crossterm::event::KeyCode::Esc => Some(ApprovalDecision::Deny),
                            _ => None,
                        };
                        if let Some(d) = decision {
                            if let Some(reply) = pending_reply.take() {
                                let _ = reply.send(d);
                            }
                            app.pending_approval = None;
                        }
                        continue;
                    }
                    if app.streaming && key.code == crossterm::event::KeyCode::Esc {
                        if let Some(handle) = current_task.take() {
                            handle.abort();
                        }
                        app.end_streaming();
                        app.status = "Ready".into();
                        app.add_system_message("Request cancelled.".into());
                        continue;
                    }
                    if key.code == crossterm::event::KeyCode::Enter
                        && key.modifiers.contains(crossterm::event::KeyModifiers::ALT)
                        && !app.streaming
                    {
                        app.handle_key(key);
                        continue;
                    }
                    if key.code == crossterm::event::KeyCode::Enter && !app.streaming {
                        if app.palette_open || app.show_model_picker || app.show_session_picker {
                            if let Some(cmd) = app.handle_key(key) {
                                if let Some(model_name) = cmd.strip_prefix("__MODEL_SELECT__:") {
                                    let model = model_name.to_string();
                                    let session_mutex = session.clone();
                                    let m = model.clone();
                                    rt.spawn(async move {
                                        let mut s = session_mutex.lock().await;
                                        s.model = m;
                                    });
                                    app.model = model;
                                    app.update_cost();
                                    app.add_system_message(format!(
                                        "Switched to model: {}",
                                        model_name
                                    ));
                                } else if let Some(sid) = cmd.strip_prefix("__SESSION_SELECT__:") {
                                    app.add_system_message(format!("Session selected: {sid}. Use 'ophis session resume {sid}' to resume."));
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
                        MouseEventKind::ScrollUp => app.handle_mouse_scroll(mouse.column, mouse.row, -1),
                        MouseEventKind::ScrollDown => app.handle_mouse_scroll(mouse.column, mouse.row, 1),
                        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                            if let Some(cmd) = app.handle_mouse_click(mouse.column, mouse.row) {
                                if cmd == "__APPROVE__" || cmd == "__DENY__" || cmd == "__APPROVE_ALL__" {
                                    let d = match cmd.as_str() {
                                        "__APPROVE__" => Some(ApprovalDecision::Approve),
                                        "__APPROVE_ALL__" => Some(ApprovalDecision::ApproveAll),
                                        "__DENY__" => Some(ApprovalDecision::Deny),
                                        _ => None,
                                    };
                                    if let Some(decision) = d {
                                        if let Some(reply) = pending_reply.take() {
                                            let _ = reply.send(decision);
                                        }
                                        app.pending_approval = None;
                                    }
                                } else if let Some(model) = cmd.strip_prefix("__MODEL_SELECT__:") {
                                    let _ = prompt_tx.send(format!("/model {}", model));
                                } else if let Some(sess) = cmd.strip_prefix("__SESSION_SELECT__:") {
                                    let _ = prompt_tx.send(format!("/session resume {}", sess));
                                } else {
                                    let _ = prompt_tx.send(cmd);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        Clear(ClearType::All),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;

    Ok(())
}

fn run_interactive(args: &[String]) -> anyhow::Result<()> {
    let working_dir = std::env::current_dir()?;
    let mut config = Config::load()?;

    let mut one_shot_prompt: Option<String> = None;
    let mut verbose = false;

    let mut args_iter = args.iter().skip(2);
    while let Some(arg) = args_iter.next() {
        if arg == "--safe" {
            config.safe_mode = true;
        } else if arg == "--confirm" {
            config.require_approval = true;
        } else if arg == "--verbose" {
            verbose = true;
        } else if arg == "--no-repomap" {
            config.no_repomap = true;
        } else if arg == "--format" || arg == "--format=json" {
            // parsed, handled downstream
        } else if let Some(turns) = arg.strip_prefix("--max-turns=") {
            if let Ok(n) = turns.parse::<u32>() {
                config.max_turns = n;
            }
        } else if let Some(model) = arg.strip_prefix("--model=") {
            config.model = model.to_string();
        } else if arg == "--model" {
            if let Some(model) = args_iter.next() {
                config.model = model.to_string();
            }
        } else if let Some(prompt) = arg.strip_prefix("--prompt=") {
            one_shot_prompt = Some(prompt.to_string());
        } else if arg == "--prompt" {
            if let Some(prompt) = args_iter.next() {
                one_shot_prompt = Some(prompt.to_string());
            }
        }
    }

    let model = config.model.clone();
    let provider = model.split_once('/').map(|(p, _)| p).unwrap_or("unknown");

    let provider_config = config
        .providers
        .get(provider)
        .ok_or_else(|| anyhow::anyhow!("No config for provider '{provider}'. Set {provider}_API_KEY or add to ~/.config/ophis/config.toml"))?;

    if provider_config.api_key.is_empty() && provider != "opencode" {
        anyhow::bail!(
            "No API key for provider '{provider}'. Set {}_API_KEY environment variable.",
            provider.to_uppercase()
        );
    }

    let rt = tokio::runtime::Runtime::new()?;
    let agent = rt
        .block_on(Agent::new(config, working_dir.clone()))?
        .with_session_store(Arc::new(open_store()?))
        .with_approver(Arc::new(CliApprover));
    let mut session = Session::new(
        uuid::Uuid::new_v4().to_string(),
        working_dir.display().to_string(),
        model.clone(),
    );

    if let Some(prompt) = one_shot_prompt {
        let json_output = args.iter().any(|a| a == "--format" || a == "--format=json");

        if json_output {
            let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
            let agent = agent.with_tui(event_tx.clone());

            let session_id = session.id.clone();
            let start = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();

            let header = serde_json::json!({
                "type": "session_start",
                "session": session_id,
                "model": session.model,
                "timestamp": start,
            });
            println!("{}", header);

            let result = rt.block_on(agent.run(&mut session, &prompt));

            while let Some(event) = event_rx.blocking_recv() {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                match event {
                    AgentEvent::TurnEnd { turn, tokens } => {
                        let entry = serde_json::json!({
                            "type": "turn_end",
                            "turn": turn,
                            "tokens": {
                                "input": tokens.prompt_tokens,
                                "output": tokens.completion_tokens,
                                "total": tokens.total_tokens,
                            },
                            "timestamp": now,
                        });
                        println!("{}", entry);
                    }
                    AgentEvent::ToolCallInfo { id, name, input, turn } => {
                        let entry = serde_json::json!({
                            "type": "tool_call",
                            "turn": turn,
                            "id": id,
                            "tool": name,
                            "input": input,
                            "timestamp": now,
                        });
                        println!("{}", entry);
                    }
                    AgentEvent::ToolResultInfo { id, content, is_error, turn, diff, metadata } => {
                        let entry = serde_json::json!({
                            "type": "tool_result",
                            "turn": turn,
                            "id": id,
                            "content": content,
                            "is_error": is_error,
                            "diff": diff,
                            "metadata": metadata,
                            "timestamp": now,
                        });
                        println!("{}", entry);
                    }
                    AgentEvent::Text(t) => {
                        let entry = serde_json::json!({
                            "type": "text",
                            "content": t,
                            "timestamp": now,
                        });
                        println!("{}", entry);
                    }
                    AgentEvent::Error(e) => {
                        let entry = serde_json::json!({
                            "type": "error",
                            "content": e,
                            "timestamp": now,
                        });
                        println!("{}", entry);
                    }
                    AgentEvent::Done => {
                        let entry = serde_json::json!({
                            "type": "done",
                            "timestamp": now,
                        });
                        println!("{}", entry);
                        break;
                    }
                    _ => {}
                }
            }

            if let Ok(response) = result {
                if let Some(usage) = &response.usage {
                    let footer = serde_json::json!({
                        "type": "usage",
                        "tokens": {
                            "input": usage.prompt_tokens,
                            "output": usage.completion_tokens,
                            "total": usage.total_tokens,
                            "cache_read": usage.cache_read_tokens,
                            "cache_write": usage.cache_write_tokens,
                        },
                        "timestamp": std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis(),
                    });
                    println!("{}", footer);
                }
            }
        } else {
            println!("ophis v{VERSION}  |  model: {}", session.model);
            let result = rt.block_on(agent.run(&mut session, &prompt));
            match result {
                Ok(response) => {
                    if let Some(usage) = &response.usage {
                        let cache_read = usage
                            .cache_read_tokens
                            .map(|t| format!(" ({} cached)", t))
                            .unwrap_or_default();
                        let cache_write = usage
                            .cache_write_tokens
                            .map(|t| format!(" ({} written)", t))
                            .unwrap_or_default();
                        if verbose {
                            eprintln!(
                                "[ophis] Tokens: {} in + {} out = {}{}{}",
                                usage.prompt_tokens,
                                usage.completion_tokens,
                                usage.total_tokens,
                                cache_read,
                                cache_write,
                            );
                        } else {
                            tracing::debug!(
                                "Tokens: {} in + {} out = {}{}{}",
                                usage.prompt_tokens,
                                usage.completion_tokens,
                                usage.total_tokens,
                                cache_read,
                                cache_write,
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                }
            }
        }
        return Ok(());
    }

    println!("ophis v{VERSION}  |  model: {model}");
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
                    let cache_read = usage
                        .cache_read_tokens
                        .map(|t| format!(" ({} cached)", t))
                        .unwrap_or_default();
                    let cache_write = usage
                        .cache_write_tokens
                        .map(|t| format!(" ({} written)", t))
                        .unwrap_or_default();
                    if verbose {
                        eprintln!(
                            "[ophis] Tokens: {} in + {} out = {}{}{}",
                            usage.prompt_tokens,
                            usage.completion_tokens,
                            usage.total_tokens,
                            cache_read,
                            cache_write,
                        );
                    } else {
                        tracing::debug!(
                            "Tokens: {} in + {} out = {}{}{}",
                            usage.prompt_tokens,
                            usage.completion_tokens,
                            usage.total_tokens,
                            cache_read,
                            cache_write,
                        );
                    }
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
    let dir = ophis_core::default_data_dir()?;
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
    println!(
        "{:<36} {:<30} {:>6} {:>8}  UPDATED",
        "ID", "MODEL", "MSGS", "TOKENS"
    );
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
    let agent = rt
        .block_on(Agent::new(config, std::env::current_dir()?))?
        .with_session_store(Arc::new(store));

    println!(
        "Resumed session: {} ({} messages, {} tokens)",
        id,
        session.messages.len(),
        session.total_tokens
    );

    let mut session = session;
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
        if input == "/quit" || input == "/exit" || input == "/q" {
            break;
        }

        let result = rt.block_on(agent.run(&mut session, &input));
        if let Err(e) = result {
            eprintln!("Error: {e}")
        }
    }
    Ok(())
}

fn show_config() -> anyhow::Result<()> {
    let config_dir = ophis_core::default_config_dir()?;
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

fn handle_palette_command(app: &mut ophis_tui::App, cmd: &str) {
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
            app.add_system_message(
                "Commands: /model /models /session /clear /quit /help /config /toggle-sidebar"
                    .into(),
            );
        }
        "/config" => {
            let dir = ophis_core::default_config_dir().unwrap_or_default();
            app.add_system_message(format!("Config directory: {}", dir.display()));
        }
        "/session" => {
            app.show_session_picker = true;
            app.session_picker_selection = 0;
        }
        _ => {
            app.add_system_message(format!("Unknown command: {cmd}"));
        }
    }
}

fn print_usage(bin: &str) {
    println!("ophis v{VERSION} — the coding agent CLI\n");
    println!("Usage:");
    println!("  {bin} chat [--model=provider/model] [--tui] [--safe] [--confirm] [--verbose]");
    println!("  {bin} chat --prompt=\"prompt\" [--model=provider/model] [--safe] [--verbose]");
    println!("  {bin} models");
    println!("  {bin} config");
    println!("  {bin} --version");
    println!();
    println!("Flags:");
    println!("  --model=<provider/model>  Model to use (e.g., opencode/deepseek-v4-flash-free)");
    println!("  --tui                     Launch the terminal UI");
    println!("  --safe                    Restrict to read-only tools");
    println!("  --confirm                 Require approval before write/edit/bash");
    println!("  --verbose                 Print per-turn token usage");
    println!("  --format json             NDJSON structured output (one-shot mode)");
    println!("  --max-turns=<N>           Max tool-call turns before stopping (default: 100)");
    println!("  --no-repomap              Skip repository map context (faster startup)");
    println!("  --prompt=<text>           One-shot prompt (non-interactive)");
    println!();
    println!("Examples:");
    println!("  {bin} chat --model=deepseek/deepseek-chat");
    println!("  {bin} chat --tui");
    println!("  {bin} chat --prompt=\"explain this project\" --verbose");
    println!("  {bin} chat --prompt=\"refactor this\" --format json");
    println!("  {bin} chat --prompt=\"fix bug\" --max-turns=5");
    println!();
    println!("Environment variables:");
    println!("  DEEPSEEK_API_KEY    DeepSeek API key");
    println!("  ANTHROPIC_API_KEY   Anthropic API key");
    println!("  OPENAI_API_KEY      OpenAI API key");
    println!("  OPENCODE_API_KEY    OpenCode API key (optional for free models)");
    println!("  OPHIS_LOG           Log level (trace, debug, info, warn, error)");
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
