use crate::{ChatRequest, ContentBlock, LlmProvider, StreamEvent, Usage};
use serde_json::Value;

pub struct OllamaProvider {
    base_url: String,
}

impl OllamaProvider {
    pub fn new() -> Self {
        Self {
            base_url: "http://localhost:11434".into(),
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    fn convert_message(&self, msg: &crate::Message) -> Value {
        let role = match msg.role {
            crate::Role::System => "system",
            crate::Role::User => "user",
            crate::Role::Assistant => "assistant",
            crate::Role::Tool => "user",
        };

        let mut text_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<Value> = Vec::new();

        for block in &msg.content {
            match block {
                ContentBlock::Text { text } => {
                    text_parts.push(text.clone());
                }
                ContentBlock::ToolUse { id, name, input } => {
                    let args_str = serde_json::to_string(input).unwrap_or_default();
                    tool_calls.push(serde_json::json!({
                        "id": id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": args_str,
                        }
                    }));
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    ..
                } => {
                    text_parts.push(format!("[Tool result for {tool_use_id}]: {content}"));
                }
            }
        }

        let content = text_parts.join("\n");
        let mut msg_val = serde_json::json!({
            "role": role,
            "content": content,
        });

        if !tool_calls.is_empty() {
            msg_val["tool_calls"] = Value::Array(tool_calls);
        }

        msg_val
    }
}

#[async_trait::async_trait]
impl LlmProvider for OllamaProvider {
    async fn stream_chat(
        &self,
        request: ChatRequest,
        event_tx: tokio::sync::mpsc::UnboundedSender<StreamEvent>,
    ) -> anyhow::Result<()> {
        let client = reqwest::Client::new();

        let messages: Vec<Value> = std::iter::once(serde_json::json!({
            "role": "system",
            "content": request.system,
        }))
        .chain(request.messages.iter().map(|m| self.convert_message(m)))
        .collect();

        let tools: Vec<Value> = request
            .tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "stream": true,
            "options": {
                "num_predict": request.max_tokens,
            }
        });

        if !tools.is_empty() {
            body["tools"] = Value::Array(tools);
        }
        if let Some(t) = request.temperature {
            body["options"]["temperature"] = t.into();
        }
        if let Some(p) = request.top_p {
            body["options"]["top_p"] = p.into();
        }

        let response = client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            let _ = event_tx.send(StreamEvent::Error {
                message: format!("Ollama API error ({status}): {text}"),
            });
            return Ok(());
        }

        use futures::StreamExt;
        let mut stream = response.bytes_stream();
        let mut text_buf = String::new();
        let mut usage = Usage::default();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            let text = String::from_utf8_lossy(&chunk);

            for line in text.lines() {
                if let Ok(obj) = serde_json::from_str::<Value>(line) {
                    if let Some(content) = obj["message"]["content"].as_str() {
                        text_buf.push_str(content);
                        let _ = event_tx.send(StreamEvent::TextDelta {
                            text: content.to_string(),
                        });
                    }
                    if obj["done"].as_bool().unwrap_or(false) {
                        usage.prompt_tokens =
                            obj.get("prompt_eval_count").and_then(|v| v.as_u64()).unwrap_or(0);
                        usage.completion_tokens =
                            obj.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0);
                        usage.total_tokens = usage.prompt_tokens + usage.completion_tokens;
                    }
                }
            }
        }

        if !text_buf.is_empty() {
            let _ = event_tx.send(StreamEvent::TextDone {
                text: std::mem::take(&mut text_buf),
            });
        }

        let _ = event_tx.send(StreamEvent::Done { usage });
        Ok(())
    }

    fn provider_name(&self) -> &str {
        "ollama"
    }

    fn default_model(&self) -> &str {
        "qwen2.5-coder:7b"
    }

    fn supports_prompt_caching(&self) -> bool {
        false
    }

    fn estimate_tokens(&self, text: &str) -> u64 {
        (text.len() as u64 + 2) / 3
    }
}
