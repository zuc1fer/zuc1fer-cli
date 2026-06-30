use crate::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};

pub struct WebFetch;

#[async_trait::async_trait]
impl Tool for WebFetch {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "webfetch".into(),
            description: "Fetches content from a specified URL and returns it as plain text. \
                Use this to read documentation, API references, or any web content."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to fetch content from"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["text", "markdown"],
                        "description": "Return format: 'text' strips HTML to plain text, 'markdown' converts to markdown. Default: text"
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Optional timeout in seconds (max 120, default 30)"
                    }
                },
                "required": ["url"]
            }),
        }
    }

    async fn execute(&self, call: &ToolCall, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let url = call.arguments["url"].as_str().unwrap_or("");
        if url.is_empty() {
            return Ok(ToolResult::error(&call.id, "URL is required"));
        }

        let url = if !url.starts_with("http://") && !url.starts_with("https://") {
            format!("https://{url}")
        } else {
            url.to_string()
        };

        let timeout_secs = call.arguments["timeout"].as_u64().unwrap_or(30).min(120);

        let client = reqwest::Client::builder()
            .user_agent("zuc1fer/1.0 (coding-agent)")
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()?;

        let response = client.get(&url).send().await;

        match response {
            Ok(resp) => {
                let status = resp.status();
                let content_type = resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();

                let body = resp.text().await.unwrap_or_default();

                if !status.is_success() {
                    return Ok(ToolResult::error(
                        &call.id,
                        format!("HTTP {status}: {:.500}", body),
                    ));
                }

                let output = if content_type.contains("text/html") {
                    let rendered = html2text::from_read(body.as_bytes(), 100);
                    truncate_chars(&rendered, 20_000)
                } else if content_type.contains("image/")
                    || content_type.contains("video/")
                    || content_type.contains("audio/")
                    || content_type.contains("application/pdf")
                    || content_type.contains("application/octet-stream")
                {
                    format!("[Binary content: {}]", content_type)
                } else {
                    truncate_chars(&body, 20_000)
                };

                Ok(ToolResult::success(&call.id, output))
            }
            Err(e) => Ok(ToolResult::error(
                &call.id,
                format!("Failed to fetch {url}: {e}"),
            )),
        }
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    let total = s.chars().count();
    if total <= max {
        return s.to_string();
    }
    let shown: String = s.chars().take(max).collect();
    format!("{shown}\n\n[truncated: {total} total chars]")
}
