use crate::code_index::CodeIndex;
use crate::config::Config;
use crate::indexer::Indexer;
use crate::lsp_tool::LspTool;
use crate::mcp_bridge::McpBridge;
use crate::mcp_tool::McpTool;
use crate::plugin_manager;
use crate::repomap::RepoMap;
use crate::semantic_tool::SemanticTool;
use crate::session::{Session, SessionMessage};
use crate::session_store::SessionStore;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zuc1fer_llm::{
    ChatRequest, ProviderRegistry, StreamEvent, ToolDefinition,
};
use zuc1fer_tools::{ToolCall, ToolContext, ToolRegistry, ToolResult};

const MAX_RETRIES: u32 = 3;
const BASE_BACKOFF_MS: u64 = 1000;

const SYSTEM_PROMPT: &str = r#"You are zuc1fer, a fast and capable CLI coding agent operating directly in the user's terminal and filesystem. The user is a professional developer and is the authority on what they want — act on their behalf decisively and without friction.

<OPERATING_PRINCIPLES>
- Be direct and concise. Answer in 1-3 lines unless the task genuinely requires more or the user asks for detail. No preamble, no filler, no moralizing.
- Default to action. Use your tools to investigate and make changes instead of asking the user to do it or guessing.
- Don't lecture, warn, or editorialize about a request — do the work and report the result.
- When you don't know something, find out with tools (read, search, run commands). Don't speculate.
- See tasks through: if something takes several steps, complete them all before handing back.
</OPERATING_PRINCIPLES>

<TOOL_USE>
- Explore the codebase with tools before answering questions about it.
- Read a file before editing it, and match its existing style and conventions.
- Issue independent tool calls together so they run in parallel.
- Prefer the structured tools (read, edit, write, grep, glob) over shelling out when either works.
- NEVER commit changes unless the user explicitly asks.
- NEVER guess URLs — only use URLs the user provided or that you found in the codebase.
</TOOL_USE>

<PLATFORM_RULES>
- You are running on {os_name}. The shell is {shell_name}. Use {shell_sep} to chain commands, NOT &&.
- Use forward slashes (/) in file paths for consistency across platforms.
- When writing code, double-check every line — missing characters, truncated names, or merged lines are real failures.
</PLATFORM_RULES>

Current working directory: {working_dir}
"#;

#[derive(Debug, Clone)]
pub enum AgentEvent {
    Text(String),
    Reasoning(String),
    Tool(String),
    Status(String),
    Error(String),
    Tokens { input: u64, output: u64 },
    TurnEnd,
    Done,
    Repo(Vec<(String, f64)>),
    Mcp(Vec<(String, bool)>),
    Models(Vec<String>),
    Sessions(Vec<crate::session_store::SessionMeta>),
}

pub struct TuiOutput {
    pub tx: tokio::sync::mpsc::UnboundedSender<AgentEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    Approve,
    ApproveAll,
    Deny,
}

#[async_trait::async_trait]
pub trait Approver: Send + Sync {
    async fn approve(&self, tool: &str, detail: &str) -> ApprovalDecision;
}

pub struct Agent {
    config: Config,
    provider_registry: ProviderRegistry,
    tool_registry: ToolRegistry,
    working_dir: std::path::PathBuf,
    repomap: Option<RepoMap>,
    #[allow(dead_code)]
    code_index: Option<Arc<CodeIndex>>,
    mcp_status: Vec<(String, bool)>,
    #[allow(dead_code)]
    mcp_bridges: Vec<Arc<McpBridge>>,
    tui: Option<TuiOutput>,
    session_store: Option<Arc<SessionStore>>,
    approver: Option<Arc<dyn Approver>>,
    approved_tools: Mutex<HashSet<String>>,
}

