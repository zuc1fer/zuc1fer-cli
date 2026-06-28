use crate::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};
use std::path::PathBuf;

pub struct WriteTool;

#[async_trait::async_trait]
impl Tool for WriteTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "write".into(),
            description: "Create a new file or completely overwrite an existing file. Use with caution — this overwrites without warning.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "filePath": {
                        "type": "string",
                        "description": "Absolute path to the file to write"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file"
                    }
                },
                "required": ["filePath", "content"]
            }),
        }
    }

    async fn execute(&self, call: &ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let path_str = call.arguments["filePath"].as_str().unwrap_or("");
        let path = PathBuf::from(path_str);

        let path = if path.is_relative() {
            ctx.working_dir.join(path)
        } else {
            path
        };

        if ctx.safe_mode {
            let canonical_workspace = ctx.working_dir.canonicalize().unwrap_or_else(|_| ctx.working_dir.clone());
            if let Ok(canonical_target) = path.canonicalize() {
                if !canonical_target.starts_with(&canonical_workspace) {
                    return Ok(ToolResult::error(
                        &call.id,
                        format!("Safe mode: blocked write outside workspace: {}", path.display()),
                    ));
                }
            }
        }

        let content = call.arguments["content"].as_str().unwrap_or("");

        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let existed = path.exists();
        std::fs::write(&path, content)?;

        let verify = std::fs::read_to_string(&path).unwrap_or_default();
        if verify != content {
            let diff = diff_strings(content, &verify);
            return Ok(ToolResult::error(
                &call.id,
                format!(
                    "Write verification FAILED. Content on disk differs from what was sent.\n\
                     Sent {} chars, got {} chars on disk.\n\
                     First difference at char {}:\n\
                     Expected: ...{}...\n\
                     Got:      ...{}...\n\
                     This may indicate path corruption or encoding issues.",
                    content.len(),
                    verify.len(),
                    diff.0,
                    &content[diff.0.saturating_sub(20)..(diff.0 + 20).min(content.len())],
                    &verify[diff.0.saturating_sub(20)..(diff.0 + 20).min(verify.len())],
                ),
            ));
        }

        if existed {
            Ok(ToolResult::success(&call.id, format!("File overwritten (verified): {}", path.display())))
        } else {
            Ok(ToolResult::success(&call.id, format!("File created (verified): {}", path.display())))
        }
    }
}

fn diff_strings(a: &str, b: &str) -> (usize, char, char) {
    for (i, (ca, cb)) in a.chars().zip(b.chars()).enumerate() {
        if ca != cb {
            return (i, ca, cb);
        }
    }
    let len = a.len().min(b.len());
    (len, '\0', '\0')
}
