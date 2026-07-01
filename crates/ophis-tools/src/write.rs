use crate::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};
use similar::TextDiff;
use std::path::PathBuf;

pub struct WriteTool;

#[async_trait::async_trait]
impl Tool for WriteTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "write".into(),
            description: "Create a new file or completely overwrite an existing file. For code with hex escapes, backslashes, or complex quotes, use content_b64 parameter to avoid JSON corruption.".into(),
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
                    },
                    "content_b64": {
                        "type": "string",
                        "description": "Base64-encoded content. Use this instead of 'content' when the file contains \\x, 0x, backslashes, or complex quotes to avoid JSON pipeline corruption."
                    }
                },
                "required": ["filePath"]
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

        if ctx.safe_mode {
            let canonical_workspace = ctx
                .working_dir
                .canonicalize()
                .unwrap_or_else(|_| ctx.working_dir.clone());
            if let Ok(canonical_target) = path.canonicalize() {
                if !canonical_target.starts_with(&canonical_workspace) {
                    return Ok(ToolResult::error(
                        &call.id,
                        format!(
                            "Safe mode: blocked write outside workspace: {}",
                            path.display()
                        ),
                    ));
                }
            }
        }

        let has_content_key = call
            .arguments
            .get("content")
            .map(|v| v.is_string())
            .unwrap_or(false)
            || call
                .arguments
                .get("content_b64")
                .map(|v| v.is_string())
                .unwrap_or(false);
        let mut content = call.arguments["content"].as_str().unwrap_or("").to_string();
        if let Some(b64) = call.arguments["content_b64"].as_str() {
            if !b64.is_empty() {
                use base64::Engine;
                match base64::engine::general_purpose::STANDARD.decode(b64) {
                    Ok(decoded) => content = String::from_utf8_lossy(&decoded).to_string(),
                    Err(e) => {
                        return Ok(ToolResult::error(
                            &call.id,
                            format!("Base64 decode failed: {e}"),
                        ))
                    }
                }
            }
        }
        if content.is_empty() && !has_content_key {
            return Ok(ToolResult::error(
                &call.id,
                "No content or content_b64 provided",
            ));
        }

        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let existed = path.exists();
        let diff = if existed {
            std::fs::read_to_string(&path).ok().map(|old| {
                let mut buf = Vec::new();
                TextDiff::from_lines(&old, &content)
                    .unified_diff()
                    .header("old", "new")
                    .to_writer(&mut buf)
                    .ok();
                String::from_utf8_lossy(&buf).to_string()
            })
        } else {
            None
        };
        std::fs::write(&path, &content)?;

        let verify = std::fs::read_to_string(&path).unwrap_or_default();
        if verify != content {
            let diff = diff_strings(&content, &verify);
            let win_start = diff.0.saturating_sub(20);
            let expected_ctx: String = content.chars().skip(win_start).take(40).collect();
            let got_ctx: String = verify.chars().skip(win_start).take(40).collect();
            return Ok(ToolResult::error(
                &call.id,
                format!(
                    "Write verification FAILED. Content on disk differs from what was sent.\n\
                     Sent {} bytes, got {} bytes on disk.\n\
                     First difference at char {}:\n\
                     Expected: ...{}...\n\
                     Got:      ...{}...\n\
                     This may indicate an encoding issue.",
                    content.len(),
                    verify.len(),
                    diff.0,
                    expected_ctx,
                    got_ctx,
                ),
            ));
        }

        if existed {
            let mut r = ToolResult::success(
                &call.id,
                format!("File overwritten (verified): {}", path.display()),
            );
            if let Some(d) = diff {
                r = r.with_diff(d);
            }
            Ok(r)
        } else {
            Ok(ToolResult::success(
                &call.id,
                format!("File created (verified): {}", path.display()),
            ))
        }
    }
}

fn diff_strings(a: &str, b: &str) -> (usize, char, char) {
    for (i, (ca, cb)) in a.chars().zip(b.chars()).enumerate() {
        if ca != cb {
            return (i, ca, cb);
        }
    }
    let len = a.len().min(b.len());
    (len, '\0', '\0')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn ctx() -> ToolContext {
        ToolContext {
            working_dir: std::env::temp_dir(),
            safe_mode: false,
        }
    }

    #[tokio::test]
    async fn test_write_diff_on_overwrite() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        let path = f.path().to_path_buf();
        f.write_all(b"hello world\nfoo bar\n").unwrap();

        let call = ToolCall {
            id: "w1".into(),
            name: "write".into(),
            arguments: serde_json::json!({
                "filePath": path.to_str().unwrap(),
                "content": "hello world\nbaz qux\n",
            }),
        };
        let result = WriteTool.execute(&call, &ctx()).await.unwrap();
        assert!(!result.is_error, "{}", result.content);
        let diff = result.metadata.as_ref().and_then(|m| m.get("diff"));
        assert!(diff.is_some(), "Expected diff metadata on overwrite");
        let diff = diff.unwrap();
        assert!(diff.contains("-foo bar"), "Diff should show removed line");
        assert!(diff.contains("+baz qux"), "Diff should show added line");
    }

    #[tokio::test]
    async fn test_write_new_file_no_diff() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.txt");

        let call = ToolCall {
            id: "w2".into(),
            name: "write".into(),
            arguments: serde_json::json!({
                "filePath": path.to_str().unwrap(),
                "content": "brand new\n",
            }),
        };
        let result = WriteTool.execute(&call, &ctx()).await.unwrap();
        assert!(!result.is_error);
        // New files should NOT have diff
        let diff = result.metadata.as_ref().and_then(|m| m.get("diff"));
        assert!(diff.is_none(), "New files should not have a diff");
    }
}
