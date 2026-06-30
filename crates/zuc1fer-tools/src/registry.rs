use crate::{
    ast_grep::AstGrepTool, bash::BashTool, edit::EditTool, git::GitTool, glob::GlobTool,
    grep::GrepTool, read::ReadTool, webfetch::WebFetch, websearch::WebSearch, write::WriteTool,
};
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
        self.register(Arc::new(WebFetch));
        self.register(Arc::new(WebSearch));
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

    pub async fn execute(&self, call: &ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
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

    pub async fn execute_parallel(&self, calls: &[ToolCall], ctx: &ToolContext) -> Vec<ToolResult> {
        const SERIAL: &[&str] = &["write", "edit", "bash"];

        let mut results: Vec<Option<ToolResult>> = vec![None; calls.len()];

        let concurrent: Vec<(usize, _)> = calls
            .iter()
            .enumerate()
            .filter(|(_, call)| !SERIAL.contains(&call.name.as_str()))
            .map(|(i, call)| {
                let tool = self.get(&call.name);
                let call = call.clone();
                let ctx = ctx.clone();
                (i, async move { Self::run_one(tool, call, ctx).await })
            })
            .collect();

        let (indices, futures): (Vec<usize>, Vec<_>) = concurrent.into_iter().unzip();
        let done = futures::future::join_all(futures).await;
        for (i, result) in indices.into_iter().zip(done) {
            results[i] = Some(result);
        }

        for (i, call) in calls.iter().enumerate() {
            if SERIAL.contains(&call.name.as_str()) {
                let tool = self.get(&call.name);
                results[i] = Some(Self::run_one(tool, call.clone(), ctx.clone()).await);
            }
        }

        results.into_iter().map(|r| r.unwrap()).collect()
    }

    async fn run_one(tool: Option<Arc<dyn Tool>>, call: ToolCall, ctx: ToolContext) -> ToolResult {
        match tool {
            Some(tool) => match tool.execute(&call, &ctx).await {
                Ok(r) => r,
                Err(e) => ToolResult::error(&call.id, e.to_string()),
            },
            None => ToolResult::error(&call.id, format!("Unknown tool: {}", call.name)),
        }
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
