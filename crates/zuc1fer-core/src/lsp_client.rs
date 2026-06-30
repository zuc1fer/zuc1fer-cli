use lsp_types::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

#[derive(Debug)]
struct LspConnection {
    _process: tokio::process::Child,
    writer: Arc<Mutex<tokio::io::BufWriter<tokio::process::ChildStdin>>>,
    next_id: AtomicI64,
    responses: Arc<Mutex<HashMap<i64, serde_json::Value>>>,
    opened: Mutex<HashSet<String>>,
}

pub struct LspClient {
    connections: Mutex<HashMap<String, Arc<LspConnection>>>,
    working_dir: PathBuf,
}

impl LspClient {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            working_dir,
        }
    }

    pub async fn definition(
        &self,
        file_path: &str,
        line: u32,
        character: u32,
    ) -> anyhow::Result<Vec<String>> {
        let lang = language_from_extension(file_path);
        let conn = self.get_or_start_connection(&lang).await?;
        self.ensure_open(conn.clone(), file_path).await?;
        let file_uri = path_to_uri(file_path);
        let uri: Uri = file_uri.parse().map_err(|e| anyhow::anyhow!("invalid uri: {e}"))?;

        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position { line, character },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let response = self
            .send_request(conn, "textDocument/definition", serde_json::to_value(&params)?)
            .await?;

        if response.is_null() {
            return Ok(vec!["No definition found".into()]);
        }

        let locations: Option<GotoDefinitionResponse> = serde_json::from_value(response).ok();
        match locations {
            Some(GotoDefinitionResponse::Scalar(loc)) => Ok(vec![format_location(&loc)]),
            Some(GotoDefinitionResponse::Array(locs)) => {
                if locs.is_empty() {
                    Ok(vec!["No definition found".into()])
                } else {
                    Ok(locs.iter().map(format_location).collect())
                }
            }
            Some(GotoDefinitionResponse::Link(links)) => {
                Ok(links.iter().map(format_location_link).collect())
            }
            None => Ok(vec!["No definition found".into()]),
        }
    }

    pub async fn references(
        &self,
        file_path: &str,
        line: u32,
        character: u32,
    ) -> anyhow::Result<Vec<String>> {
        let lang = language_from_extension(file_path);
        let conn = self.get_or_start_connection(&lang).await?;
        self.ensure_open(conn.clone(), file_path).await?;
        let file_uri = path_to_uri(file_path);
        let uri: Uri = file_uri.parse().map_err(|e| anyhow::anyhow!("invalid uri: {e}"))?;

        let params = ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position { line, character },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: ReferenceContext { include_declaration: true },
        };

        let response = self
            .send_request(conn, "textDocument/references", serde_json::to_value(&params)?)
            .await?;

        if response.is_null() {
            return Ok(vec!["No references found".into()]);
        }

        let locations: Option<Vec<Location>> = serde_json::from_value(response).ok();
        match locations {
            Some(locs) if locs.is_empty() => Ok(vec!["No references found".into()]),
            Some(locs) => Ok(locs.iter().map(format_location).collect()),
            None => Ok(vec!["No references found".into()]),
        }
    }

    pub async fn hover(
        &self,
        file_path: &str,
        line: u32,
        character: u32,
    ) -> anyhow::Result<String> {
        let lang = language_from_extension(file_path);
        let conn = self.get_or_start_connection(&lang).await?;
        self.ensure_open(conn.clone(), file_path).await?;
        let file_uri = path_to_uri(file_path);
        let uri: Uri = file_uri.parse().map_err(|e| anyhow::anyhow!("invalid uri: {e}"))?;

        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position { line, character },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        let response = self
            .send_request(conn, "textDocument/hover", serde_json::to_value(&params)?)
            .await?;

        if response.is_null() {
            return Ok("No hover information available".into());
        }

        let hover: Option<Hover> = serde_json::from_value(response).ok();
        match hover {
            Some(h) => Ok(format_hover_contents(&h.contents)),
            None => Ok("No hover information available".into()),
        }
    }

    pub async fn diagnostics(&self, file_path: &str) -> anyhow::Result<Vec<String>> {
        let lang = language_from_extension(file_path);
        let conn = self.get_or_start_connection(&lang).await?;
        let file_uri = path_to_uri(file_path);
        let uri: Uri = file_uri.parse().map_err(|e| anyhow::anyhow!("invalid uri: {e}"))?;

        self.ensure_open(conn.clone(), file_path).await?;

        let params = DocumentDiagnosticParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            identifier: None,
            previous_result_id: None,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let response = self
            .send_request(
                conn.clone(),
                "textDocument/diagnostic",
                serde_json::to_value(&params)?,
            )
            .await;

        match response {
            Ok(val) => {
                let result: Option<RelatedFullDocumentDiagnosticReport> = serde_json::from_value(val).ok();
                match result {
                    Some(report) => Ok(report
                        .full_document_diagnostic_report
                        .items
                        .iter()
                        .map(|d| {
                            format!(
                                "L{}:C{} [{}] {}",
                                d.range.start.line + 1,
                                d.range.start.character + 1,
                                severity_label(d.severity),
                                d.message
                            )
                        })
                        .collect()),
                    _ => Ok(vec![]),
                }
            }
            Err(_) => {
                let params = lsp_types::PublishDiagnosticsParams {
                    uri: uri.clone(),
                    diagnostics: vec![],
                    version: None,
                };
                let _ = self
                    .send_notification(conn, "textDocument/publishDiagnostics", serde_json::to_value(&params)?)
                    .await;
                Ok(vec![])
            }
        }
    }

    async fn get_or_start_connection(&self, language: &str) -> anyhow::Result<Arc<LspConnection>> {
        let mut conns = self.connections.lock().await;
        if let Some(conn) = conns.get(language) {
            return Ok(conn.clone());
        }

        let conn = start_lsp_server(language, &self.working_dir).await?;
        let conn = Arc::new(conn);
        conns.insert(language.to_string(), conn.clone());
        Ok(conn)
    }

    async fn did_open(&self, conn: Arc<LspConnection>, file_path: &str) -> anyhow::Result<()> {
        let file_uri = path_to_uri(file_path);
        let uri: Uri = file_uri.parse().map_err(|e| anyhow::anyhow!("invalid uri: {e}"))?;
        let content = std::fs::read_to_string(file_path).unwrap_or_default();

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: language_from_extension(file_path),
                version: 1,
                text: content,
            },
        };

        self.send_notification(conn, "textDocument/didOpen", serde_json::to_value(&params)?)
            .await
    }

    async fn ensure_open(&self, conn: Arc<LspConnection>, file_path: &str) -> anyhow::Result<()> {
        {
            let opened = conn.opened.lock().await;
            if opened.contains(file_path) {
                return Ok(());
            }
        }
        self.did_open(conn.clone(), file_path).await?;
        conn.opened.lock().await.insert(file_path.to_string());
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        Ok(())
    }

    async fn send_request(
        &self,
        conn: Arc<LspConnection>,
        method: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let id = conn.next_id.fetch_add(1, Ordering::SeqCst);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let content = serde_json::to_string(&request)?;
        let mut writer = conn.writer.lock().await;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());
        writer.write_all(header.as_bytes()).await?;
        writer.write_all(content.as_bytes()).await?;
        writer.flush().await?;
        drop(writer);

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(15);
        while start.elapsed() < timeout {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let mut responses = conn.responses.lock().await;
            if let Some(result) = responses.remove(&id) {
                if let Some(err) = result.get("error") {
                    let msg = err["message"].as_str().unwrap_or("LSP error");
                    anyhow::bail!("LSP error: {msg}");
                }
                return Ok(result["result"].clone());
            }
        }

        anyhow::bail!("LSP request timed out")
    }

    async fn send_notification(
        &self,
        conn: Arc<LspConnection>,
        method: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        let content = serde_json::to_string(&notification)?;
        let mut writer = conn.writer.lock().await;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());
        writer.write_all(header.as_bytes()).await?;
        writer.write_all(content.as_bytes()).await?;
        writer.flush().await?;
        Ok(())
    }
}

