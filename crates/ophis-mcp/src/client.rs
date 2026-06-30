use crate::protocol::*;
use crate::transport::StdioTransport;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct McpClient {
    transport: StdioTransport,
    server_info: ServerInfo,
    tools: Vec<ToolInfo>,
    next_id: Arc<AtomicU64>,
}

impl McpClient {
    pub async fn connect(command: &str, args: &[String]) -> anyhow::Result<Self> {
        let transport = StdioTransport::connect(command, args).await?;

        Ok(Self {
            transport,
            server_info: ServerInfo {
                name: String::new(),
                version: String::new(),
            },
            tools: Vec::new(),
            next_id: Arc::new(AtomicU64::new(1)),
        })
    }

    async fn send_request(&mut self, req: &JsonRpcRequest) -> anyhow::Result<()> {
        let msg = JsonRpcMessage::Request(req.clone());
        self.transport.send(&msg).await?;
        Ok(())
    }

    async fn read_response(&mut self, expected_id: u64) -> anyhow::Result<JsonRpcResponse> {
        while let Some(msg) = self.transport.recv().await {
            if let JsonRpcMessage::Response(resp) = msg {
                if resp.id == expected_id {
                    return Ok(resp);
                }
            }
        }
        anyhow::bail!("Transport closed while waiting for response")
    }

    async fn request(&mut self, method: &str, params: Value) -> anyhow::Result<JsonRpcResponse> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = JsonRpcRequest::new(id, method, params);
        self.send_request(&req).await?;
        self.read_response(id).await
    }

    async fn notify(&mut self, method: &str, params: Value) -> anyhow::Result<()> {
        let msg = JsonRpcMessage::Notification(JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: method.into(),
            params,
        });
        self.transport.send(&msg).await?;
        Ok(())
    }

    pub async fn initialize(&mut self) -> anyhow::Result<()> {
        let resp = self
            .request(
                "initialize",
                serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {
                        "name": "ophis",
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                }),
            )
            .await?;

        let init: InitializeResult = serde_json::from_value(resp.result)?;
        self.server_info = init.server_info;
        tracing::info!(
            "MCP connected: {} v{} (protocol {})",
            self.server_info.name,
            self.server_info.version,
            init.protocol_version,
        );

        self.notify("notifications/initialized", serde_json::json!({}))
            .await?;

        Ok(())
    }

    pub async fn discover_tools(&mut self) -> anyhow::Result<Vec<ToolInfo>> {
        let resp = self.request("tools/list", Value::Null).await?;
        let list: ListToolsResult = serde_json::from_value(resp.result)?;
        self.tools = list.tools.clone();
        tracing::info!("MCP: discovered {} tools", self.tools.len());
        Ok(list.tools)
    }

    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: Value,
    ) -> anyhow::Result<CallToolResult> {
        let resp = self
            .request(
                "tools/call",
                serde_json::json!({
                    "name": name,
                    "arguments": arguments,
                }),
            )
            .await?;

        let result: CallToolResult = serde_json::from_value(resp.result)?;
        Ok(result)
    }

    pub fn tools(&self) -> &[ToolInfo] {
        &self.tools
    }

    pub fn server_name(&self) -> &str {
        &self.server_info.name
    }

    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        self.transport.shutdown().await?;
        Ok(())
    }
}
