use crate::{ast_grep::AstGrepTool, bash::BashTool, edit::EditTool, git::GitTool, glob::GlobTool, grep::GrepTool, read::ReadTool, write::WriteTool};
use crate::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};
use std::collections::HashMap;
use std::sync::Arc;

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };
        registry.register_builtins();
        registry
    }

    fn register_builtins(&mut self) {
        self.register(Arc::new(BashTool));
        self.register(Arc::new(ReadTool));
        self.register(Arc::new(WriteTool));
        self.register(Arc::new(EditTool));
        self.register(Arc::new(GlobTool));
        self.register(Arc::new(GrepTool));
        self.register(Arc::new(AstGrepTool));
        self.register(Arc::new(GitTool));
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let def = tool.definition();
        let name = def.name.clone();
        tracing::debug!("Registered tool: {name}");
        self.tools.insert(name, tool);
    }

    pub fn definitions(&self) -> Vec<ToolDef> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub async fn execute(
        &self,
        call: &ToolCall,
        ctx: &ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let name = &call.name;

        if let Some(tool) = self.tools.get(name) {
            tool.execute(call, ctx).await
        } else {
            Ok(ToolResult::error(
                &call.id,
                format!("Unknown tool: {name}. Available: {}", self.tool_names()),
            ))
        }
    }

    pub async fn execute_parallel(
        &self,
        calls: &[ToolCall],
        ctx: &ToolContext,
    ) -> Vec<ToolResult> {
        let futures: Vec<_> = calls
            .iter()
            .map(|call| {
                let tool = self.get(&call.name);
                let call = call.clone();
                let ctx = ctx.clone();
                async move {
                    if let Some(tool) = tool {
                        match tool.execute(&call, &ctx).await {
                            Ok(r) => r,
                            Err(e) => ToolResult::error(&call.id, e.to_string()),
                        }
                    } else {
                        ToolResult::error(
                            &call.id,
                            format!("Unknown tool: {}", call.name),
                        )
                    }
                }
            })
            .collect();

        futures::future::join_all(futures).await
    }

    fn tool_names(&self) -> String {
        let mut names: Vec<_> = self.tools.keys().cloned().collect();
        names.sort();
        names.join(", ")
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
