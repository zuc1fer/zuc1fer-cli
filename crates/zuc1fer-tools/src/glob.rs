use crate::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

pub struct GlobTool;

impl GlobTool {
    async fn run_rg_glob(&self, cwd: &PathBuf, pattern: &str) -> anyhow::Result<Vec<String>> {
        let mut cmd = Command::new("rg");
        cmd.args(["--files", "--glob", pattern, "--no-config"])
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().await?;

        if !output.stderr.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if output.status.code() == Some(2) {
                anyhow::bail!("Invalid glob pattern: {stderr}");
            }
        }

        let text = String::from_utf8_lossy(&output.stdout);
        Ok(text.lines().map(|s| s.to_string()).collect())
    }
}

#[async_trait::async_trait]
impl Tool for GlobTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "glob".into(),
            description: "Find files matching a glob pattern using ripgrep. Supports patterns like '**/*.rs', 'src/**/*.ts'. Results limited to 100 files.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern to match files against (e.g., '**/*.rs', 'src/**/*.tsx')"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to search in (defaults to working directory)"
                    }
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn execute(&self, call: &ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let pattern = call.arguments["pattern"].as_str().unwrap_or("*");
        let search_dir = call
            .arguments["path"]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or_else(|| ctx.working_dir.clone());

        match self.run_rg_glob(&search_dir, pattern).await {
            Ok(files) => {
                let limit = 100;
                let total = files.len();
                let output = files
                    .iter()
                    .take(limit)
                    .map(|f| {
                        let full = search_dir.join(f);
                        full.display().to_string()
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                let mut result = ToolResult::success(&call.id, output);
                if total > limit {
                    result
                        .metadata
                        .get_or_insert_with(std::collections::HashMap::new)
                        .insert("truncated".into(), format!("true ({total} total, showing {limit})"));
                }
                Ok(result)
            }
            Err(e) => Ok(ToolResult::error(&call.id, e.to_string())),
        }
    }
}
