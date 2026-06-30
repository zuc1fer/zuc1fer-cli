use crate::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};

pub struct WebSearch;

#[async_trait::async_trait]
impl Tool for WebSearch {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "websearch".into(),
            description: "Search the web via the Tavily API. Returns titles, URLs, and content snippets for the top results. Requires the TAVILY_API_KEY environment variable.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (default 5, max 20)"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, call: &ToolCall, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let query = call.arguments["query"].as_str().unwrap_or("");
        if query.is_empty() {
            return Ok(ToolResult::error(&call.id, "Query is required"));
        }

        let api_key = std::env::var("TAVILY_API_KEY").unwrap_or_default();
        if api_key.is_empty() {
            return Ok(ToolResult::error(
                &call.id,
                "Web search is not configured. Set the TAVILY_API_KEY environment variable (get a key at https://tavily.com).",
            ));
        }

        let limit = call.arguments["limit"]
            .as_u64()
            .unwrap_or(5)
            .clamp(1, 20);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()?;

        let body = serde_json::json!({
            "api_key": api_key,
            "query": query,
            "max_results": limit,
            "search_depth": "basic",
        });

        let resp = match client
            .post("https://api.tavily.com/search")
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return Ok(ToolResult::error(&call.id, format!("Search request failed: {e}"))),
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Ok(ToolResult::error(
                &call.id,
                format!("Tavily API error ({status}): {:.500}", text),
            ));
        }

        let data: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                return Ok(ToolResult::error(
                    &call.id,
                    format!("Failed to parse Tavily response: {e}"),
                ))
            }
        };

        let results = data["results"].as_array().cloned().unwrap_or_default();
        if results.is_empty() {
            return Ok(ToolResult::success(&call.id, "No results found."));
        }

        let mut output = String::new();
        for (i, r) in results.iter().enumerate() {
            let title = r["title"].as_str().unwrap_or("(no title)");
            let url = r["url"].as_str().unwrap_or("");
            let content = r["content"].as_str().unwrap_or("");
            output.push_str(&format!("{}. {}\n   URL: {}\n   {}\n\n", i + 1, title, url, content));
        }

        Ok(ToolResult::success(&call.id, output))
    }
}
