use crate::protocol::JsonRpcMessage;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

pub struct StdioTransport {
    process: Child,
    response_rx: mpsc::UnboundedReceiver<JsonRpcMessage>,
    stdin_writer: tokio::process::ChildStdin,
}

impl StdioTransport {
    pub async fn connect(command: &str, args: &[String]) -> anyhow::Result<Self> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stdin"))?;

        let (response_tx, response_rx) = mpsc::unbounded_channel::<JsonRpcMessage>();

        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 {
                    break;
                }
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    if let Ok(msg) = serde_json::from_str::<JsonRpcMessage>(trimmed) {
                        if response_tx.send(msg).is_err() {
                            break;
                        }
                    }
                }
                line.clear();
            }
        });

        Ok(Self {
            process: child,
            response_rx,
            stdin_writer: stdin,
        })
    }

    pub async fn send(&mut self, message: &JsonRpcMessage) -> anyhow::Result<()> {
        let mut json = serde_json::to_string(message)?;
        json.push('\n');
        self.stdin_writer.write_all(json.as_bytes()).await?;
        self.stdin_writer.flush().await?;
        Ok(())
    }

    pub async fn recv(&mut self) -> Option<JsonRpcMessage> {
        self.response_rx.recv().await
    }

    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        self.process.kill().await?;
        Ok(())
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        let _ = self.process.start_kill();
    }
}
