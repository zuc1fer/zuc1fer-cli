use crate::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};
use globset::Glob;
use ignore::WalkBuilder;
use std::path::PathBuf;

pub struct GlobTool;

fn find_files(
    search_dir: PathBuf,
    pattern: &str,
    limit: usize,
) -> anyhow::Result<(Vec<String>, usize)> {
    let matcher = Glob::new(pattern)
        .map_err(|e| anyhow::anyhow!("Invalid glob pattern: {e}"))?
        .compile_matcher();

    let mut results: Vec<String> = Vec::new();
    let mut total = 0usize;

    for entry in WalkBuilder::new(&search_dir).build().flatten() {
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let path = entry.path();
        let rel = path.strip_prefix(&search_dir).unwrap_or(path);
        if matcher.is_match(rel) {
            total += 1;
            if results.len() < limit {
                results.push(path.display().to_string());
            }
        }
    }

    Ok((results, total))
}

#[async_trait::async_trait]
impl Tool for GlobTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "glob".into(),
            description: "Find files matching a glob pattern. Supports patterns like '**/*.rs', 'src/**/*.ts'. Honors .gitignore. Results limited to 100 files.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern to match files against (e.g., '**/*.rs', 'src/**/*.tsx')"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to search in (defaults to working directory)"
                    }
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn execute(&self, call: &ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let pattern = call.arguments["pattern"]
            .as_str()
            .unwrap_or("*")
            .to_string();
        let search_dir_path = call.arguments["path"].as_str();
        let mut search_dir = search_dir_path
            .map(PathBuf::from)
            .unwrap_or_else(|| ctx.working_dir.clone());

        if !search_dir.exists() {
            if let Some(path_str) = search_dir_path {
                if let Some(alt) = crate::try_fuzzy_path(path_str) {
                    if alt.exists() {
                        search_dir = alt;
                    }
                }
            }
        }

        let limit = 100;
        let res =
            tokio::task::spawn_blocking(move || find_files(search_dir, &pattern, limit)).await;
        let (files, total) = match res {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => return Ok(ToolResult::error(&call.id, e.to_string())),
            Err(e) => {
                return Ok(ToolResult::error(
                    &call.id,
                    format!("glob task failed: {e}"),
                ))
            }
        };

        if files.is_empty() {
            return Ok(ToolResult::success(&call.id, "(no files)"));
        }

        let mut result = ToolResult::success(&call.id, files.join("\n"));
        if total > limit {
            result
                .metadata
                .get_or_insert_with(std::collections::HashMap::new)
                .insert(
                    "truncated".into(),
                    format!("true ({total} total, showing {limit})"),
                );
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(dir: &std::path::Path) -> ToolContext {
        ToolContext {
            working_dir: dir.to_path_buf(),
            safe_mode: false,
        }
    }

    #[tokio::test]
    async fn glob_matches_by_extension_recursively() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn a() {}").unwrap();
        std::fs::write(dir.path().join("b.txt"), "text").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("c.rs"), "fn c() {}").unwrap();

        let call = ToolCall {
            id: "g1".into(),
            name: "glob".into(),
            arguments: serde_json::json!({ "pattern": "**/*.rs" }),
        };
        let result = GlobTool.execute(&call, &ctx(dir.path())).await.unwrap();
        assert!(!result.is_error);
        assert!(
            result.content.contains("a.rs"),
            "should match top-level a.rs"
        );
        assert!(result.content.contains("c.rs"), "should match nested c.rs");
        assert!(!result.content.contains("b.txt"));
    }
}
