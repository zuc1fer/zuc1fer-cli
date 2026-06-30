use crate::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};
use std::path::PathBuf;

pub struct ReadTool;

#[async_trait::async_trait]
impl Tool for ReadTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "read".into(),
            description: "Read a file from the filesystem. Use for reading specific files you already know about by path. Prefer glob for finding files by pattern and grep for searching file contents. Supports offset (1-indexed line) and limit to read portions of large files.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "filePath": {
                        "type": "string",
                        "description": "Absolute path to the file to read"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Line number to start reading from (1-indexed, default 1)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of lines to read (default 2000)"
                    }
                },
                "required": ["filePath"]
            }),
        }
    }

    async fn execute(&self, call: &ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let path_str = call.arguments["filePath"].as_str().unwrap_or("");
        let path = PathBuf::from(path_str);

        let path = if path.is_relative() {
            ctx.working_dir.join(path)
        } else {
            path
        };

        if !path.exists() {
            let alt = crate::try_fuzzy_path(path_str);
            if let Some(fixed) = alt {
                if fixed.exists() {
                    return self.read_file(&call.id, &fixed, call);
                }
            }
            return Ok(ToolResult::error(
                &call.id,
                format!(
                    "File not found: {}. Try with forward slashes in the path.",
                    path.display()
                ),
            ));
        }

        if path.is_dir() {
            let entries: Vec<String> = std::fs::read_dir(&path)?
                .filter_map(|e| e.ok())
                .map(|e| {
                    let ft = if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        "/"
                    } else {
                        ""
                    };
                    format!("{}{}", e.file_name().to_string_lossy(), ft)
                })
                .collect();
            return Ok(ToolResult::success(&call.id, entries.join("\n")));
        }

        self.read_file(&call.id, &path, call)
    }
}

impl ReadTool {
    fn read_file(&self, id: &str, path: &PathBuf, call: &ToolCall) -> anyhow::Result<ToolResult> {
        let content = std::fs::read_to_string(path)?;
        let lines: Vec<&str> = content.lines().collect();

        let offset = call.arguments["offset"].as_u64().unwrap_or(1).max(1) as usize;
        let limit = call.arguments["limit"].as_u64().unwrap_or(2000).max(1) as usize;

        let start = (offset - 1).min(lines.len());
        let end = (start + limit).min(lines.len());

        let output: String = lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{}: {}", start + i + 1, line))
            .collect::<Vec<_>>()
            .join("\n");

        let mut result = ToolResult::success(id, output);

        if end < lines.len() {
            result
                .metadata
                .get_or_insert_with(std::collections::HashMap::new)
                .insert(
                    "truncated".into(),
                    format!("true ({} of {} lines)", end, lines.len()),
                );
        }

        Ok(result)
    }
}
