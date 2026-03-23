use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ClaudeChunk {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub event: Option<ClaudeStreamEvent>,
    #[serde(default)]
    pub message: Option<ClaudeMessage>,
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
    pub delta: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ClaudeMessage {
    #[serde(default)]
    pub role: Option<String>,
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
    #[serde(rename = "tool_result")]
    ToolResult {
        #[serde(rename = "tool_use_id")]
        tool_use_id: String,
        #[serde(default)]
        content: Value,
        #[serde(default)]
        is_error: Option<bool>,
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
        let delta: ClaudeDelta = serde_json::from_value(event.delta.unwrap()).unwrap();
        assert!(matches!(delta, ClaudeDelta::Thinking { thinking } if thinking == "abc"));
    }

    #[test]
    fn parses_message_delta_as_raw_json() {
        let value = r#"{"type":"stream_event","event":{"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null}}}"#;
        let parsed: ClaudeChunk = serde_json::from_str(value).unwrap();
        let event = parsed.event.unwrap();

        assert_eq!(
            event.delta.unwrap(),
            serde_json::json!({
                "stop_reason": "end_turn",
                "stop_sequence": null
            })
        );
    }

    #[test]
    fn parses_user_tool_result_chunk() {
        let value = r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_1","type":"tool_result","content":"ok","is_error":false}]}}"#;
        let parsed: ClaudeChunk = serde_json::from_str(value).unwrap();
        let message = parsed.message.unwrap();

        assert_eq!(message.role.as_deref(), Some("user"));
        assert!(matches!(
            &message.content[0],
            ClaudeContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "toolu_1"
        ));
    }
}
