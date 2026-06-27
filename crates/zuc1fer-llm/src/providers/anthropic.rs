use crate::{
    ChatRequest, ContentBlock, LlmProvider, StreamEvent, Usage,
};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;

pub struct AnthropicProvider {
    api_key: SecretString,
    base_url: String,
    anthropic_version: String,
}

impl AnthropicProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: SecretString::from(api_key.into()),
            base_url: "https://api.anthropic.com".into(),
            anthropic_version: "2023-06-01".into(),
        }
    }

    fn convert_request(&self, request: &ChatRequest) -> Value {
        let mut messages: Vec<Value> = Vec::new();
        for msg in &request.messages {
            if matches!(msg.role, crate::Role::System) {
                continue;
            }
            let content = self.convert_content(&msg.content);
            let role = match msg.role {
                crate::Role::User | crate::Role::Tool => "user",
                crate::Role::Assistant => "assistant",
                crate::Role::System => unreachable!(),
            };
            messages.push(serde_json::json!({ "role": role, "content": content }));
        }

        let tools: Vec<Value> = request
            .tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })
            })
            .collect();

        let system_block = if request.cache_system {
            vec![
                serde_json::json!({
                    "type": "text",
                    "text": request.system,
                    "cache_control": { "type": "ephemeral" }
                }),
            ]
        } else {
            vec![serde_json::json!({
                "type": "text",
                "text": request.system,
            })]
        };

        let mut body = serde_json::json!({
            "model": request.model,
            "system": system_block,
            "messages": messages,
            "max_tokens": request.max_tokens,
            "stream": true,
        });

        if !tools.is_empty() {
            body["tools"] = Value::Array(tools);
        }
        if let Some(t) = request.temperature {
            body["temperature"] = t.into();
        }
        if let Some(p) = request.top_p {
            body["top_p"] = p.into();
        }

        body
    }

    fn convert_content(&self, blocks: &[ContentBlock]) -> Value {
        if blocks.len() == 1 {
            if let ContentBlock::Text { text } = &blocks[0] {
                return Value::String(text.clone());
            }
        }
        Value::Array(
            blocks
                .iter()
                .map(|b| match b {
                    ContentBlock::Text { text } => serde_json::json!({
                        "type": "text",
                        "text": text,
                    }),
                    ContentBlock::ToolUse { id, name, input } => serde_json::json!({
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": input,
                    }),
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": tool_use_id,
                        "content": content,
                        "is_error": is_error.unwrap_or(false),
                    }),
                })
                .collect(),
        )
    }
}

#[async_trait::async_trait]
impl LlmProvider for AnthropicProvider {
    async fn stream_chat(
        &self,
        request: ChatRequest,
        event_tx: tokio::sync::mpsc::UnboundedSender<StreamEvent>,
    ) -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        let body = self.convert_request(&request);

        let response = client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", self.api_key.expose_secret())
            .header("anthropic-version", &self.anthropic_version)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            let _ = event_tx.send(StreamEvent::Error {
                message: format!("Anthropic API error ({status}): {text}"),
            });
            return Ok(());
        }

        use futures::StreamExt;
        let mut stream = response.bytes_stream();
        let mut text_buf = String::new();
        let mut current_tool_id = String::new();
        let mut current_tool_name = String::new();
        let mut current_tool_input = String::new();
        let mut usage = Usage::default();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            let text = String::from_utf8_lossy(&chunk);

            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let Some(payload) = line.strip_prefix("data: ") else {
                    continue;
                };

                if let Ok(obj) = serde_json::from_str::<Value>(payload) {
                    let event_type = obj["type"].as_str().unwrap_or("");

                    match event_type {
                        "content_block_start" => {
                            if obj["content_block"]["type"] == "tool_use" {
                                current_tool_id = obj["content_block"]["id"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string();
                                current_tool_name = obj["content_block"]["name"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string();
                                current_tool_input.clear();
                                let _ = event_tx.send(StreamEvent::ToolUseStart {
                                    id: current_tool_id.clone(),
                                    name: current_tool_name.clone(),
                                });
                            }
                        }
                        "content_block_delta" => {
                            if let Some(t) = obj["delta"]["text"].as_str() {
                                text_buf.push_str(t);
                                let _ = event_tx.send(StreamEvent::TextDelta {
                                    text: t.to_string(),
                                });
                            }
                            if let Some(input) = obj["delta"]["input_json"].as_str() {
                                current_tool_input.push_str(input);
                                let _ = event_tx.send(StreamEvent::ToolUseDelta {
                                    id: current_tool_id.clone(),
                                    input_json: input.to_string(),
                                });
                            }
                        }
                        "content_block_stop" => {
                            if !current_tool_id.is_empty() && !current_tool_name.is_empty() {
                                let input: Value =
                                    serde_json::from_str(&current_tool_input).unwrap_or(Value::Null);
                                let _ = event_tx.send(StreamEvent::ToolUseDone {
                                    id: std::mem::take(&mut current_tool_id),
                                    name: std::mem::take(&mut current_tool_name),
                                    input,
                                });
                            }
                        }
                        "message_delta" => {
                            if let Some(u) = obj["usage"].as_object() {
                                usage.completion_tokens = u
                                    .get("output_tokens")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                            }
                        }
                        "message_start" => {
                            if let Some(u) = obj["message"]["usage"].as_object() {
                                usage.prompt_tokens = u
                                    .get("input_tokens")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                                usage.cache_read_tokens = u
                                    .get("cache_read_input_tokens")
                                    .and_then(|v| v.as_u64());
                                usage.cache_write_tokens = u
                                    .get("cache_creation_input_tokens")
                                    .and_then(|v| v.as_u64());
                            }
                        }
                        "error" => {
                            let _ = event_tx.send(StreamEvent::Error {
                                message: obj["error"]["message"]
                                    .as_str()
                                    .unwrap_or("Unknown Anthropic error")
                                    .to_string(),
                            });
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
        }

        if !text_buf.is_empty() {
            let _ = event_tx.send(StreamEvent::TextDone {
                text: std::mem::take(&mut text_buf),
            });
        }

        usage.total_tokens = usage.prompt_tokens + usage.completion_tokens;
        let _ = event_tx.send(StreamEvent::Done { usage });
        Ok(())
    }

    fn provider_name(&self) -> &str {
        "anthropic"
    }

    fn default_model(&self) -> &str {
        "claude-sonnet-4-20250514"
    }

    fn supports_prompt_caching(&self) -> bool {
        true
    }

    fn estimate_tokens(&self, text: &str) -> u64 {
        (text.len() as u64 * 3) / 4
    }
}
