use crate::claude::stream::{
    ClaudeChunk, ClaudeContentBlock, ClaudeDelta, ClaudeMessage, ClaudeStreamEvent,
};
use crate::claude::tool_name::normalize_tool_name;
use crate::provider::stream::{
    ReasoningPart, StreamPart, TextPart, ToolCallPart, ToolInputDeltaPart, ToolInputStartPart,
};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveBlockKind {
    Thinking,
    Text,
    ToolUse,
}

#[derive(Debug, Clone, PartialEq)]
struct ActiveToolUse {
    id: String,
    tool_name: String,
    raw_input: String,
    parsed_input: Value,
}

#[derive(Debug, Default)]
pub struct ClaudeTranslator {
    active_blocks: HashMap<u32, ActiveBlockKind>,
    active_tool_uses: HashMap<u32, ActiveToolUse>,
    saw_stream_event: bool,
}

impl ClaudeTranslator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_chunk(&mut self, chunk: &ClaudeChunk) -> Vec<StreamPart> {
        match chunk.kind.as_str() {
            "stream_event" => {
                self.saw_stream_event = true;
                chunk
                    .event
                    .as_ref()
                    .map(|event| self.push_stream_event(event))
                    .unwrap_or_default()
            }
            "assistant" => {
                if self.saw_stream_event {
                    Vec::new()
                } else {
                    chunk
                        .message
                        .as_ref()
                        .map(assistant_message_to_parts)
                        .unwrap_or_default()
                }
            }
            _ => Vec::new(),
        }
    }

    fn push_stream_event(&mut self, event: &ClaudeStreamEvent) -> Vec<StreamPart> {
        match event.kind.as_str() {
            "content_block_start" => self.handle_block_start(event),
            "content_block_delta" => self.handle_block_delta(event),
            "content_block_stop" => self.handle_block_stop(event),
            _ => Vec::new(),
        }
    }

    fn handle_block_start(&mut self, event: &ClaudeStreamEvent) -> Vec<StreamPart> {
        let Some(index) = event.index else {
            return Vec::new();
        };

        match event.content_block.as_ref() {
            Some(ClaudeContentBlock::Thinking { .. }) => {
                self.active_blocks.insert(index, ActiveBlockKind::Thinking);
                vec![StreamPart::ReasoningStart { id: part_id(index) }]
            }
            Some(ClaudeContentBlock::Text { .. }) => {
                self.active_blocks.insert(index, ActiveBlockKind::Text);
                vec![StreamPart::TextStart { id: part_id(index) }]
            }
            Some(ClaudeContentBlock::ToolUse { id, name, input }) => {
                self.active_blocks.insert(index, ActiveBlockKind::ToolUse);
                let raw_input = if input.is_null() || input == &Value::Object(Default::default()) {
                    String::new()
                } else {
                    input.to_string()
                };
                self.active_tool_uses.insert(
                    index,
                    ActiveToolUse {
                        id: id.clone(),
                        tool_name: normalize_tool_name(name).to_string(),
                        raw_input,
                        parsed_input: input.clone(),
                    },
                );
                vec![StreamPart::ToolInputStart(ToolInputStartPart {
                    id: id.clone(),
                    tool_name: normalize_tool_name(name).to_string(),
                })]
            }
            Some(ClaudeContentBlock::ToolResult { .. }) => Vec::new(),
            None => Vec::new(),
        }
    }

    fn handle_block_delta(&mut self, event: &ClaudeStreamEvent) -> Vec<StreamPart> {
        let Some(index) = event.index else {
            return Vec::new();
        };

        let delta = event
            .delta
            .as_ref()
            .and_then(|value| serde_json::from_value::<ClaudeDelta>(value.clone()).ok());

        match delta.as_ref() {
            Some(ClaudeDelta::Thinking { thinking }) => {
                vec![StreamPart::ReasoningDelta(ReasoningPart {
                    id: part_id(index),
                    delta: thinking.clone(),
                })]
            }
            Some(ClaudeDelta::Text { text }) => vec![StreamPart::TextDelta(TextPart {
                id: part_id(index),
                delta: text.clone(),
            })],
            Some(ClaudeDelta::InputJson { partial_json }) => {
                let Some(tool_use) = self.active_tool_uses.get_mut(&index) else {
                    return Vec::new();
                };
                tool_use.raw_input.push_str(partial_json);
                if let Ok(value) = serde_json::from_str::<Value>(&tool_use.raw_input) {
                    tool_use.parsed_input = value;
                }
                vec![StreamPart::ToolInputDelta(ToolInputDeltaPart {
                    id: tool_use.id.clone(),
                    delta: partial_json.clone(),
                })]
            }
            Some(ClaudeDelta::Other) | None => Vec::new(),
        }
    }

    fn handle_block_stop(&mut self, event: &ClaudeStreamEvent) -> Vec<StreamPart> {
        let Some(index) = event.index else {
            return Vec::new();
        };

        match self.active_blocks.remove(&index) {
            Some(ActiveBlockKind::Thinking) => {
                vec![StreamPart::ReasoningEnd { id: part_id(index) }]
            }
            Some(ActiveBlockKind::Text) => vec![StreamPart::TextEnd { id: part_id(index) }],
            Some(ActiveBlockKind::ToolUse) => {
                let Some(tool_use) = self.active_tool_uses.remove(&index) else {
                    return Vec::new();
                };
                vec![
                    StreamPart::ToolInputEnd {
                        id: tool_use.id.clone(),
                    },
                    StreamPart::ToolCall(ToolCallPart {
                        id: tool_use.id.clone(),
                        tool_call_id: tool_use.id,
                        tool_name: normalize_tool_name(&tool_use.tool_name).to_string(),
                        input: tool_use.parsed_input,
                    }),
                ]
            }
            None => Vec::new(),
        }
    }
}