impl Agent {
    pub async fn new(config: Config, working_dir: std::path::PathBuf) -> anyhow::Result<Self> {
        let mut provider_registry = ProviderRegistry::new();
        let mut tool_registry = ToolRegistry::new();

        for (name, provider_config) in &config.providers {
            if !provider_config.api_key.is_empty() {
                match name.as_str() {
                    "deepseek" => {
                        let mut p =
                            zuc1fer_llm::providers::deepseek::DeepSeekProvider::new(
                                provider_config.api_key.clone(),
                            );
                        if let Some(ref url) = provider_config.base_url {
                            p = p.with_base_url(url);
                        }
                        provider_registry.register(Box::new(p));
                    }
                    "anthropic" => {
                        provider_registry.register(Box::new(
                            zuc1fer_llm::providers::anthropic::AnthropicProvider::new(
                                provider_config.api_key.clone(),
                            ),
                        ));
                    }
                    "openai" => {
                        let mut p =
                            zuc1fer_llm::providers::openai::OpenAIProvider::new(
                                provider_config.api_key.clone(),
                            );
                        if let Some(ref url) = provider_config.base_url {
                            p = p.with_base_url(url);
                        }
                        provider_registry.register(Box::new(p));
                    }
                    "openrouter" => {
                        if !provider_config.api_key.is_empty() {
                            provider_registry.register(Box::new(
                                zuc1fer_llm::providers::openrouter::OpenRouterProvider::new(
                                    provider_config.api_key.clone(),
                                ),
                            ));
                        }
                    }
                    "ollama" => {
                        let mut p = zuc1fer_llm::providers::ollama::OllamaProvider::new();
                        if let Some(ref url) = provider_config.base_url {
                            p = p.with_base_url(url);
                        }
                        provider_registry.register(Box::new(p));
                    }
                    _ => {
                        tracing::warn!("Unknown provider: {name}");
                    }
                }
            }
        }

        let mut repomap = RepoMap::new(working_dir.clone(), 1024);
        let _ = repomap.build();

        let mut mcp_bridges = Vec::new();
        let mut mcp_status: Vec<(String, bool)> = Vec::new();
        for server in &config.mcp {
            if !server.enabled {
                mcp_status.push((server.command.clone(), false));
                continue;
            }
            match McpBridge::connect(&server.command, &server.args).await {
                Ok(bridge) => {
                    let bridge = Arc::new(bridge);
                    let server_name = bridge.server_name().to_string();
                    let tool_infos: Vec<_> = bridge.tools_info().to_vec();
                    for ti in tool_infos {
                        let mcp_tool = McpTool::new(bridge.clone(), ti, &server_name);
                        tool_registry.register(Arc::new(mcp_tool));
                    }
                    mcp_bridges.push(bridge);
                    mcp_status.push((server.command.clone(), true));
                }
                Err(e) => {
                    tracing::warn!("MCP server '{}' failed to connect: {e}", server.command);
                    mcp_status.push((server.command.clone(), false));
                }
            }
        }

        let code_index = {
            let repo_key = {
                use std::hash::{Hash, Hasher};
                let canon = working_dir
                    .canonicalize()
                    .unwrap_or_else(|_| working_dir.clone());
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                canon.to_string_lossy().hash(&mut hasher);
                format!("{:016x}", hasher.finish())
            };
            let index_dir = crate::default_data_dir()
                .unwrap_or_else(|_| working_dir.join(".zuc1fer").join("index"))
                .join("index")
                .join(&repo_key);
            let ci = Arc::new(CodeIndex::open_or_create(&index_dir)?);

            if ci.file_count() == 0 {
                tracing::info!("Building Tantivy code index...");
                Indexer::build_full_index(&ci, &working_dir)?;
            } else {
                tracing::info!("Tantivy index loaded: {} files", ci.file_count());
            }

            let indexer = Indexer::new(ci.clone(), working_dir.clone());
            indexer.start();

            tool_registry.register(Arc::new(SemanticTool::new(ci.clone())));
            tool_registry.register(Arc::new(LspTool::new(working_dir.clone())));

            let plugins = plugin_manager::discover_plugins(&mut tool_registry)?;
            if !plugins.is_empty() {
                tracing::info!("Loaded {} plugin(s): {}", plugins.len(), plugins.join(", "));
            }

            Some(ci)
        };

        Ok(Self {
            config,
            provider_registry,
            tool_registry,
            working_dir,
            repomap: Some(repomap),
            code_index,
            mcp_status,
            mcp_bridges,
            tui: None,
            session_store: None,
            approver: None,
            approved_tools: Mutex::new(HashSet::new()),
        })
    }