async fn start_lsp_server(
    language: &str,
    working_dir: &PathBuf,
) -> anyhow::Result<LspConnection> {
    let (program, args) = server_command(language);
    if program.is_empty() {
        anyhow::bail!(
            "No LSP server configured for '{}'. Install one: rust-analyzer, pyright, typescript-language-server, or gopls.",
            language
        );
    }

    let mut child = Command::new(&program)
        .args(&args)
        .current_dir(working_dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;

    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();

    let writer = Arc::new(Mutex::new(tokio::io::BufWriter::new(stdin)));
    let responses: Arc<Mutex<HashMap<i64, serde_json::Value>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let responses_clone = responses.clone();

    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut header = String::new();
        loop {
            header.clear();
            let mut content_length: usize = 0;
            loop {
                header.clear();
                if reader.read_line(&mut header).await.is_err() {
                    return;
                }
                if header == "\r\n" {
                    break;
                }
                if let Some(len_str) = header.trim().strip_prefix("Content-Length: ") {
                    content_length = len_str.trim().parse().unwrap_or(0);
                }
            }

            if content_length == 0 {
                continue;
            }

            let mut body = vec![0u8; content_length];
            if tokio::io::AsyncReadExt::read_exact(&mut reader, &mut body).await.is_err() {
                return;
            }

            if let Ok(msg) = serde_json::from_slice::<serde_json::Value>(&body) {
                if let Some(id) = msg.get("id").and_then(|v| v.as_i64()) {
                    let mut resp = responses_clone.lock().await;
                    resp.insert(id, msg);
                }
            }
        }
    });

    let root_uri = format!(
        "file:///{}",
        working_dir.display().to_string().replace('\\', "/")
    );

    let conn = Arc::new(LspConnection {
        _process: child,
        writer: writer.clone(),
        next_id: AtomicI64::new(1),
        responses,
        opened: Mutex::new(HashSet::new()),
    });

    let init_params = serde_json::json!({
        "processId": std::process::id(),
        "rootUri": root_uri,
        "capabilities": {
            "textDocument": {
                "definition": { "dynamicRegistration": false },
                "references": { "dynamicRegistration": false },
                "hover": {
                    "dynamicRegistration": false,
                    "contentFormat": ["plaintext", "markdown"]
                },
                "diagnostic": { "dynamicRegistration": false }
            }
        },
        "workspaceFolders": [{
            "uri": root_uri,
            "name": "workspace"
        }]
    });

    let response = send_raw_request(conn.clone(), "initialize", init_params).await?;
    if response.get("error").is_some() {
        let msg = response["error"]["message"].as_str().unwrap_or("initialize failed");
        anyhow::bail!("LSP initialize failed: {msg}");
    }

    let initialized = serde_json::json!({});
    send_raw_notification(conn.clone(), "initialized", initialized).await?;

    tracing::info!("LSP server started: {program} for {language}");
    Ok(Arc::try_unwrap(conn).unwrap())
}

