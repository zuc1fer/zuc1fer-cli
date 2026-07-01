use crate::lsp_client::LspClient;
use std::sync::Arc;
use ophis_tools::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};

pub struct LspTool {
    client: Arc<LspClient>,
}

impl LspTool {
    pub fn new(client: Arc<LspClient>) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl Tool for LspTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "lsp".into(),
            description: "Interact with language servers for code intelligence. \
                Actions: 'definition' (go to definition), 'references' (find all references), \
                'hover' (get symbol documentation), 'diagnostics' (get file errors/warnings). \
                Requires LSP servers installed (rust-analyzer, pyright, typescript-language-server, gopls)."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["definition", "references", "hover", "diagnostics"],
                        "description": "LSP action to perform"
                    },
                    "filePath": {
                        "type": "string",
                        "description": "Absolute path to the source file"
                    },
                    "line": {
                        "type": "integer",
                        "description": "Line number (1-based, as shown in editors). Required for definition/references/hover."
                    },
                    "character": {
                        "type": "integer",
                        "description": "Character offset (0-based). Required for definition/references/hover."
                    }
                },
                "required": ["action", "filePath"]
            }),
        }
    }

    async fn execute(&self, call: &ToolCall, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let action = call.arguments["action"].as_str().unwrap_or("");
        let file_path = call.arguments["filePath"].as_str().unwrap_or("");

        if file_path.is_empty() {
            return Ok(ToolResult::error(&call.id, "filePath is required"));
        }

        match action {
            "definition" => {
                let line = line_from_args(&call.arguments, "line");
                let character = char_from_args(&call.arguments, "character");
                match self.client.definition(file_path, line, character).await {
                    Ok(results) => {
                        let output = results.join("\n");
                        Ok(ToolResult::success(&call.id, output))
                    }
                    Err(e) => Ok(ToolResult::error(&call.id, e.to_string())),
                }
            }
            "references" => {
                let line = line_from_args(&call.arguments, "line");
                let character = char_from_args(&call.arguments, "character");
                match self.client.references(file_path, line, character).await {
                    Ok(results) => {
                        let output = results.join("\n");
                        Ok(ToolResult::success(&call.id, output))
                    }
                    Err(e) => Ok(ToolResult::error(&call.id, e.to_string())),
                }
            }
            "hover" => {
                let line = line_from_args(&call.arguments, "line");
                let character = char_from_args(&call.arguments, "character");
                match self.client.hover(file_path, line, character).await {
                    Ok(text) => Ok(ToolResult::success(&call.id, text)),
                    Err(e) => Ok(ToolResult::error(&call.id, e.to_string())),
                }
            }
            "diagnostics" => match self.client.diagnostics(file_path).await {
                Ok(results) => {
                    if results.is_empty() {
                        Ok(ToolResult::success(&call.id, "No diagnostics found."))
                    } else {
                        let output = results.join("\n");
                        Ok(ToolResult::success(&call.id, output))
                    }
                }
                Err(e) => Ok(ToolResult::error(&call.id, e.to_string())),
            },
            _ => Ok(ToolResult::error(
                &call.id,
                format!(
                    "Unknown action: {action}. Use: definition, references, hover, diagnostics"
                ),
            )),
        }
    }
}

fn line_from_args(args: &serde_json::Value, key: &str) -> u32 {
    args[key].as_u64().unwrap_or(1).saturating_sub(1) as u32
}

fn char_from_args(args: &serde_json::Value, key: &str) -> u32 {
    args[key].as_u64().unwrap_or(0) as u32
}
