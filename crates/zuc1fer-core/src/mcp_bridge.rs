use std::sync::Arc;
use tokio::sync::Mutex;
use zuc1fer_mcp::{CallToolResult, McpClient, ToolInfo};

pub struct McpBridge {
    client: Arc<Mutex<McpClient>>,
    server_name: String,
    tools: Vec<ToolInfo>,
}

impl McpBridge {
    pub async fn connect(command: &str, args: &[String]) -> anyhow::Result<Self> {
        let mut client = McpClient::connect(command, args).await?;
        client.initialize().await?;
        let tools = client.discover_tools().await?;
        let server_name = client.server_name().to_string();

        tracing::info!(
            "MCP bridge connected: {server_name} ({} tools)",
            tools.len()
        );

        Ok(Self {
            client: Arc::new(Mutex::new(client)),
            server_name,
            tools,
        })
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    pub fn tools_info(&self) -> &[ToolInfo] {
        &self.tools
    }

    pub fn tool_definitions(&self) -> Vec<zuc1fer_tools::ToolDef> {
        self.tools
            .iter()
            .map(|t| zuc1fer_tools::ToolDef {
                name: format!("mcp__{}_{}", self.server_name.replace(['-', ' '], "_"), t.name),
                description: format!(
                    "[MCP: {}] {}",
                    self.server_name, t.description
                ),
                parameters: t.input_schema.clone(),
            })
            .collect()
    }

    pub async fn execute(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> anyhow::Result<CallToolResult> {
        let mut client = self.client.lock().await;
        client.call_tool(tool_name, arguments).await
    }
}