async fn send_raw_request(
    conn: Arc<LspConnection>,
    method: &str,
    params: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let id = conn.next_id.fetch_add(1, Ordering::SeqCst);

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });

    let content = serde_json::to_string(&request)?;
    let mut writer = conn.writer.lock().await;
    let header = format!("Content-Length: {}\r\n\r\n", content.len());
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(content.as_bytes()).await?;
    writer.flush().await?;
    drop(writer);

    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(30);
    while start.elapsed() < timeout {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let mut responses = conn.responses.lock().await;
        if let Some(result) = responses.remove(&id) {
            return Ok(result);
        }
    }

    anyhow::bail!("LSP initialize timed out")
}

async fn send_raw_notification(
    conn: Arc<LspConnection>,
    method: &str,
    params: serde_json::Value,
) -> anyhow::Result<()> {
    let notification = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    });

    let content = serde_json::to_string(&notification)?;
    let mut writer = conn.writer.lock().await;
    let header = format!("Content-Length: {}\r\n\r\n", content.len());
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(content.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

fn language_from_extension(file_path: &str) -> String {
    let ext = std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "rs" => "rust".into(),
        "py" => "python".into(),
        "js" | "jsx" | "ts" | "tsx" => "typescript".into(),
        "go" => "go".into(),
        _ => ext.to_string(),
    }
}

fn server_command(language: &str) -> (String, Vec<String>) {
    match language {
        "rust" => ("rust-analyzer".into(), vec![]),
        "python" => ("pyright-langserver".into(), vec!["--stdio".into()]),
        "typescript" => (
            "typescript-language-server".into(),
            vec!["--stdio".into()],
        ),
        "go" => ("gopls".into(), vec![]),
        _ => ("".into(), vec![]),
    }
}

fn path_to_uri(file_path: &str) -> String {
    format!("file:///{}", file_path.replace('\\', "/"))
}

fn format_location(loc: &Location) -> String {
    format!(
        "{}:{}:{}",
        loc.uri.to_string().trim_start_matches("file:///"),
        loc.range.start.line + 1,
        loc.range.start.character + 1,
    )
}

fn format_location_link(link: &LocationLink) -> String {
    format!(
        "{}:{}:{}",
        link.target_uri
            .to_string()
            .trim_start_matches("file:///"),
        link.target_selection_range.start.line + 1,
        link.target_selection_range.start.character + 1,
    )
}

fn format_hover_contents(contents: &HoverContents) -> String {
    match contents {
        HoverContents::Scalar(MarkedString::String(s)) => s.clone(),
        HoverContents::Scalar(MarkedString::LanguageString(ls)) => ls.value.clone(),
        HoverContents::Array(items) => items
            .iter()
            .map(|i| match i {
                MarkedString::String(s) => s.clone(),
                MarkedString::LanguageString(ls) => ls.value.clone(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        HoverContents::Markup(content) => content.value.clone(),
    }
}

fn severity_label(severity: Option<DiagnosticSeverity>) -> &'static str {
    match severity {
        Some(DiagnosticSeverity::ERROR) => "error",
        Some(DiagnosticSeverity::WARNING) => "warn",
        Some(DiagnosticSeverity::INFORMATION) => "info",
        Some(DiagnosticSeverity::HINT) => "hint",
        _ => "unknown",
    }
}
