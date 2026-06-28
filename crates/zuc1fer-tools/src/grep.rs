use crate::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

pub struct GrepTool;

impl GrepTool {
    async fn run_rg(
        &self,
        cwd: &PathBuf,
        pattern: &str,
        include: Option<&str>,
    ) -> anyhow::Result<Vec<String>> {
        let mut cmd = Command::new("rg");
        cmd.arg("--json")
            .arg("--no-config")
            .arg("--no-heading")
            .arg("--color")
            .arg("never")
            .arg("--no-messages");

        if let Some(inc) = include {
            if !inc.is_empty() {
                cmd.arg("--glob").arg(inc);
            }
        }

        cmd.arg("--").arg(pattern);
        cmd.current_dir(cwd);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await?;

        if !output.stderr.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if output.status.code() == Some(2) {
                anyhow::bail!("Invalid regex pattern: {stderr}");
            }
        }

        let text = String::from_utf8_lossy(&output.stdout);

        let mut results = Vec::new();
        for line in text.lines() {
            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                let ty = entry["type"].as_str().unwrap_or("");
                if ty == "match" {
                    let path = entry["data"]["path"]["text"]
                        .as_str()
                        .unwrap_or("unknown");
                    let line_num = entry["data"]["line_number"].as_u64().unwrap_or(0);
                    let text = entry["data"]["lines"]["text"]
                        .as_str()
                        .unwrap_or("")
                        .trim_end();
                    results.push(format!("{path}:{line_num}: {text}"));
                }
            }
        }

        Ok(results)
    }
}

#[async_trait::async_trait]
impl Tool for GrepTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "grep".into(),
            description: "Search file contents using regex patterns via ripgrep. Returns file paths with line numbers and matching lines. Results limited to 100 matches.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to search in (defaults to working directory)"
                    },
                    "include": {
                        "type": "string",
                        "description": "File pattern to filter (e.g., '*.rs', '*.{ts,tsx}')"
                    }
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn execute(&self, call: &ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let pattern = call.arguments["pattern"].as_str().unwrap_or("");
        let search_dir_path = call.arguments["path"].as_str();
        let mut search_dir = search_dir_path
            .map(PathBuf::from)
            .unwrap_or_else(|| ctx.working_dir.clone());

        if !search_dir.exists() {
            if let Some(path_str) = search_dir_path {
                if let Some(alt) = crate::try_fuzzy_path(path_str) {
                    if alt.exists() {
                        search_dir = alt;
                    }
                }
            }
        }
        let include = call.arguments["include"].as_str();

        match self.run_rg(&search_dir, pattern, include).await {
            Ok(matches) => {
                let limit = 100;
                let total = matches.len();
                let output = matches
                    .iter()
                    .take(limit)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n");

                let mut result = ToolResult::success(&call.id, output);
                if total > limit {
                    result
                        .metadata
                        .get_or_insert_with(std::collections::HashMap::new)
                        .insert(
                            "truncated".into(),
                            format!("true ({total} total, showing {limit})"),
                        );
                }
                if total == 0 {
                    return Ok(ToolResult::success(&call.id, "(no matches)"));
                }
                Ok(result)
            }
            Err(e) => Ok(ToolResult::error(&call.id, e.to_string())),
        }
    }
}
