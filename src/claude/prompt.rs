use crate::provider::{MessagePart, MessageRole, ProviderRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudePrompt {
    pub system_prompt: Option<String>,
    pub user_prompt: String,
}

pub fn build_claude_prompt(request: &ProviderRequest) -> ClaudePrompt {
    let mut system_sections = Vec::new();
    if let Some(prompt) = request
        .system_prompt
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        system_sections.push(prompt.trim().to_string());
    }

    let mut body = String::new();

    for message in &request.messages {
        let rendered = render_message_parts(&message.parts);
        if rendered.trim().is_empty() {
            continue;
        }

        match message.role {
            MessageRole::System => system_sections.push(rendered),
            MessageRole::User => {
                push_section(&mut body, "user", &rendered);
            }
            MessageRole::Assistant => {
                push_section(&mut body, "assistant", &rendered);
            }
            MessageRole::Tool => {
                push_section(&mut body, "user", &rendered);
            }
        }
    }

    if body.is_empty() {
        push_section(&mut body, "user", "");
    }

    ClaudePrompt {
        system_prompt: (!system_sections.is_empty()).then(|| system_sections.join("\n\n")),
        user_prompt: body,
    }
}

fn push_section(buffer: &mut String, role: &str, content: &str) {
    if !buffer.is_empty() {
        buffer.push_str("\n\n");
    }
    buffer.push_str(role);
    buffer.push_str(":\n");
    buffer.push_str(content);
}

fn render_message_parts(parts: &[MessagePart]) -> String {
    let mut rendered = Vec::new();

    for part in parts {
        match part {
            MessagePart::Text { text } => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    rendered.push(trimmed.to_string());
                }
            }
            MessagePart::ToolCall {
                call_id,
                tool_name,
                input,
            } => rendered.push(format!(
                "tool_call:\n- id: {call_id}\n- tool: {tool_name}\n- input: {}",
                serde_json::to_string_pretty(input).unwrap_or_else(|_| "{}".into())
            )),
            MessagePart::ToolResult {
                call_id,
                tool_name,
                output,
            } => rendered.push(format!(
                "tool_result:\n- id: {call_id}\n- tool: {}\n- output: {}",
                tool_name.clone().unwrap_or_default(),
                serde_json::to_string_pretty(output).unwrap_or_else(|_| "{}".into())
            )),
        }
    }

    rendered.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{
        MessagePart, MessageRole, ProviderMessage, ProviderModel, ProviderRequest,
    };
    use serde_json::json;

    #[test]
    fn builds_system_and_conversation_sections() {
        let prompt = build_claude_prompt(&ProviderRequest {
            model: ProviderModel::claude("sonnet", "Claude Sonnet"),
            system_prompt: Some("provider rules".into()),
            messages: vec![
                ProviderMessage {
                    role: MessageRole::System,
                    parts: vec![MessagePart::Text {
                        text: "agent rules".into(),
                    }],
                },
                ProviderMessage {
                    role: MessageRole::User,
                    parts: vec![MessagePart::Text {
                        text: "first question".into(),
                    }],
                },
                ProviderMessage {
                    role: MessageRole::Assistant,
                    parts: vec![
                        MessagePart::Text {
                            text: "first answer".into(),
                        },
                        MessagePart::ToolCall {
                            call_id: "toolu_1".into(),
                            tool_name: "Read".into(),
                            input: json!({"file_path": "/tmp/a"}),
                        },
                    ],
                },
                ProviderMessage {
                    role: MessageRole::Tool,
                    parts: vec![MessagePart::ToolResult {
                        call_id: "toolu_1".into(),
                        tool_name: Some("Read".into()),
                        output: json!({"content": "tool result"}),
                    }],
                },
                ProviderMessage {
                    role: MessageRole::User,
                    parts: vec![MessagePart::Text {
                        text: "latest question".into(),
                    }],
                },
            ],
        });

        assert_eq!(
            prompt.system_prompt.as_deref(),
            Some("provider rules\n\nagent rules")
        );
        assert!(prompt.user_prompt.contains("user:\nfirst question"));
        assert!(prompt.user_prompt.contains("assistant:\nfirst answer"));
        assert!(prompt.user_prompt.contains("tool_call:\n- id: toolu_1"));
        assert!(prompt
            .user_prompt
            .contains("user:\ntool_result:\n- id: toolu_1"));
        assert!(prompt.user_prompt.ends_with("user:\nlatest question"));
    }
}
