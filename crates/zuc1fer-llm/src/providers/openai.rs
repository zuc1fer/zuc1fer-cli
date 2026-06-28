use crate::{ChatRequest, ContentBlock, LlmProvider, StreamEvent, Usage};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;

pub struct OpenAIProvider {
    api_key: SecretString,
    base_url: String,
}

impl OpenAIProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: SecretString::from(api_key.into()),
            base_url: "https://api.openai.com".into(),
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    fn convert_request(&self, request: &ChatRequest) -> Value {
        let mut system_msg = serde_json::json!({
            "role": "system",
            "content": request.system,
        });
        if request.cache_system {
            system_msg["cache_control"] = serde_json::json!({"type": "ephemeral"});
        }

        let messages: Vec<Value> = std::iter::once(system_msg)
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
            "max_completion_tokens": request.max_tokens,
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

    fn convert_message(&self, msg: &crate::Message) -> Value {
        let role = role_to_str(&msg.role);

        let mut text_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<Value> = Vec::new();
        let mut tool_call_id: Option<String> = None;
        let mut tool_content = String::new();

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
                    tool_call_id = Some(tool_use_id.clone());
                    tool_content = content.clone();
                }
            }
        }

        if role == "tool" {
            serde_json::json!({
                "role": "tool",
                "tool_call_id": tool_call_id.unwrap_or_default(),
                "content": tool_content,
            })
        } else if !tool_calls.is_empty() {
            serde_json::json!({
                "role": role,
                "content": text_parts.join("\n"),
                "tool_calls": tool_calls,
            })
        } else {
            serde_json::json!({
                "role": role,
                "content": text_parts.join("\n"),
            })
        }
    }
}

fn role_to_str(role: &crate::Role) -> &str {
    match role {
        crate::Role::System => "system",
        crate::Role::User => "user",
        crate::Role::Assistant => "assistant",
        crate::Role::Tool => "tool",
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenAIProvider {
    async fn stream_chat(
        &self,
        request: ChatRequest,
        event_tx: tokio::sync::mpsc::UnboundedSender<StreamEvent>,
    ) -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        let body = self.convert_request(&request);

        let response = client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key.expose_secret()))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            let _ = event_tx.send(StreamEvent::Error {
                message: format!("OpenAI API error ({status}): {text}"),
            });
            return Ok(());
        }

        use futures::StreamExt;
        let mut stream = response.bytes_stream();
        let mut text_buf = String::new();
        let mut current_tool_id: Option<String> = None;
        let mut current_tool_name = String::new();
        let mut current_tool_args = String::new();
        let mut usage = Usage::default();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            let text = String::from_utf8_lossy(&chunk);

            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line == "data: [DONE]" {
                    continue;
                }
                let Some(payload) = line.strip_prefix("data: ") else {
                    continue;
                };

                if let Ok(obj) = serde_json::from_str::<Value>(payload) {
                    if let Some(choices) = obj["choices"].as_array() {
                        for choice in choices {
                            if let Some(delta) = choice.get("delta") {
                                if let Some(tool_calls) = delta["tool_calls"].as_array() {
                                    for tc in tool_calls {
                                        if let Some(id) = tc["id"].as_str() {
                                            if current_tool_id.as_deref() != Some(id) {
                                                if let Some(prev_id) = current_tool_id.take() {
                                                    let input = serde_json::from_str(&current_tool_args)
                                                        .unwrap_or(Value::Null);
                                                    let _ = event_tx
                                                        .send(StreamEvent::ToolUseDone {
                                                            id: prev_id,
                                                            name: current_tool_name.clone(),
                                                            input,
                                                        });
                                                }
                                                current_tool_id = Some(id.to_string());
                                                current_tool_args.clear();
                                                let name = tc["function"]["name"]
                                                    .as_str()
                                                    .unwrap_or("")
                                                    .to_string();
                                                current_tool_name = name.clone();
                                                let _ = event_tx.send(
                                                    StreamEvent::ToolUseStart {
                                                        id: id.to_string(),
                                                        name,
                                                    },
                                                );
                                            }
                                        }
                                        if let Some(args) = tc["function"]["arguments"].as_str() {
                                            current_tool_args.push_str(args);
                                            let _ = event_tx.send(StreamEvent::ToolUseDelta {
                                                id: current_tool_id.clone().unwrap_or_default(),
                                                input_json: args.to_string(),
                                            });
                                        }
                                    }
                                }
                                if let Some(content) = delta["content"].as_str() {
                                    if !content.is_empty() {
                                        text_buf.push_str(content);
                                        let _ = event_tx.send(StreamEvent::TextDelta {
                                            text: content.to_string(),
                                        });
                                    }
                                }
                            }
                            if let Some(reason) = choice["finish_reason"].as_str() {
                                if reason == "tool_calls" && current_tool_id.is_some() {
                                    let id = current_tool_id.take().unwrap();
                                    let input = serde_json::from_str(&current_tool_args)
                                        .unwrap_or(Value::Null);
                                    let _ = event_tx.send(StreamEvent::ToolUseDone {
                                        id,
                                        name: current_tool_name.clone(),
                                        input,
                                    });
                                }
                            }
                        }
                    }
                    if let Some(u) = obj.get("usage") {
                        usage.prompt_tokens = u["prompt_tokens"].as_u64().unwrap_or(0);
                        usage.completion_tokens = u["completion_tokens"].as_u64().unwrap_or(0);
                        usage.total_tokens = u["total_tokens"].as_u64().unwrap_or(0);
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
        "openai"
    }

    fn default_model(&self) -> &str {
        "gpt-4o"
    }

    fn supports_prompt_caching(&self) -> bool {
        true
    }

    fn estimate_tokens(&self, text: &str) -> u64 {
        (text.len() as u64 + 2) / 3
    }
}
