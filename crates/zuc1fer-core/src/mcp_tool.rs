use std::sync::Arc;
use zuc1fer_mcp::ToolInfo;
use zuc1fer_tools::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};

use crate::mcp_bridge::McpBridge;

pub struct McpTool {
    bridge: Arc<McpBridge>,
    tool_info: ToolInfo,
    tool_name: String,
}

impl McpTool {
    pub fn new(bridge: Arc<McpBridge>, tool_info: ToolInfo, server_name: &str) -> Self {
        let tool_name = format!("mcp__{}_{}", server_name.replace(['-', ' '], "_"), tool_info.name);
        Self {
            bridge,
            tool_info,
            tool_name,
        }
    }
}

#[async_trait::async_trait]
impl Tool for McpTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: self.tool_name.clone(),
            description: format!("[MCP] {}", self.tool_info.description),
            parameters: self.tool_info.input_schema.clone(),
        }
    }

    async fn execute(&self, call: &ToolCall, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        match self.bridge.execute(&self.tool_info.name, call.arguments.clone()).await {
            Ok(result) => {
                let content: String = result
                    .content
                    .iter()
                    .map(|c| c.text.clone())
                    .collect::<Vec<_>>()
                    .join("\n");

                if result.is_error {
                    Ok(ToolResult::error(&call.id, content))
                } else {
                    Ok(ToolResult::success(&call.id, content))
                }
            }
            Err(e) => Ok(ToolResult::error(&call.id, e.to_string())),
        }
    }
}
