use crate::provider::{MessageRole, ProviderRequest};

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
    let mut saw_messages = false;

    for message in &request.messages {
        let trimmed = message.content.trim();
        if trimmed.is_empty() {
            continue;
        }

        saw_messages = true;
        match message.role {
            MessageRole::System => system_sections.push(trimmed.to_string()),
            MessageRole::User => {
                push_section(&mut body, "user", trimmed);
            }
            MessageRole::Assistant => {
                push_section(&mut body, "assistant", trimmed);
            }
            MessageRole::Tool => {
                push_section(&mut body, "tool", trimmed);
            }
        }
    }

    if !request.prompt.trim().is_empty() {
        push_section(&mut body, "user", request.prompt.trim());
    } else if !saw_messages {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{MessageRole, ProviderMessage, ProviderModel, ProviderRequest};

    #[test]
    fn builds_system_and_conversation_sections() {
        let prompt = build_claude_prompt(&ProviderRequest {
            model: ProviderModel::claude("sonnet", "Claude Sonnet"),
            system_prompt: Some("provider rules".into()),
            prompt: "latest question".into(),
            messages: vec![
                ProviderMessage {
                    role: MessageRole::System,
                    content: "agent rules".into(),
                },
                ProviderMessage {
                    role: MessageRole::User,
                    content: "first question".into(),
                },
                ProviderMessage {
                    role: MessageRole::Assistant,
                    content: "first answer".into(),
                },
                ProviderMessage {
                    role: MessageRole::Tool,
                    content: "tool result".into(),
                },
            ],
        });

        assert_eq!(
            prompt.system_prompt.as_deref(),
            Some("provider rules\n\nagent rules")
        );
        assert!(prompt.user_prompt.contains("user:\nfirst question"));
        assert!(prompt.user_prompt.contains("assistant:\nfirst answer"));
        assert!(prompt.user_prompt.contains("tool:\ntool result"));
        assert!(prompt.user_prompt.ends_with("user:\nlatest question"));
    }
}
