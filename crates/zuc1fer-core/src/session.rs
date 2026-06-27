use serde::{Deserialize, Serialize};
use zuc1fer_llm::Role;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub working_dir: String,
    pub model: String,
    pub messages: Vec<SessionMessage>,
    pub total_tokens: u64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    pub tool_calls: Option<Vec<serde_json::Value>>,
    pub tool_results: Option<Vec<serde_json::Value>>,
    pub timestamp: String,
}

impl Session {
    pub fn new(id: String, working_dir: String, model: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id,
            working_dir,
            model,
            messages: Vec::new(),
            total_tokens: 0,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn add_message(&mut self, msg: SessionMessage) {
        self.messages.push(msg);
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }

    pub fn to_llm_messages(&self) -> Vec<zuc1fer_llm::Message> {
        self.messages
            .iter()
            .flat_map(|m| {
                let role = match m.role.as_str() {
                    "system" => Role::System,
                    "assistant" => Role::Assistant,
                    "tool" => Role::Tool,
                    _ => Role::User,
                };

                if role == Role::Tool {
                    m.tool_results
                        .as_ref()
                        .map(|tr| {
                            tr.iter()
                                .map(|result| {
                                    let id = result["id"].as_str().unwrap_or("").to_string();
                                    let content =
                                        result["content"].as_str().unwrap_or("").to_string();
                                    let is_error = result["is_error"].as_bool().unwrap_or(false);
                                    zuc1fer_llm::Message {
                                        role: Role::Tool,
                                        content: vec![zuc1fer_llm::ContentBlock::tool_result(
                                            id, content, is_error,
                                        )],
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                } else {
                    let mut blocks = vec![zuc1fer_llm::ContentBlock::text(&m.content)];

                    if let Some(tc) = &m.tool_calls {
                        for call in tc {
                            let id = call["id"].as_str().unwrap_or("").to_string();
                            let name = call["name"].as_str().unwrap_or("").to_string();
                            let input = call["input"].clone();
                            blocks.push(zuc1fer_llm::ContentBlock::tool_use(id, name, input));
                        }
                    }

                    vec![zuc1fer_llm::Message {
                        role,
                        content: blocks,
                    }]
                }
            })
            .collect()
    }
}
