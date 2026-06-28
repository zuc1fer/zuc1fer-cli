use crate::code_index::CodeIndex;
use std::sync::Arc;
use zuc1fer_tools::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};

pub struct SemanticTool {
    index: Arc<CodeIndex>,
}

impl SemanticTool {
    pub fn new(index: Arc<CodeIndex>) -> Self {
        Self { index }
    }
}

#[async_trait::async_trait]
impl Tool for SemanticTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "semantic".into(),
            description: "Search the codebase using BM25 full-text ranking. \
                Returns ranked file paths with relevance scores and content snippets. \
                Use this to find code by keywords, function names, or concepts.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query (keywords, function names, concepts)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum results (default 10, max 30)"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, call: &ToolCall, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let query = call.arguments["query"].as_str().unwrap_or("");
        if query.is_empty() {
            return Ok(ToolResult::error(&call.id, "Query is required"));
        }

        let limit = call.arguments["limit"].as_u64().unwrap_or(10).min(30) as usize;
        let file_count = self.index.file_count();

        if file_count == 0 {
            return Ok(ToolResult::error(
                &call.id,
                "Code index is empty. The indexer may not have run yet.",
            ));
        }

        let results = self.index.search(query, limit)?;

        if results.is_empty() {
            return Ok(ToolResult::success(&call.id, "No matching files found."));
        }

        let mut output = format!(
            "Found {} results (searched {} files):\n\n",
            results.len(),
            file_count
        );

        for (i, r) in results.iter().enumerate() {
            output.push_str(&format!(
                "{}. {} (score: {:.3})\n   {}\n\n",
                i + 1,
                r.path,
                r.score,
                r.snippet,
            ));
        }

        Ok(ToolResult::success(&call.id, output))
    }
}
