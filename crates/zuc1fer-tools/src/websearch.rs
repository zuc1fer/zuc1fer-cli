use crate::{Tool, ToolCall, ToolContext, ToolDef, ToolResult};

pub struct WebSearch;

#[async_trait::async_trait]
impl Tool for WebSearch {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "websearch".into(),
            description: "Search the web using DuckDuckGo. Returns titles, URLs, and snippets for the top results. \
                Use this to find documentation, solutions, or current information.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (default 10, max 20)"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, call: &ToolCall, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let query = call.arguments["query"]
            .as_str()
            .unwrap_or("");
        if query.is_empty() {
            return Ok(ToolResult::error(&call.id, "Query is required"));
        }

        let limit = call.arguments["limit"]
            .as_u64()
            .unwrap_or(10)
            .min(20) as usize;

        let client = reqwest::Client::builder()
            .user_agent("zuc1fer/1.0 (coding-agent; search)")
            .timeout(std::time::Duration::from_secs(15))
            .build()?;

        let search_url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding(query)
        );

        let response = client.get(&search_url).send().await;

        match response {
            Ok(resp) => {
                let body = resp.text().await.unwrap_or_default();
                let results = parse_ddg_results(&body, limit);

                if results.is_empty() {
                    return Ok(ToolResult::success(
                        &call.id,
                        "No results found. The search engine may be blocking automated queries.",
                    ));
                }

                let mut output = String::new();
                for (i, (title, url, snippet)) in results.iter().enumerate() {
                    output.push_str(&format!(
                        "{}. {}\n   URL: {}\n   {}\n\n",
                        i + 1,
                        title,
                        url,
                        snippet
                    ));
                }

                Ok(ToolResult::success(&call.id, output))
            }
            Err(e) => Ok(ToolResult::error(
                &call.id,
                format!("Search failed: {e}"),
            )),
        }
    }
}

fn urlencoding(s: &str) -> String {
    let mut result = String::new();
    for byte in s.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            b' ' => result.push('+'),
            _ => result.push_str(&format!("%{:02X}", byte)),
        }
    }
    result
}

fn parse_ddg_results(html: &str, limit: usize) -> Vec<(String, String, String)> {
    let mut results = Vec::new();

    let mut in_result = false;
    let mut current_title = String::new();
    let mut current_url = String::new();
    let mut current_snippet = String::new();

    for line in html.lines() {
        let trimmed = line.trim();

        if trimmed.contains("class=\"result__title\"") || trimmed.contains("class=\"result__a\"") {
            in_result = true;
            current_title.clear();
            current_url.clear();
            current_snippet.clear();
        }

        if in_result {
            if let Some(title) = extract_tag_content_with_class(trimmed, "a", "class=\"result__a\"") {
                current_title = title;
            }
            if current_title.is_empty() {
                if let Some(title) = extract_tag_content(trimmed, "a") {
                    current_title = title;
                }
            }

            if let Some(url) = extract_href(trimmed) {
                if current_url.is_empty() && !url.contains("duckduckgo.com") {
                    current_url = clean_ddg_url(&url);
                }
            }

            if let Some(snip) = extract_tag_content_with_class(trimmed, "a", "class=\"result__snippet\"") {
                current_snippet = snip;
            }
            if current_snippet.is_empty() {
                if let Some(snip) = extract_class_content(trimmed, "result__snippet") {
                    current_snippet = strip_html(&snip);
                }
            }

            if !current_title.is_empty() && !current_snippet.is_empty() {
                results.push((current_title.clone(), current_url.clone(), current_snippet.clone()));
                in_result = false;

                if results.len() >= limit {
                    break;
                }
            }
        }
    }

    results
}

fn extract_tag_content(line: &str, tag: &str) -> Option<String> {
    extract_tag_content_with_class(line, tag, "")
}

fn extract_tag_content_with_class(line: &str, tag: &str, class: &str) -> Option<String> {
    if !class.is_empty() && !line.contains(class) {
        return None;
    }
    let open = format!("<{}", tag);
    if let Some(start) = line.find(&open) {
        let after_open = &line[start..];
        if let Some(content_start) = after_open.find('>') {
            let content = &after_open[content_start + 1..];
            if let Some(end) = content.find(&format!("</{}>", tag)) {
                return Some(html_entity_decode(content[..end].trim()));
            }
        }
    }
    None
}

fn extract_href(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    if let Some(href_pos) = lower.find("href=\"") {
        let start = href_pos + 6;
        let rest = &line[start..];
        if let Some(end) = rest.find('"') {
            return Some(rest[..end].to_string());
        }
    }
    if let Some(href_pos) = lower.find("href='") {
        let start = href_pos + 6;
        let rest = &line[start..];
        if let Some(end) = rest.find('\'') {
            return Some(rest[..end].to_string());
        }
    }
    None
}

fn extract_class_content(line: &str, class: &str) -> Option<String> {
    if !line.contains(class) {
        return None;
    }
    if let Some(start) = line.find('>') {
        let content = &line[start + 1..];
        if let Some(end) = content.find('<') {
            return Some(content[..end].trim().to_string());
        }
    }
    None
}

fn clean_ddg_url(url: &str) -> String {
    if url.starts_with("//") {
        format!("https:{url}")
    } else if let Some(uddg) = url.find("uddg=") {
        let rest = &url[uddg + 5..];
        if let Some(end) = rest.find('&') {
            urlencoding_decode(&rest[..end])
        } else {
            urlencoding_decode(rest)
        }
    } else {
        url.to_string()
    }
}

fn urlencoding_decode(s: &str) -> String {
    let mut result = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                result.push(hex as char);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            result.push(' ');
        } else {
            result.push(bytes[i] as char);
        }
        i += 1;
    }
    result
}

fn html_entity_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&nbsp;", " ")
}

fn strip_html(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(c);
        }
    }
    html_entity_decode(&result).trim().to_string()
}