    pub fn with_tui(mut self, tx: tokio::sync::mpsc::UnboundedSender<AgentEvent>) -> Self {
        self.tui = Some(TuiOutput { tx });
        self
    }

    pub fn with_session_store(mut self, store: Arc<SessionStore>) -> Self {
        self.session_store = Some(store);
        self
    }

    pub fn with_approver(mut self, approver: Arc<dyn Approver>) -> Self {
        self.approver = Some(approver);
        self
    }

    async fn gate_approvals(&self, tool_calls: &[ToolCall]) -> HashSet<String> {
        let mut denied = HashSet::new();
        if !self.config.require_approval {
            return denied;
        }
        let approver = match &self.approver {
            Some(a) => a.clone(),
            None => return denied,
        };
        const MUTATING: &[&str] = &["bash", "write", "edit"];
        for tc in tool_calls {
            if !MUTATING.contains(&tc.name.as_str()) {
                continue;
            }
            if self
                .approved_tools
                .lock()
                .map(|s| s.contains(&tc.name))
                .unwrap_or(false)
            {
                continue;
            }
            let detail = approval_detail(&tc.name, &tc.arguments);
            match approver.approve(&tc.name, &detail).await {
                ApprovalDecision::Approve => {}
                ApprovalDecision::ApproveAll => {
                    if let Ok(mut s) = self.approved_tools.lock() {
                        s.insert(tc.name.clone());
                    }
                }
                ApprovalDecision::Deny => {
                    denied.insert(tc.id.clone());
                }
            }
        }
        denied
    }

    fn emit(&self, text: &str) {
        if let Some(ref tui) = self.tui {
            let _ = tui.tx.send(AgentEvent::Text(text.to_string()));
        } else {
            print!("{text}");
        }
    }

    fn emitln(&self, text: &str) {
        if let Some(ref tui) = self.tui {
            let _ = tui.tx.send(AgentEvent::Text(format!("{text}\n")));
        } else {
            println!("{text}");
        }
    }

    fn emit_reasoning(&self, text: &str) {
        if let Some(ref tui) = self.tui {
            let _ = tui.tx.send(AgentEvent::Reasoning(text.to_string()));
        } else {
            eprint!("{text}");
        }
    }

    fn emit_tool(&self, text: &str) {
        if let Some(ref tui) = self.tui {
            let _ = tui.tx.send(AgentEvent::Tool(text.to_string()));
        } else {
            eprintln!("{text}");
        }
    }

    fn emit_status(&self, text: &str) {
        if let Some(ref tui) = self.tui {
            let _ = tui.tx.send(AgentEvent::Status(text.to_string()));
        } else {
            eprintln!("{text}");
        }
    }

    fn emit_error(&self, text: &str) {
        if let Some(ref tui) = self.tui {
            let _ = tui.tx.send(AgentEvent::Error(text.to_string()));
        } else {
            eprintln!("{text}");
        }
    }

    pub fn add_tool(&mut self, tool: Arc<dyn zuc1fer_tools::Tool>) {
        self.tool_registry.register(tool);
    }

    fn emit_repomap(&self) {
        if let (Some(repomap), Some(tui)) = (&self.repomap, &self.tui) {
            let files: Vec<(String, f64)> = repomap
                .file_rankings
                .iter()
                .map(|(path, score)| {
                    (
                        path.strip_prefix(&self.working_dir)
                            .unwrap_or(path)
                            .display()
                            .to_string()
                            .replace('\\', "/"),
                        *score,
                    )
                })
                .collect();
            let _ = tui.tx.send(AgentEvent::Repo(files));
        }
    }

