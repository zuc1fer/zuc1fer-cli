use crate::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

pub struct BashTool;

const MAX_OUTPUT_BYTES: usize = 100_000;

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
        let workdir = call.arguments["workdir"]
            .as_str()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| ctx.working_dir.clone());

        if ctx.safe_mode {
            let lower = cmd.to_lowercase();
            let dangerous = [
                "rm ", "del ", "rmdir ", "sudo ", "chmod ", "chown ", "dd ", "mkfs", "shutdown",
                "reboot", "halt", "format ", "fdisk", "diskpart",
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
                .kill_on_drop(true)
                .spawn()?
        } else {
            Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .current_dir(&workdir)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()?
        };

        let timeout = tokio::time::Duration::from_millis(timeout_ms);
        let mut stdout_pipe = child.stdout.take();
        let mut stderr_pipe = child.stderr.take();
        let result = tokio::time::timeout(timeout, async {
            let read_out = async {
                let mut buf = Vec::new();
                if let Some(out) = stdout_pipe.as_mut() {
                    out.read_to_end(&mut buf).await?;
                }
                anyhow::Ok(buf)
            };
            let read_err = async {
                let mut buf = Vec::new();
                if let Some(err) = stderr_pipe.as_mut() {
                    err.read_to_end(&mut buf).await?;
                }
                anyhow::Ok(buf)
            };
            let (out_res, err_res) = tokio::join!(read_out, read_err);
            let stdout = String::from_utf8_lossy(&out_res?).to_string();
            let stderr = String::from_utf8_lossy(&err_res?).to_string();

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
                if output.len() > MAX_OUTPUT_BYTES {
                    let mut end = MAX_OUTPUT_BYTES;
                    while end > 0 && !output.is_char_boundary(end) {
                        end -= 1;
                    }
                    let total = output.len();
                    output.truncate(end);
                    output.push_str(&format!(
                        "\n\n[output truncated: {end} of {total} bytes shown]"
                    ));
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
