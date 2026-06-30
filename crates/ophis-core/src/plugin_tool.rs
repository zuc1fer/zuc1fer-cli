use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use ophis_tools::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};

pub struct PluginTool {
    name: String,
    description: String,
    command: String,
    args: Vec<String>,
    timeout_ms: u64,
}

impl PluginTool {
    pub fn from_config(
        name: String,
        description: String,
        command: String,
        args: Vec<String>,
        timeout_ms: u64,
    ) -> Self {
        Self {
            name,
            description,
            command,
            args,
            timeout_ms,
        }
    }

    fn build_params_schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": true
        })
    }
}

#[async_trait::async_trait]
impl Tool for PluginTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: self.name.clone(),
            description: self.description.clone(),
            parameters: Self::build_params_schema(),
        }
    }

    async fn execute(&self, call: &ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let workdir_str = ctx.working_dir.display().to_string();
        let input = serde_json::json!({
            "arguments": call.arguments,
            "working_dir": workdir_str,
        });

        let mut child = tokio::process::Command::new(&self.command)
            .args(&self.args)
            .current_dir(&ctx.working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            let input_str = serde_json::to_string(&input)? + "\n";
            stdin.write_all(input_str.as_bytes()).await?;
            drop(stdin);
        }

        let timeout = tokio::time::Duration::from_millis(self.timeout_ms);
        let result = tokio::time::timeout(timeout, async {
            let output = child.wait_with_output().await?;
            anyhow::Ok(output)
        })
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&stdout) {
                    let content = parsed["content"].as_str().unwrap_or(&stdout).to_string();
                    let is_error = parsed["is_error"]
                        .as_bool()
                        .unwrap_or(!output.status.success());
                    if is_error {
                        Ok(ToolResult::error(&call.id, content))
                    } else {
                        Ok(ToolResult::success(&call.id, content))
                    }
                } else if output.status.success() {
                    let mut content = stdout;
                    if !stderr.is_empty() {
                        content.push_str("\n[stderr]\n");
                        content.push_str(&stderr);
                    }
                    Ok(ToolResult::success(&call.id, content))
                } else {
                    Ok(ToolResult::error(
                        &call.id,
                        format!("Plugin failed (exit {}): {}", output.status, stderr),
                    ))
                }
            }
            Ok(Err(e)) => Ok(ToolResult::error(&call.id, format!("Plugin error: {e}"))),
            Err(_) => Ok(ToolResult::error(
                &call.id,
                format!("Plugin timed out after {}ms", self.timeout_ms),
            )),
        }
    }
}
