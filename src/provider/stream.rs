use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StreamPart {
    Start,
    ReasoningStart { id: String },
    ReasoningDelta(ReasoningPart),
    ReasoningEnd { id: String },
    TextStart { id: String },
    TextDelta(TextPart),
    TextEnd { id: String },
    ToolInputStart(ToolInputStartPart),
    ToolInputDelta(ToolInputDeltaPart),
    ToolInputEnd { id: String },
    ToolCall(ToolCallPart),
    Finish { reason: FinishReason },
    Error { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinishReason {
    EndTurn,
    ToolCall,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReasoningPart {
    pub id: String,
    pub delta: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextPart {
    pub id: String,
    pub delta: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallPart {
    pub id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub input: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolInputStartPart {
    pub id: String,
    pub tool_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolInputDeltaPart {
    pub id: String,
    pub delta: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_part_roundtrip() {
        let part = StreamPart::ToolCall(ToolCallPart {
            id: "id-1".into(),
            tool_call_id: "call-1".into(),
            tool_name: "Read".into(),
            input: serde_json::json!({"file_path": "/tmp/a"}),
        });

        let json = serde_json::to_string(&part).unwrap();
        let parsed: StreamPart = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, part);
    }
}
