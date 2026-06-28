use crate::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};
use std::process::Stdio;
use tokio::process::Command;
use tokio::io::AsyncReadExt;

pub struct BashTool;

#[async_trait::async_trait]
impl Tool for BashTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "bash".into(),
            description: "Execute a shell command. Returns stdout and stderr output.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Optional timeout in milliseconds"
                    },
                    "workdir": {
                        "type": "string",
                        "description": "Working directory for the command"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(&self, call: &ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let cmd = call.arguments["command"].as_str().unwrap_or("");
        let timeout_ms = call.arguments["timeout"].as_u64().unwrap_or(120_000);
        let workdir = call
            .arguments["workdir"]
            .as_str()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| ctx.working_dir.clone());

        if ctx.safe_mode {
            let lower = cmd.to_lowercase();
            let dangerous = [
                "rm ", "del ", "rmdir ", "sudo ", "chmod ", "chown ",
                "dd ", "mkfs", "shutdown", "reboot", "halt",
                "format ", "fdisk", "diskpart",
            ];
            for pattern in &dangerous {
                if lower.contains(pattern) {
                    return Ok(ToolResult::error(
                        &call.id,
                        format!("Safe mode: blocked potentially destructive command matching '{pattern}'. Use --safe=false to disable."),
                    ));
                }
            }
        }

        tracing::debug!("bash: {}", cmd);

        let mut child = if cfg!(windows) {
            Command::new("powershell")
                .arg("-NoProfile")
                .arg("-Command")
                .arg(cmd)
                .current_dir(&workdir)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?
        } else {
            Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .current_dir(&workdir)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?
        };

        let timeout = tokio::time::Duration::from_millis(timeout_ms);
        let result = tokio::time::timeout(timeout, async {
            let stdout = {
                let mut buf = Vec::new();
                if let Some(mut out) = child.stdout.take() {
                    out.read_to_end(&mut buf).await?;
                }
                String::from_utf8_lossy(&buf).to_string()
            };

            let stderr = {
                let mut buf = Vec::new();
                if let Some(mut err) = child.stderr.take() {
                    err.read_to_end(&mut buf).await?;
                }
                String::from_utf8_lossy(&buf).to_string()
            };

            let status = child.wait().await?;
            anyhow::Ok((stdout, stderr, status.code()))
        })
        .await;

        match result {
            Ok(Ok((stdout, stderr, code))) => {
                let mut output = String::new();
                if !stdout.is_empty() {
                    output.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str("[stderr]\n");
                    output.push_str(&stderr);
                }
                if output.is_empty() {
                    output = format!("Command completed with exit code: {}", code.unwrap_or(-1));
                }
                Ok(ToolResult::success(&call.id, output))
            }
            Ok(Err(e)) => Ok(ToolResult::error(&call.id, format!("Failed: {e}"))),
            Err(_) => Ok(ToolResult::error(
                &call.id,
                format!("Command timed out after {timeout_ms}ms"),
            )),
        }
    }
}
