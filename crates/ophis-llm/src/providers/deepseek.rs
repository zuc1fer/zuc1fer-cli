use crate::{ChatRequest, ContentBlock, LlmProvider, StreamEvent, ToolDefinition, Usage};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;

pub struct DeepSeekProvider {
    api_key: SecretString,
    base_url: String,
}

impl DeepSeekProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: SecretString::from(api_key.into()),
            base_url: "https://api.deepseek.com".into(),
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    fn convert_request(&self, request: &ChatRequest) -> Value {
        let messages: Vec<Value> = std::iter::once(serde_json::json!({
            "role": "system",
            "content": request.system,
        }))
        .chain(request.messages.iter().map(|m| self.convert_message(m)))
        .collect();

        let tools: Vec<Value> = request.tools.iter().map(convert_tool_def).collect();

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "stream": true,
            "stream_options": { "include_usage": true },
            "max_tokens": request.max_tokens,
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
        if let Some(ref effort) = request.reasoning_effort {
            body["reasoning_effort"] = effort.as_str().into();
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

fn convert_tool_def(tool: &ToolDefinition) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("type".into(), "function".into());
    let mut f = serde_json::Map::new();
    f.insert("name".into(), tool.name.clone().into());
    f.insert("description".into(), tool.description.clone().into());
    f.insert("parameters".into(), tool.input_schema.clone());
    m.insert("function".into(), Value::Object(f));
    Value::Object(m)
}

#[async_trait::async_trait]
impl LlmProvider for DeepSeekProvider {
    async fn stream_chat(
        &self,
        request: ChatRequest,
        event_tx: tokio::sync::mpsc::UnboundedSender<StreamEvent>,
    ) -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        let body = self.convert_request(&request);

        let response = client
            .post(format!("{}/chat/completions", self.base_url))
            .header(
                "Authorization",
                format!("Bearer {}", self.api_key.expose_secret()),
            )
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            let _ = event_tx.send(StreamEvent::Error {
                message: format!("DeepSeek API error ({status}): {text}"),
            });
            return Ok(());
        }

        use futures::StreamExt;
        let mut stream = response.bytes_stream();
        let mut line_buf = crate::sse::LineBuffer::new();
        let mut text_buf = String::new();
        let mut current_tool_id: Option<String> = None;
        let mut current_tool_name = String::new();
        let mut current_tool_args = String::new();
        let mut usage = Usage::default();

        let mut stream_done = false;
        while !stream_done {
            let lines: Vec<String> = match stream.next().await {
                Some(chunk) => line_buf.push(&chunk?),
                None => {
                    stream_done = true;
                    line_buf.flush().into_iter().collect()
                }
            };
            for line in lines {
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
                                                    let input: Value =
                                                        serde_json::from_str(&current_tool_args)
                                                            .unwrap_or(Value::Null);
                                                    let _ =
                                                        event_tx.send(StreamEvent::ToolUseDone {
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
                                                let _ = event_tx.send(StreamEvent::ToolUseStart {
                                                    id: id.to_string(),
                                                    name,
                                                });
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
                                if let Some(reasoning) = delta["reasoning_content"].as_str() {
                                    if !reasoning.is_empty() {
                                        let _ = event_tx.send(StreamEvent::ReasoningDelta {
                                            text: reasoning.to_string(),
                                        });
                                    }
                                }
                            }
                            if let Some(reason) = choice["finish_reason"].as_str() {
                                if reason == "tool_calls" && current_tool_id.is_some() {
                                    let id = current_tool_id.take().unwrap();
                                    let input: Value = serde_json::from_str(&current_tool_args)
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
        "deepseek"
    }

    fn default_model(&self) -> &str {
        "deepseek-chat"
    }

    fn supports_prompt_caching(&self) -> bool {
        false
    }

    fn estimate_tokens(&self, text: &str) -> u64 {
        (text.len() as u64).div_ceil(3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ContentBlock, Message, Role};

    fn provider() -> DeepSeekProvider {
        DeepSeekProvider::new("sk-test")
    }

    #[test]
    fn test_convert_simple_user_message() {
        let msg = Message {
            role: Role::User,
            content: vec![ContentBlock::text("hello world")],
        };
        let json = provider().convert_message(&msg);
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"], "hello world");
    }

    #[test]
    fn test_convert_assistant_with_tool_calls() {
        let msg = Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::text("let me search"),
                ContentBlock::tool_use("call_1", "grep", serde_json::json!({"pattern": "test"})),
            ],
        };
        let json = provider().convert_message(&msg);
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["content"], "let me search");
        let tool_calls = json["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["id"], "call_1");
        assert_eq!(tool_calls[0]["function"]["name"], "grep");
    }

    #[test]
    fn test_convert_tool_result_message() {
        let msg = Message {
            role: Role::Tool,
            content: vec![ContentBlock::tool_result(
                "call_1",
                "found 5 matches",
                false,
            )],
        };
        let json = provider().convert_message(&msg);
        assert_eq!(json["role"], "tool");
        assert_eq!(json["tool_call_id"], "call_1");
        assert_eq!(json["content"], "found 5 matches");
    }

    #[test]
    fn test_role_to_str_all_variants() {
        assert_eq!(role_to_str(&Role::System), "system");
        assert_eq!(role_to_str(&Role::User), "user");
        assert_eq!(role_to_str(&Role::Assistant), "assistant");
        assert_eq!(role_to_str(&Role::Tool), "tool");
    }
}
