use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

pub struct GrepEngine;

impl GrepEngine {
    pub async fn search(
        cwd: &PathBuf,
        pattern: &str,
        include: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<GrepMatch>> {
        let mut cmd = Command::new("rg");
        cmd.args(["--json", "--no-config", "--no-heading", "--color", "never", "--no-messages"]);

        if let Some(inc) = include {
            if !inc.is_empty() {
                cmd.arg("--glob").arg(inc);
            }
        }

        cmd.arg("--").arg(pattern);
        cmd.current_dir(cwd);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await?;

        if !output.stderr.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if output.status.code() == Some(2) {
                anyhow::bail!("Invalid regex pattern: {}", stderr.trim());
            }
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let mut results = Vec::new();

        for line in text.lines().take(limit) {
            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                let ty = entry["type"].as_str().unwrap_or("");
                if ty == "match" {
                    let path = entry["data"]["path"]["text"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_string();
                    let line_num = entry["data"]["line_number"].as_u64().unwrap_or(0) as usize;
                    let text = entry["data"]["lines"]["text"]
                        .as_str()
                        .unwrap_or("")
                        .trim_end()
                        .to_string();
                    results.push(GrepMatch {
                        path,
                        line: line_num,
                        text,
                    });
                }
            }
        }

        Ok(results)
    }
}

#[derive(Debug, Clone)]
pub struct GrepMatch {
    pub path: String,
    pub line: usize,
    pub text: String,
}

impl std::fmt::Display for GrepMatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.path, self.line, self.text)
    }
}