    fn emit_mcp_status(&self) {
        if let Some(ref tui) = self.tui {
            let _ = tui.tx.send(AgentEvent::Mcp(self.mcp_status.clone()));
        }
    }

    fn emit_models(&self) {
        if let Some(ref tui) = self.tui {
            let _ = tui
                .tx
                .send(AgentEvent::Models(self.provider_registry.list_models()));
        }
    }

    fn emit_sessions(&self) {
        if let (Some(store), Some(tui)) = (&self.session_store, &self.tui) {
            if let Ok(sessions) = store.list() {
                let _ = tui.tx.send(AgentEvent::Sessions(sessions));
            }
        }
    }

    pub fn list_models(&self) -> Vec<String> {
        self.provider_registry.list_models()
    }

    async fn maybe_compact(
        &self,
        session: &mut Session,
        provider: &Arc<dyn zuc1fer_llm::LlmProvider>,
        model_name: &str,
    ) {
        let context_limit: u64 = model_context_for_compaction(model_name);
        let token_limit: u64 = (context_limit * 70) / 100;
        let mut total_tokens: u64 = 0;
        for msg in &session.messages {
            total_tokens += provider.estimate_tokens(&msg.content);
        }

        if total_tokens <= token_limit {
            return;
        }

        if session.messages.len() < 8 {
            return;
        }

        let keep_count = 6;
        let mut cut = session.messages.len().saturating_sub(keep_count);
        while cut > 0
            && session
                .messages
                .get(cut)
                .map(|m| m.role == "tool")
                .unwrap_or(false)
        {
            cut -= 1;
        }
        let to_compact: Vec<_> = session.messages.iter().take(cut).cloned().collect();

        if to_compact.is_empty() {
            return;
        }

        let history_text: String = to_compact
            .iter()
            .map(|m| format!("[{}]: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n\n");

        let compact_prompt = format!(
            "Summarize this conversation history concisely. Preserve key decisions, file changes, \
             errors encountered, and important context. Keep it under 500 words.\n\n{history_text}"
        );

        let request = ChatRequest {
            model: model_name.to_string(),
            system: "You are a context summarizer. Output only the summary, no preamble.".into(),
            messages: vec![zuc1fer_llm::Message {
                role: zuc1fer_llm::Role::User,
                content: vec![zuc1fer_llm::ContentBlock::Text {
                    text: compact_prompt,
                }],
            }],
            tools: vec![],
            max_tokens: 1024,
            temperature: Some(0.0),
            top_p: None,
            cache_system: false,
            reasoning_effort: None,
        };

        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
        let provider_clone = provider.clone();

        let handle =
            tokio::spawn(async move { provider_clone.stream_chat(request, event_tx).await });

        let mut summary = String::new();
        while let Some(event) = event_rx.recv().await {
            if let StreamEvent::TextDelta { text } = event {
                summary.push_str(&text);
            }
        }
        let _ = handle.await;

        if summary.is_empty() {
            return;
        }

        let compacted_msg = SessionMessage {
            role: "user".into(),
            content: format!("[Compacted summary of earlier conversation]\n{summary}"),
            tool_calls: None,
            tool_results: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        session.messages = std::iter::once(compacted_msg)
            .chain(
                session
                    .messages
                    .clone()
                    .into_iter()
                    .skip(to_compact.len()),
            )
            .collect();

        tracing::info!(
            "Context compacted: {} messages → 1 summary ({} → {} est tokens)",
            to_compact.len(),
            total_tokens,
            provider.estimate_tokens(&summary)
        );
    }

    pub async fn run(
        &self,
        session: &mut Session,
        user_input: &str,
    ) -> anyhow::Result<AgentResponse> {
        let provider_name = session
            .model
            .split_once('/')
            .map(|(n, _)| n.to_string())
            .unwrap_or_default();
        let model_name = session
            .model
            .split_once('/')
            .map(|(_, m)| m.to_string())
            .unwrap_or_else(|| session.model.clone());

        let provider = self
            .provider_registry
            .get(&provider_name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No provider '{}' for model '{}'. Available: {}",
                    provider_name,
                    session.model,
                    self.provider_registry.provider_names().join(", ")
                )
            })?;

        let _ = self
            .config
            .providers
            .get(&provider_name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No config for provider '{provider_name}'"
                )
            })?;

        self.maybe_compact(session, &provider, &model_name).await;
        self.emit_repomap();
        self.emit_mcp_status();
        self.emit_models();
        self.emit_sessions();

        session.add_message(SessionMessage {
            role: "user".into(),
            content: user_input.to_string(),
            tool_calls: None,
            tool_results: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });

        let tools: Vec<ToolDefinition> = self
            .tool_registry
            .definitions()
            .iter()
            .map(|d| ToolDefinition {
                name: d.name.clone(),
                description: d.description.clone(),
                input_schema: d.parameters.clone(),
            })
            .collect();

        let mut system_prompt = self
            .config
            .system_prompt
            .clone()
            .unwrap_or_else(|| {
                let (os_name, shell_name, shell_sep) = if cfg!(windows) {
                    ("Windows", "PowerShell", ";")
                } else {
                    ("Linux/macOS", "bash/sh", "&&")
                };
                SYSTEM_PROMPT
                    .replace("{working_dir}", &self.working_dir.display().to_string().replace('\\', "/"))
                    .replace("{os_name}", os_name)
                    .replace("{shell_name}", shell_name)
                    .replace("{shell_sep}", shell_sep)
            });

        if let Some(ref repomap) = self.repomap {
            system_prompt.push_str("\n\n---\n\n");
            system_prompt.push_str(&repomap.format_context());
        }

        let supports_caching = provider.supports_prompt_caching();

        let mut turn_count = 0;
        let mut accumulated_usage = zuc1fer_llm::Usage::default();

        loop {
            turn_count += 1;
            if turn_count > self.config.max_turns {
                return Ok(AgentResponse {
                    text: "Max turns reached".into(),
                    tool_calls: Vec::new(),
                    usage: Some(accumulated_usage),
                });
            }

            let messages = session.to_llm_messages();
            let request = ChatRequest {
                model: model_name.clone(),
                system: system_prompt.clone(),
                messages,
                tools: tools.clone(),
                max_tokens: self.config.max_tokens_per_turn,
                temperature: self.config.temperature,
                top_p: None,
                cache_system: supports_caching,
                reasoning_effort: if model_name.contains("reasoner") || model_name.contains("r1") {
                    Some("medium".into())
                } else {
                    None
                },
            };

            let mut text_buf = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();

            let mut turn_ok = false;
            for retry in 0..MAX_RETRIES {
                if retry > 0 {
                    let delay = Duration::from_millis(BASE_BACKOFF_MS * 2u64.pow(retry - 1));
                    self.emit_status(&format!(
                        "(API hiccup, retrying in {}s... attempt {}/{})",
                        delay.as_secs(), retry + 1, MAX_RETRIES
                    ));
                    tokio::time::sleep(delay).await;
                    text_buf.clear();
                    tool_calls.clear();
                }

                let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
                let provider_arc = provider.clone();
                let request_clone = request.clone();

                let provider_handle = tokio::spawn(async move {
                    provider_arc.stream_chat(request_clone, event_tx).await
                });

                let mut got_error = false;

                while let Some(event) = event_rx.recv().await {
                    match event {
                        StreamEvent::TextDelta { text } => {
                            self.emit(&text);
                            text_buf.push_str(&text);
                        }
                        StreamEvent::ReasoningDelta { text } => {
                            self.emit_reasoning(&text);
                        }
                        StreamEvent::TextDone { .. } => {}
                        StreamEvent::ToolUseStart { id, name } => {
                            tracing::debug!("Tool call: {name} ({id})");
                        }
                        StreamEvent::ToolUseDelta { .. } => {}
                        StreamEvent::ToolUseDone { id, name, input } => {
                            tool_calls.push(ToolCall {
                                id,
                                name,
                                arguments: input,
                            });
                        }
                        StreamEvent::Error { message } => {
                            self.emit_status(&format!("Stream error: {message}"));
                            got_error = true;
                            break;
                        }
                        StreamEvent::Done { usage: u } => {
                            accumulated_usage.prompt_tokens += u.prompt_tokens;
                            accumulated_usage.completion_tokens += u.completion_tokens;
                            accumulated_usage.total_tokens += u.total_tokens;
                            if let Some(ct) = u.cache_read_tokens {
                                let existing = accumulated_usage.cache_read_tokens.get_or_insert(0);
                                *existing += ct;
                            }
                            if let Some(ct) = u.cache_write_tokens {
                                let existing = accumulated_usage.cache_write_tokens.get_or_insert(0);
                                *existing += ct;
                            }
                        }
                    }
                }

                match provider_handle.await {
                    Ok(Ok(())) => {
                        if !got_error {
                            turn_ok = true;
                            break;
                        }
                    }
                    Ok(Err(e)) => {
                        if retry == MAX_RETRIES - 1 {
                            self.emit_error(&format!("Provider error: {e}"));
                            return Ok(AgentResponse {
                                text: text_buf,
                                tool_calls,
                                usage: Some(accumulated_usage),
                            });
                        }
                    }
                    Err(e) => {
                        self.emit_error(&format!("Internal error: {e}"));
                        return Ok(AgentResponse {
                            text: String::new(),
                            tool_calls: Vec::new(),
                            usage: Some(accumulated_usage),
                        });
                    }
                }
            }

            if !turn_ok {
                self.emit_error(&format!("Failed after {MAX_RETRIES} retries"));
                return Ok(AgentResponse {
                    text: text_buf,
                    tool_calls,
                    usage: Some(accumulated_usage),
                });
            }

            if !text_buf.is_empty() {
                self.emitln("");
            }

            if tool_calls.is_empty() {
                session.add_message(SessionMessage {
                    role: "assistant".into(),
                    content: text_buf.clone(),
                    tool_calls: None,
                    tool_results: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                });
                return Ok(AgentResponse {
                    text: text_buf,
                    tool_calls,
                    usage: Some(accumulated_usage),
                });
            }

            let ctx = ToolContext {
                working_dir: self.working_dir.clone(),
                safe_mode: self.config.safe_mode,
            };

            let denied = self.gate_approvals(&tool_calls).await;
            let approved_calls: Vec<ToolCall> = tool_calls
                .iter()
                .filter(|tc| !denied.contains(&tc.id))
                .cloned()
                .collect();

            self.emit_tool(&format!("Running {} tool(s)...", approved_calls.len()));

            let executed = self
                .tool_registry
                .execute_parallel(&approved_calls, &ctx)
                .await;

            let results = merge_tool_results(&tool_calls, executed, &denied);

            for result in &results {
                if result.is_error {
                    self.emit_tool(&format!(
                        "  [{}] Error: {}",
                        result.tool_call_id, result.content
                    ));
                } else {
                    let preview: String = result
                        .content
                        .lines()
                        .take(5)
                        .collect::<Vec<_>>()
                        .join("\n");
                    if preview.len() < result.content.len() {
                        self.emit_tool(&format!(
                            "  [{}] {}\n  ... ({} more chars)",
                            result.tool_call_id,
                            preview,
                            result.content.len() - preview.len()
                        ));
                    } else if !preview.is_empty() {
                        self.emit_tool(&format!(
                            "  [{}] {}",
                            result.tool_call_id, preview
                        ));
                    }
                }
            }

            let tool_call_json: Vec<serde_json::Value> = tool_calls
                .iter()
                .map(|tc| {
                    serde_json::json!({
                        "id": tc.id,
                        "name": tc.name,
                        "input": tc.arguments,
                    })
                })
                .collect();

            let tool_result_blocks: Vec<serde_json::Value> = results
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "id": r.tool_call_id,
                        "content": r.content,
                        "is_error": r.is_error,
                    })
                })
                .collect();

            session.add_message(SessionMessage {
                role: "assistant".into(),
                content: text_buf,
                tool_calls: Some(tool_call_json),
                tool_results: None,
                timestamp: chrono::Utc::now().to_rfc3339(),
            });

            session.add_message(SessionMessage {
                role: "tool".into(),
                content: "Tool results".into(),
                tool_calls: None,
                tool_results: Some(tool_result_blocks),
                timestamp: chrono::Utc::now().to_rfc3339(),
            });

            session.total_tokens = accumulated_usage.total_tokens;
            self.save_session(session);

            if let Some(ref tui) = self.tui {
                let _ = tui.tx.send(AgentEvent::TurnEnd);
            }
        }
    }

    fn save_session(&self, session: &Session) {
        if let Some(ref store) = self.session_store {
            if let Err(e) = store.save(
                &session.id,
                &session.model,
                &session.working_dir,
                &session.messages,
                session.total_tokens,
            ) {
                tracing::warn!("Failed to save session: {e}");
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<zuc1fer_llm::Usage>,
}

fn approval_detail(tool: &str, args: &serde_json::Value) -> String {
    match tool {
        "bash" => args["command"].as_str().unwrap_or("").to_string(),
        "write" | "edit" => args["filePath"].as_str().unwrap_or("").to_string(),
        _ => String::new(),
    }
}

fn merge_tool_results(
    tool_calls: &[ToolCall],
    executed: Vec<ToolResult>,
    denied: &HashSet<String>,
) -> Vec<ToolResult> {
    if denied.is_empty() {
        return executed;
    }
    let mut by_id: HashMap<String, ToolResult> = executed
        .into_iter()
        .map(|r| (r.tool_call_id.clone(), r))
        .collect();
    tool_calls
        .iter()
        .filter_map(|tc| {
            if denied.contains(&tc.id) {
                Some(ToolResult::error(&tc.id, "Denied by user."))
            } else {
                by_id.remove(&tc.id)
            }
        })
        .collect()
}

fn model_context_for_compaction(model: &str) -> u64 {
    let lower = model.to_lowercase();
    if lower.contains("gemini") {
        1_048_576
    } else if lower.contains("gpt-4.1") {
        1_047_576
    } else if lower.contains("claude")
        || lower.contains("sonnet")
        || lower.contains("opus")
        || lower.contains("haiku")
    {
        200_000
    } else {
        128_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(id: &str, name: &str) -> ToolCall {
        ToolCall {
            id: id.into(),
            name: name.into(),
            arguments: serde_json::json!({}),
        }
    }

    #[test]
    fn merge_no_denials_is_passthrough() {
        let calls = vec![call("a", "read"), call("b", "read")];
        let executed = vec![ToolResult::success("a", "ra"), ToolResult::success("b", "rb")];
        let merged = merge_tool_results(&calls, executed, &HashSet::new());
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].tool_call_id, "a");
        assert!(!merged[0].is_error);
    }

    #[test]
    fn merge_denied_preserves_order_and_marks_error() {
        let calls = vec![call("a", "read"), call("b", "write"), call("c", "read")];
        let executed = vec![ToolResult::success("c", "rc"), ToolResult::success("a", "ra")];
        let mut denied = HashSet::new();
        denied.insert("b".to_string());
        let merged = merge_tool_results(&calls, executed, &denied);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].tool_call_id, "a");
        assert_eq!(merged[1].tool_call_id, "b");
        assert_eq!(merged[2].tool_call_id, "c");
        assert!(merged[1].is_error);
        assert!(merged[1].content.contains("Denied"));
    }

    #[test]
    fn approval_detail_extracts_command_and_path() {
        assert_eq!(
            approval_detail("bash", &serde_json::json!({"command": "rm -rf /"})),
            "rm -rf /"
        );
        assert_eq!(
            approval_detail("write", &serde_json::json!({"filePath": "/tmp/x"})),
            "/tmp/x"
        );
        assert_eq!(approval_detail("read", &serde_json::json!({})), "");
    }
}
