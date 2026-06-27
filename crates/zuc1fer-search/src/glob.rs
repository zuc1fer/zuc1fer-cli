use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

pub struct GlobEngine;

impl GlobEngine {
    pub async fn find(
        cwd: &PathBuf,
        pattern: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<PathBuf>> {
        let mut cmd = Command::new("rg");
        cmd.args(["--files", "--glob", pattern, "--no-config"])
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().await?;

        if !output.stderr.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if output.status.code() == Some(2) {
                anyhow::bail!("Invalid glob pattern: {}", stderr.trim());
            }
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let results: Vec<PathBuf> = text
            .lines()
            .take(limit)
            .map(|l| cwd.join(l))
            .collect();

        Ok(results)
    }
}
