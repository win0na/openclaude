use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ClaudeChunk {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub event: Option<ClaudeStreamEvent>,
    #[serde(default)]
    pub message: Option<ClaudeAssistantMessage>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ClaudeStreamEvent {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub index: Option<u32>,
    #[serde(default, rename = "content_block")]
    pub content_block: Option<ClaudeContentBlock>,
    #[serde(default)]
    pub delta: Option<ClaudeDelta>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ClaudeAssistantMessage {
    pub content: Vec<ClaudeContentBlock>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type")]
pub enum ClaudeContentBlock {
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(default)]
        signature: Option<String>,
    },
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        #[serde(default)]
        input: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type")]
pub enum ClaudeDelta {
    #[serde(rename = "thinking_delta")]
    Thinking { thinking: String },
    #[serde(rename = "text_delta")]
    Text { text: String },
    #[serde(rename = "input_json_delta")]
    InputJson { partial_json: String },
    #[serde(other)]
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_thinking_delta() {
        let value = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"abc"}}}"#;
        let parsed: ClaudeChunk = serde_json::from_str(value).unwrap();
        let event = parsed.event.unwrap();
        assert!(
            matches!(event.delta.unwrap(), ClaudeDelta::Thinking { thinking } if thinking == "abc")
        );
    }
}
