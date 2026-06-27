use crate::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};
use std::path::PathBuf;

pub struct GitTool;

impl GitTool {
    fn open_repo(&self, cwd: &PathBuf) -> anyhow::Result<git2::Repository> {
        Ok(git2::Repository::discover(cwd)?)
    }

    fn diff(&self, repo: &git2::Repository, staged: bool) -> anyhow::Result<String> {
        let mut opts = git2::DiffOptions::new();
        let diff = if staged {
            let head = repo.head().ok();
            let tree = head.as_ref().and_then(|h| h.peel_to_tree().ok());
            repo.diff_tree_to_index(tree.as_ref(), None, Some(&mut opts))?
        } else {
            repo.diff_index_to_workdir(None, Some(&mut opts))?
        };

        let mut output = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            if let Ok(content) = std::str::from_utf8(line.content()) {
                let prefix = match line.origin() {
                    '+' => "+",
                    '-' => "-",
                    ' ' => " ",
                    'F' => "F",
                    'H' => "H",
                    _ => "",
                };
                output.push_str(&format!("{prefix}{content}"));
            }
            true
        })?;
        Ok(output)
    }

    fn log(&self, repo: &git2::Repository, count: usize) -> anyhow::Result<String> {
        let mut revwalk = repo.revwalk()?;
        revwalk.push_head()?;
        revwalk.set_sorting(git2::Sort::TIME)?;

        let mut output = String::new();
        for (_i, oid) in revwalk.enumerate().take(count) {
            let oid = oid?;
            let commit = repo.find_commit(oid)?;
            let short_id = &oid.to_string()[..7.min(oid.to_string().len())];
            let message = commit.message().unwrap_or("").lines().next().unwrap_or("");
            let author = commit.author();
            let time = commit.time();
            let ts = chrono::DateTime::from_timestamp(time.seconds(), 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default();

            output.push_str(&format!(
                "{} {} {} {}\n",
                short_id, ts, author.name().unwrap_or("unknown"), message
            ));
        }
        Ok(output.trim().to_string())
    }

    fn status(&self, repo: &git2::Repository) -> anyhow::Result<String> {
        let mut output = String::new();
        for entry in repo.statuses(None)?.iter() {
            let status = entry.status();
            let mut flags = String::new();

            if status.contains(git2::Status::INDEX_NEW) {
                flags.push_str("A ");
            } else if status.contains(git2::Status::INDEX_MODIFIED) {
                flags.push_str("M ");
            } else if status.contains(git2::Status::INDEX_DELETED) {
                flags.push_str("D ");
            } else if status.contains(git2::Status::WT_NEW) {
                flags.push_str("??");
            } else if status.contains(git2::Status::WT_MODIFIED) {
                flags.push_str("M ");
            } else if status.contains(git2::Status::WT_DELETED) {
                flags.push_str("D ");
            }

            if let Some(path) = entry.path() {
                output.push_str(&format!("{flags} {path}\n"));
            }
        }
        Ok(if output.is_empty() {
            "clean".into()
        } else {
            output.trim().to_string()
        })
    }

    fn show(&self, repo: &git2::Repository, target: &str) -> anyhow::Result<String> {
        let obj = repo.revparse_single(target)?;
        let commit = obj.peel_to_commit()?;
        let message = commit.message().unwrap_or("");
        let author = commit.author();
        let time = commit.time();
        let ts = chrono::DateTime::from_timestamp(time.seconds(), 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_default();

        let tree = commit.tree()?;
        let parent_tree = commit.parents().next().and_then(|p| p.tree().ok());

        let mut opts = git2::DiffOptions::new();
        let diff = repo.diff_tree_to_tree(
            parent_tree.as_ref(),
            Some(&tree),
            Some(&mut opts),
        )?;

        let mut patch = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            if let Ok(content) = std::str::from_utf8(line.content()) {
                patch.push_str(content);
            }
            true
        })?;

        Ok(format!(
            "commit {}\nAuthor: {} <{}>\nDate:   {}\n\n    {}\n\n{}",
            obj.id(),
            author.name().unwrap_or("unknown"),
            author.email().unwrap_or("unknown"),
            ts,
            message.replace('\n', "\n    "),
            patch,
        ))
    }
}

#[async_trait::async_trait]
impl Tool for GitTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "git".into(),
            description: "Native git operations. Commands: diff (unstaged), diff --staged, log [count], status, show <ref>. Fast native implementation via libgit2 — no shelling out.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Git subcommand: diff, log, status, show"
                    },
                    "staged": {
                        "type": "boolean",
                        "description": "For diff: show staged changes instead of unstaged"
                    },
                    "count": {
                        "type": "integer",
                        "description": "For log: number of commits to show (default 10)"
                    },
                    "target": {
                        "type": "string",
                        "description": "For show: commit hash, tag, or branch to show"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(&self, call: &ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let command = call.arguments["command"].as_str().unwrap_or("status");

        match self.open_repo(&ctx.working_dir) {
            Ok(repo) => {
                let result = match command {
                    "diff" => {
                        let staged = call.arguments["staged"].as_bool().unwrap_or(false);
                        self.diff(&repo, staged).map(|d| {
                            if d.is_empty() {
                                "(no changes)".into()
                            } else {
                                d
                            }
                        })
                    }
                    "log" => {
                        let count = call.arguments["count"].as_u64().unwrap_or(10) as usize;
                        self.log(&repo, count)
                    }
                    "status" => self.status(&repo),
                    "show" => {
                        let target = call.arguments["target"]
                            .as_str()
                            .unwrap_or("HEAD");
                        self.show(&repo, target)
                    }
                    _ => anyhow::bail!("Unknown git command: {command}. Use: diff, log, status, show"),
                };

                match result {
                    Ok(output) => Ok(ToolResult::success(&call.id, output)),
                    Err(e) => Ok(ToolResult::error(&call.id, e.to_string())),
                }
            }
            Err(_) => Ok(ToolResult::error(
                &call.id,
                "Not a git repository (or no git repository found in parent directories)",
            )),
        }
    }
}
