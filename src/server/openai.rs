use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub tools: Vec<ChatTool>,
    #[serde(default)]
    pub tool_choice: Option<ChatToolChoice>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: ChatContent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChatToolCall>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatContent {
    Text(String),
    Parts(Vec<ChatContentPart>),
}

impl ChatContent {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            ChatContent::Text(s) => Some(s),
            ChatContent::Parts(parts) => parts.iter().find_map(|p| match p {
                ChatContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatContentPart {
    Text { text: String },
    ImageUrl { image_url: ChatImageUrl },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatImageUrl {
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatTool {
    #[serde(rename = "type")]
    pub tool_type: ChatToolType,
    pub function: ChatFunction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatToolType {
    Function,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatFunction {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatToolChoice {
    Mode(ChatToolChoiceMode),
    Function(ChatNamedToolChoice),
    Other(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatToolChoiceMode {
    Auto,
    None,
    Required,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatNamedToolChoice {
    #[serde(rename = "type")]
    pub tool_type: ChatToolType,
    pub function: ChatFunctionChoice,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatFunctionChoice {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: ChatToolType,
    pub function: ChatFunctionCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<ChatUsage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatChoice {
    pub index: u32,
    pub message: ChatMessage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatChunkChoice>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatChunkChoice {
    pub index: u32,
    pub delta: ChatDelta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatDelta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<ChatRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChatToolCallDelta>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatToolCallDelta {
    pub index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<ChatFunctionCallDelta>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatFunctionCallDelta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

pub fn format_sse(data: &str) -> String {
    format!("data: {data}\n\n")
}

pub fn format_sse_done() -> String {
    "data: [DONE]\n\n".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chat_request() {
        let json = r#"{
            "model": "claude-sonnet",
            "messages": [
                {"role": "user", "content": "hello"}
            ],
            "stream": true
        }"#;

        let request: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.model, "claude-sonnet");
        assert_eq!(request.messages.len(), 1);
        assert!(request.stream);
    }

    #[test]
    fn parses_string_tool_choice() {
        let json = r#"{
            "model": "claude-sonnet",
            "messages": [
                {"role": "user", "content": "hello"}
            ],
            "tool_choice": "auto"
        }"#;

        let request: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(
            request.tool_choice,
            Some(ChatToolChoice::Mode(ChatToolChoiceMode::Auto))
        );
    }

    #[test]
    fn parses_function_tool_choice() {
        let json = r#"{
            "model": "claude-sonnet",
            "messages": [
                {"role": "user", "content": "hello"}
            ],
            "tool_choice": {
                "type": "function",
                "function": {"name": "Read"}
            }
        }"#;

        let request: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(
            request.tool_choice,
            Some(ChatToolChoice::Function(ChatNamedToolChoice {
                tool_type: ChatToolType::Function,
                function: ChatFunctionChoice {
                    name: "Read".into(),
                },
            }))
        );
    }

    #[test]
    fn parses_object_wrapped_tool_choice_mode() {
        let json = r#"{
            "model": "claude-sonnet",
            "messages": [
                {"role": "user", "content": "hello"}
            ],
            "tool_choice": {
                "type": "auto"
            }
        }"#;

        let request: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(
            request.tool_choice,
            Some(ChatToolChoice::Other(serde_json::json!({ "type": "auto" })))
        );
    }

    #[test]
    fn parses_unknown_tool_choice_object() {
        let json = r#"{
            "model": "claude-sonnet",
            "messages": [
                {"role": "user", "content": "hello"}
            ],
            "tool_choice": {
                "type": "custom_mode",
                "tool_name": "Read"
            }
        }"#;

        let request: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(
            request.tool_choice,
            Some(ChatToolChoice::Other(serde_json::json!({
                "type": "custom_mode",
                "tool_name": "Read"
            })))
        );
    }

    #[test]
    fn formats_chat_response() {
        let response = ChatResponse {
            id: "chatcmpl-123".into(),
            object: "chat.completion".into(),
            created: 1700000000,
            model: "claude-sonnet".into(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatMessage {
                    role: ChatRole::Assistant,
                    content: ChatContent::Text("hello".into()),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".into()),
            }],
            usage: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("chat.completion"));
        assert!(json.contains("hello"));
    }
}
