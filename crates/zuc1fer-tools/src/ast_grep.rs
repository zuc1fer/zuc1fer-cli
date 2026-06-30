use crate::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

pub struct AstGrepTool;

impl AstGrepTool {
    async fn run_sg(
        &self,
        cwd: &PathBuf,
        pattern: &str,
        lang: Option<&str>,
    ) -> anyhow::Result<Vec<String>> {
        let mut cmd = Command::new("sg");
        cmd.arg("run")
            .arg("--pattern")
            .arg(pattern)
            .arg("--json")
            .arg("stream");

        if let Some(l) = lang {
            cmd.arg("--lang").arg(l);
        }

        cmd.current_dir(cwd);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await?;

        if !output.stderr.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if output.status.code() == Some(2) || output.status.code().is_none() {
                anyhow::bail!("ast-grep error: {}", stderr.trim());
            }
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let mut results = Vec::new();

        for line in text.lines() {
            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                if entry["type"].as_str() == Some("Match") {
                    let path = entry["file"].as_str().unwrap_or("unknown");
                    let line_num = entry["range"]["start"]["line"].as_u64().unwrap_or(0);
                    let text = entry["text"].as_str().unwrap_or("").trim_end();
                    results.push(format!("{path}:{line_num}: {text}"));
                }
            }
        }

        Ok(results)
    }
}

#[async_trait::async_trait]
impl Tool for AstGrepTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "ast_grep".into(),
            description: "Structural code search using AST pattern matching. Search by code shape, not text regex. Works across 20+ languages. Pattern syntax: write code with meta-variables like `$$$` (ellipsis), `$VAR` (capture). Example: `$$$fetch($$$)` finds all fetch calls. Example: `useState($A)` captures all useState arguments. Requires `sg` (ast-grep) installed: `npm i -g @ast-grep/cli`".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "AST pattern to match. Write code with meta-variables. $VAR captures a node, $$$ matches zero or more nodes."
                    },
                    "lang": {
                        "type": "string",
                        "description": "Language to restrict search to (e.g., 'rust', 'typescript', 'python'). Optional, auto-detects if omitted."
                    }
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn execute(&self, call: &ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let pattern = call.arguments["pattern"].as_str().unwrap_or("");
        let lang = call.arguments["lang"].as_str();

        match self.run_sg(&ctx.working_dir, pattern, lang).await {
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
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("program not found") || msg.contains("cannot find") {
                    Ok(ToolResult::error(
                        &call.id,
                        "ast-grep (sg) not installed. Install it: npm i -g @ast-grep/cli",
                    ))
                } else {
                    Ok(ToolResult::error(&call.id, msg))
                }
            }
        }
    }
}