pub fn chunk_to_stream_parts(chunk: &ClaudeChunk) -> Vec<StreamPart> {
    let mut translator = ClaudeTranslator::new();
    translator.push_chunk(chunk)
}

fn assistant_message_to_parts(message: &ClaudeMessage) -> Vec<StreamPart> {
    let mut parts = Vec::new();

    for (index, block) in message.content.iter().enumerate() {
        let part_id = part_id(index as u32);
        match block {
            ClaudeContentBlock::Thinking { thinking, .. } if !thinking.is_empty() => {
                parts.push(StreamPart::ReasoningStart {
                    id: part_id.clone(),
                });
                parts.push(StreamPart::ReasoningDelta(ReasoningPart {
                    id: part_id.clone(),
                    delta: thinking.clone(),
                }));
                parts.push(StreamPart::ReasoningEnd { id: part_id });
            }
            ClaudeContentBlock::Text { text } if !text.is_empty() => {
                parts.push(StreamPart::TextStart {
                    id: part_id.clone(),
                });
                parts.push(StreamPart::TextDelta(TextPart {
                    id: part_id.clone(),
                    delta: text.clone(),
                }));
                parts.push(StreamPart::TextEnd { id: part_id });
            }
            ClaudeContentBlock::ToolUse { id, name, input } => {
                parts.push(StreamPart::ToolInputStart(ToolInputStartPart {
                    id: id.clone(),
                    tool_name: normalize_tool_name(name).to_string(),
                }));
                let raw = input.to_string();
                if !raw.is_empty() && raw != "{}" {
                    parts.push(StreamPart::ToolInputDelta(ToolInputDeltaPart {
                        id: id.clone(),
                        delta: raw,
                    }));
                }
                parts.push(StreamPart::ToolInputEnd { id: id.clone() });
                parts.push(StreamPart::ToolCall(ToolCallPart {
                    id: id.clone(),
                    tool_call_id: id.clone(),
                    tool_name: normalize_tool_name(name).to_string(),
                    input: input.clone(),
                }));
            }
            ClaudeContentBlock::ToolResult { .. } => {}
            _ => {}
        }
    }

    parts
}

