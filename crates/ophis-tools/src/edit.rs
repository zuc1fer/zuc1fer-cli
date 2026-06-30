use crate::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};
use std::path::PathBuf;

pub struct EditTool;

#[async_trait::async_trait]
impl Tool for EditTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "edit".into(),
            description: "Perform exact string replacements in a file. Use for surgical changes to existing files (prefer over write for small edits). Provide enough surrounding context in oldString to make it unique. Use replaceAll for renaming across the whole file. Preserve exact indentation when matching.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "filePath": {
                        "type": "string",
                        "description": "Absolute path to the file to modify"
                    },
                    "oldString": {
                        "type": "string",
                        "description": "The text to replace"
                    },
                    "newString": {
                        "type": "string",
                        "description": "The text to replace it with (must be different from oldString)"
                    },
                    "replaceAll": {
                        "type": "boolean",
                        "description": "Replace all occurrences of oldString (default false)"
                    }
                },
                "required": ["filePath", "oldString", "newString"]
            }),
        }
    }

    async fn execute(&self, call: &ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let path_str = call.arguments["filePath"].as_str().unwrap_or("");
        let path = PathBuf::from(path_str);

        let mut path = if path.is_relative() {
            ctx.working_dir.join(path)
        } else {
            path
        };

        if !path.exists() {
            if let Some(alt) = crate::try_fuzzy_path(path_str) {
                if alt.exists() {
                    path = alt;
                }
            }
        }

        let old = call.arguments["oldString"].as_str().unwrap_or("");
        let new = call.arguments["newString"].as_str().unwrap_or("");
        let replace_all = call.arguments["replaceAll"].as_bool().unwrap_or(false);

        if old == new {
            return Ok(ToolResult::error(
                &call.id,
                "oldString and newString are identical",
            ));
        }

        let content = std::fs::read_to_string(&path)?;
        let occurrences = content.match_indices(old).count();

        if occurrences == 0 {
            return Ok(ToolResult::error(
                &call.id,
                "oldString not found in content",
            ));
        }

        if occurrences > 1 && !replace_all {
            return Ok(ToolResult::error(
                &call.id,
                format!("Found {occurrences} matches for oldString. Provide more surrounding lines in oldString to identify the correct match, or use replaceAll to change every instance."),
            ));
        }

        let new_content = if replace_all {
            content.replace(old, new)
        } else {
            content.replacen(old, new, 1)
        };

        std::fs::write(&path, &new_content)?;

        Ok(ToolResult::success(
            &call.id,
            if replace_all {
                format!("Replaced {occurrences} occurrences in {}", path.display())
            } else {
                format!("Replaced 1 occurrence in {}", path.display())
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_file(content: &str) -> (tempfile::NamedTempFile, PathBuf) {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let path = f.path().to_path_buf();
        (f, path)
    }

    fn ctx() -> ToolContext {
        ToolContext {
            working_dir: std::env::temp_dir(),
            safe_mode: false,
        }
    }

    #[tokio::test]
    async fn test_edit_single_occurrence() {
        let (_f, path) = temp_file("hello world\nfoo bar\n");
        let call = ToolCall {
            id: "t1".into(),
            name: "edit".into(),
            arguments: serde_json::json!({
                "filePath": path.to_str().unwrap(),
                "oldString": "foo bar",
                "newString": "baz qux",
            }),
        };
        let result = EditTool.execute(&call, &ctx()).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("Replaced 1 occurrence"));
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "hello world\nbaz qux\n");
    }

    #[tokio::test]
    async fn test_edit_not_found() {
        let (_f, path) = temp_file("hello world\n");
        let call = ToolCall {
            id: "t2".into(),
            name: "edit".into(),
            arguments: serde_json::json!({
                "filePath": path.to_str().unwrap(),
                "oldString": "not here",
                "newString": "nope",
            }),
        };
        let result = EditTool.execute(&call, &ctx()).await.unwrap();
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_edit_multiple_without_replace_all() {
        let (_f, path) = temp_file("foo\nfoo\nfoo\n");
        let call = ToolCall {
            id: "t3".into(),
            name: "edit".into(),
            arguments: serde_json::json!({
                "filePath": path.to_str().unwrap(),
                "oldString": "foo",
                "newString": "bar",
            }),
        };
        let result = EditTool.execute(&call, &ctx()).await.unwrap();
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_edit_replace_all() {
        let (_f, path) = temp_file("foo\nfoo\nfoo\n");
        let call = ToolCall {
            id: "t4".into(),
            name: "edit".into(),
            arguments: serde_json::json!({
                "filePath": path.to_str().unwrap(),
                "oldString": "foo",
                "newString": "bar",
                "replaceAll": true,
            }),
        };
        let result = EditTool.execute(&call, &ctx()).await.unwrap();
        assert!(!result.is_error);
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "bar\nbar\nbar\n");
    }
}
