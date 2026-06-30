use crate::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};
use globset::Glob;
use grep::regex::RegexMatcher;
use grep::searcher::sinks::UTF8;
use grep::searcher::{BinaryDetection, SearcherBuilder};
use ignore::WalkBuilder;
use std::path::PathBuf;

pub struct GrepTool;

fn search_files(
    cwd: PathBuf,
    pattern: &str,
    include: Option<String>,
    limit: usize,
) -> anyhow::Result<(Vec<String>, usize)> {
    let matcher = RegexMatcher::new(pattern)
        .map_err(|e| anyhow::anyhow!("Invalid regex pattern: {e}"))?;

    let include_matcher = match include.as_deref() {
        Some(inc) if !inc.is_empty() => Some(
            Glob::new(inc)
                .map_err(|e| anyhow::anyhow!("Invalid include glob: {e}"))?
                .compile_matcher(),
        ),
        _ => None,
    };

    let mut searcher = SearcherBuilder::new()
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .build();

    let mut results: Vec<String> = Vec::new();
    let mut total = 0usize;

    for entry in WalkBuilder::new(&cwd).build().flatten() {
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let path = entry.path();
        if let Some(im) = &include_matcher {
            let rel = path.strip_prefix(&cwd).unwrap_or(path);
            if !im.is_match(rel) {
                continue;
            }
        }
        let path_str = path.display().to_string();
        let _ = searcher.search_path(
            &matcher,
            path,
            UTF8(|lnum, line| {
                total += 1;
                if results.len() < limit {
                    results.push(format!("{}:{}: {}", path_str, lnum, line.trim_end()));
                }
                Ok(true)
            }),
        );
    }

    Ok((results, total))
}

#[async_trait::async_trait]
impl Tool for GrepTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "grep".into(),
            description: "Search file contents using regex patterns. Returns file paths with line numbers and matching lines. Honors .gitignore, skips binary files. Results limited to 100 matches.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to search in (defaults to working directory)"
                    },
                    "include": {
                        "type": "string",
                        "description": "Glob to filter files (e.g., '*.rs', '**/*.ts')"
                    }
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn execute(&self, call: &ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let pattern = call.arguments["pattern"].as_str().unwrap_or("").to_string();
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

        let include = call.arguments["include"].as_str().map(|s| s.to_string());
        let limit = 100;

        let res =
            tokio::task::spawn_blocking(move || search_files(search_dir, &pattern, include, limit))
                .await;
        let (matches, total) = match res {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => return Ok(ToolResult::error(&call.id, e.to_string())),
            Err(e) => return Ok(ToolResult::error(&call.id, format!("grep task failed: {e}"))),
        };

        if matches.is_empty() {
            return Ok(ToolResult::success(&call.id, "(no matches)"));
        }

        let mut result = ToolResult::success(&call.id, matches.join("\n"));
        if total > limit {
            result
                .metadata
                .get_or_insert_with(std::collections::HashMap::new)
                .insert("truncated".into(), format!("true ({total} total, showing {limit})"));
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
    async fn grep_finds_matches_with_line_numbers() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "alpha\nbeta\nalpha again\n").unwrap();

        let call = ToolCall {
            id: "r1".into(),
            name: "grep".into(),
            arguments: serde_json::json!({ "pattern": "alpha" }),
        };
        let result = GrepTool.execute(&call, &ctx(dir.path())).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains(":1:"));
        assert!(result.content.contains(":3:"));
        assert!(!result.content.contains(":2:"));
    }

    #[tokio::test]
    async fn grep_include_filter_restricts_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "needle\n").unwrap();
        std::fs::write(dir.path().join("b.txt"), "needle\n").unwrap();

        let call = ToolCall {
            id: "r2".into(),
            name: "grep".into(),
            arguments: serde_json::json!({ "pattern": "needle", "include": "*.rs" }),
        };
        let result = GrepTool.execute(&call, &ctx(dir.path())).await.unwrap();
        assert!(result.content.contains("a.rs"));
        assert!(!result.content.contains("b.txt"));
    }

    #[tokio::test]
    async fn grep_invalid_regex_errors() {
        let dir = tempfile::tempdir().unwrap();
        let call = ToolCall {
            id: "r3".into(),
            name: "grep".into(),
            arguments: serde_json::json!({ "pattern": "(unclosed" }),
        };
        let result = GrepTool.execute(&call, &ctx(dir.path())).await.unwrap();
        assert!(result.is_error);
    }
}