fn part_id(index: u32) -> String {
    format!("part-{index}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::stream::{
        ClaudeChunk, ClaudeContentBlock, ClaudeMessage, ClaudeStreamEvent,
    };
    use serde_json::json;

    #[test]
    fn stream_thinking_delta_maps_to_reasoning_delta() {
        let chunk = ClaudeChunk {
            kind: "stream_event".into(),
            event: Some(ClaudeStreamEvent {
                kind: "content_block_delta".into(),
                index: Some(0),
                content_block: None,
                delta: Some(json!({
                    "type": "thinking_delta",
                    "thinking": "abc"
                })),
            }),
            message: None,
        };

        let mut translator = ClaudeTranslator::new();
        assert_eq!(
            translator.push_chunk(&chunk),
            vec![StreamPart::ReasoningDelta(ReasoningPart {
                id: "part-0".into(),
                delta: "abc".into(),
            })]
        );
    }

    #[test]
    fn assistant_tool_use_maps_to_tool_input_and_tool_call() {
        let chunk = ClaudeChunk {
            kind: "assistant".into(),
            event: None,
            message: Some(ClaudeMessage {
                role: Some("assistant".into()),
                content: vec![ClaudeContentBlock::ToolUse {
                    id: "toolu_1".into(),
                    name: "Read".into(),
                    input: json!({"file_path": "/tmp/a"}),
                }],
            }),
        };

        assert_eq!(
            chunk_to_stream_parts(&chunk),
            vec![
                StreamPart::ToolInputStart(ToolInputStartPart {
                    id: "toolu_1".into(),
                    tool_name: "read".into(),
                }),
                StreamPart::ToolInputDelta(ToolInputDeltaPart {
                    id: "toolu_1".into(),
                    delta: json!({"file_path": "/tmp/a"}).to_string(),
                }),
                StreamPart::ToolInputEnd {
                    id: "toolu_1".into()
                },
                StreamPart::ToolCall(ToolCallPart {
                    id: "toolu_1".into(),
                    tool_call_id: "toolu_1".into(),
                    tool_name: "read".into(),
                    input: json!({"file_path": "/tmp/a"}),
                }),
            ]
        );
    }

    #[test]
    fn streamed_tool_input_json_accumulates_until_stop() {
        let mut translator = ClaudeTranslator::new();

        let start = ClaudeChunk {
            kind: "stream_event".into(),
            event: Some(ClaudeStreamEvent {
                kind: "content_block_start".into(),
                index: Some(1),
                content_block: Some(ClaudeContentBlock::ToolUse {
                    id: "toolu_2".into(),
                    name: "Edit".into(),
                    input: json!({}),
                }),
                delta: None,
            }),
            message: None,
        };
        assert_eq!(
            translator.push_chunk(&start),
            vec![StreamPart::ToolInputStart(ToolInputStartPart {
                id: "toolu_2".into(),
                tool_name: "edit".into(),
            })]
        );

        let delta_one = ClaudeChunk {
            kind: "stream_event".into(),
            event: Some(ClaudeStreamEvent {
                kind: "content_block_delta".into(),
                index: Some(1),
                content_block: None,
                delta: Some(json!({
                    "type": "input_json_delta",
                    "partial_json": r#"{"file_path":"/tmp/a""#
                })),
            }),
            message: None,
        };
        assert_eq!(translator.push_chunk(&delta_one).len(), 1);

        let delta_two = ClaudeChunk {
            kind: "stream_event".into(),
            event: Some(ClaudeStreamEvent {
                kind: "content_block_delta".into(),
                index: Some(1),
                content_block: None,
                delta: Some(json!({
                    "type": "input_json_delta",
                    "partial_json": r#", "old_string":"x"}"#
                })),
            }),
            message: None,
        };
        assert_eq!(translator.push_chunk(&delta_two).len(), 1);

        let stop = ClaudeChunk {
            kind: "stream_event".into(),
            event: Some(ClaudeStreamEvent {
                kind: "content_block_stop".into(),
                index: Some(1),
                content_block: None,
                delta: None,
            }),
            message: None,
        };

        assert_eq!(
            translator.push_chunk(&stop),
            vec![
                StreamPart::ToolInputEnd {
                    id: "toolu_2".into()
                },
                StreamPart::ToolCall(ToolCallPart {
                    id: "toolu_2".into(),
                    tool_call_id: "toolu_2".into(),
                    tool_name: "edit".into(),
                    input: json!({"file_path":"/tmp/a", "old_string":"x"}),
                }),
            ]
        );
    }

    #[test]
    fn ignores_assistant_summary_after_stream_events() {
        let mut translator = ClaudeTranslator::new();

        let streamed = ClaudeChunk {
            kind: "stream_event".into(),
            event: Some(ClaudeStreamEvent {
                kind: "content_block_delta".into(),
                index: Some(0),
                content_block: None,
                delta: Some(json!({
                    "type": "text_delta",
                    "text": "hello"
                })),
            }),
            message: None,
        };

        let assistant = ClaudeChunk {
            kind: "assistant".into(),
            event: None,
            message: Some(ClaudeMessage {
                role: Some("assistant".into()),
                content: vec![ClaudeContentBlock::Text {
                    text: "hello".into(),
                }],
            }),
        };

        assert_eq!(translator.push_chunk(&streamed).len(), 1);
        assert!(translator.push_chunk(&assistant).is_empty());
    }

    #[test]
    fn ignores_user_tool_result_chunks() {
        let chunk = ClaudeChunk {
            kind: "user".into(),
            event: None,
            message: Some(ClaudeMessage {
                role: Some("user".into()),
                content: vec![ClaudeContentBlock::ToolResult {
                    tool_use_id: "toolu_1".into(),
                    content: json!("ok"),
                    is_error: Some(false),
                }],
            }),
        };

        assert!(chunk_to_stream_parts(&chunk).is_empty());
    }

    #[test]
    fn normalizes_claude_native_websearch_tool_name() {
        let chunk = ClaudeChunk {
            kind: "assistant".into(),
            event: None,
            message: Some(ClaudeMessage {
                role: Some("assistant".into()),
                content: vec![ClaudeContentBlock::ToolUse {
                    id: "toolu_1".into(),
                    name: "WebSearch".into(),
                    input: json!({"query": "rust"}),
                }],
            }),
        };

        let parts = chunk_to_stream_parts(&chunk);
        assert!(matches!(
            &parts[0],
            StreamPart::ToolInputStart(ToolInputStartPart { tool_name, .. }) if tool_name == "websearch_web_search_exa"
        ));
        assert!(matches!(
            &parts[3],
            StreamPart::ToolCall(ToolCallPart { tool_name, .. }) if tool_name == "websearch_web_search_exa"
        ));
    }

    #[test]
    fn normalizes_claude_native_toolsearch_tool_name() {
        let chunk = ClaudeChunk {
            kind: "assistant".into(),
            event: None,
            message: Some(ClaudeMessage {
                role: Some("assistant".into()),
                content: vec![ClaudeContentBlock::ToolUse {
                    id: "toolu_9".into(),
                    name: "ToolSearch".into(),
                    input: json!({"query": "tool names"}),
                }],
            }),
        };

        let parts = chunk_to_stream_parts(&chunk);
        assert!(matches!(
            &parts[0],
            StreamPart::ToolInputStart(ToolInputStartPart { tool_name, .. }) if tool_name == "websearch_web_search_exa"
        ));
        assert!(matches!(
            &parts[3],
            StreamPart::ToolCall(ToolCallPart { tool_name, .. }) if tool_name == "websearch_web_search_exa"
        ));
    }

    #[test]
    fn normalizes_claude_native_multiedit_tool_name() {
        let mut translator = ClaudeTranslator::new();
        let chunk = ClaudeChunk {
            kind: "stream_event".into(),
            event: Some(ClaudeStreamEvent {
                kind: "content_block_start".into(),
                index: Some(0),
                content_block: Some(ClaudeContentBlock::ToolUse {
                    id: "toolu_2".into(),
                    name: "MultiEdit".into(),
                    input: json!({}),
                }),
                delta: None,
            }),
            message: None,
        };

        let parts = translator.push_chunk(&chunk);
        assert!(matches!(
            &parts[0],
            StreamPart::ToolInputStart(ToolInputStartPart { tool_name, .. }) if tool_name == "edit"
        ));
    }
}
