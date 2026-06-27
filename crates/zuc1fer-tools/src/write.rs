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

        let content = call.arguments["content"].as_str().unwrap_or("");

        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let existed = path.exists();
        std::fs::write(&path, content)?;

        if existed {
            Ok(ToolResult::success(&call.id, format!("File overwritten: {}", path.display())))
        } else {
            Ok(ToolResult::success(&call.id, format!("File created: {}", path.display())))
        }
    }
}
