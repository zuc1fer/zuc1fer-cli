pub mod ast_grep;
pub mod bash;
pub mod edit;
pub mod git;
pub mod glob;
pub mod grep;
pub mod read;
pub mod registry;
pub mod webfetch;
pub mod websearch;
pub mod write;

pub use registry::ToolRegistry;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
    pub is_error: bool,
    pub metadata: Option<HashMap<String, String>>,
}

impl ToolResult {
    pub fn success(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_call_id: id.into(),
            content: content.into(),
            is_error: false,
            metadata: None,
        }
    }

    pub fn error(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_call_id: id.into(),
            content: content.into(),
            is_error: true,
            metadata: None,
        }
    }

    pub fn with_diff(mut self, diff: String) -> Self {
        let mut m = self.metadata.unwrap_or_default();
        m.insert("diff".into(), diff);
        self.metadata = Some(m);
        self
    }

    pub fn truncated(&self) -> bool {
        self.metadata
            .as_ref()
            .and_then(|m| m.get("truncated"))
            .map(|v| v == "true")
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub working_dir: std::path::PathBuf,
    pub safe_mode: bool,
}

#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDef;
    async fn execute(&self, call: &ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult>;
}

pub fn try_fuzzy_path(path_str: &str) -> Option<std::path::PathBuf> {
    let p = std::path::PathBuf::from(path_str);
    if p.is_absolute() {
        let swapped = std::path::PathBuf::from(path_str.replace('\\', "/"));
        if swapped != p && swapped.exists() {
            return Some(swapped);
        }
        let swapped_back = std::path::PathBuf::from(path_str.replace('/', "\\"));
        if swapped_back != p && swapped_back.exists() {
            return Some(swapped_back);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_result_success() {
        let result = ToolResult::success("call_1", "hello world");
        assert_eq!(result.tool_call_id, "call_1");
        assert_eq!(result.content, "hello world");
        assert!(!result.is_error);
        assert!(!result.truncated());
    }

    #[test]
    fn test_tool_result_error() {
        let result = ToolResult::error("call_2", "something went wrong");
        assert_eq!(result.tool_call_id, "call_2");
        assert!(result.is_error);
    }

    #[test]
    fn test_tool_result_with_diff() {
        let r = ToolResult::success("t1", "file modified");
        let r = r.with_diff("--- old\n+++ new\n@@ -1 +1 @@\n-foo\n+bar\n".into());
        assert!(!r.is_error);
        let m = r.metadata.unwrap();
        assert_eq!(m.get("diff").unwrap(), "--- old\n+++ new\n@@ -1 +1 @@\n-foo\n+bar\n");
    }

    #[test]
    fn test_tool_result_truncated() {
        let mut result = ToolResult::success("call_3", "data");
        result.metadata = Some({
            let mut m = HashMap::new();
            m.insert("truncated".into(), "true".into());
            m
        });
        assert!(result.truncated());
    }

    #[test]
    fn test_tool_def_serialization() {
        let def = ToolDef {
            name: "test".into(),
            description: "a test tool".into(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        };
        let json = serde_json::to_string(&def).unwrap();
        let parsed: ToolDef = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "test");
    }
}
