use crate::claude::stream::{ClaudeChunk, ClaudeContentBlock, ClaudeDelta, ClaudeStreamEvent};
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
}

impl ClaudeTranslator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_chunk(&mut self, chunk: &ClaudeChunk) -> Vec<StreamPart> {
        match chunk.kind.as_str() {
            "stream_event" => chunk
                .event
                .as_ref()
                .map(|event| self.push_stream_event(event))
                .unwrap_or_default(),
            "assistant" => chunk
                .message
                .as_ref()
                .map(assistant_message_to_parts)
                .unwrap_or_default(),
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
                        tool_name: name.clone(),
                        raw_input,
                        parsed_input: input.clone(),
                    },
                );
                vec![StreamPart::ToolInputStart(ToolInputStartPart {
                    id: id.clone(),
                    tool_name: name.clone(),
                })]
            }
            None => Vec::new(),
        }
    }

    fn handle_block_delta(&mut self, event: &ClaudeStreamEvent) -> Vec<StreamPart> {
        let Some(index) = event.index else {
            return Vec::new();
        };

        match event.delta.as_ref() {
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
                        tool_name: tool_use.tool_name,
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

fn assistant_message_to_parts(
    message: &crate::claude::stream::ClaudeAssistantMessage,
) -> Vec<StreamPart> {
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
                    tool_name: name.clone(),
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
                    tool_name: name.clone(),
                    input: input.clone(),
                }));
            }
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
        ClaudeAssistantMessage, ClaudeChunk, ClaudeContentBlock, ClaudeDelta, ClaudeStreamEvent,
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
                delta: Some(ClaudeDelta::Thinking {
                    thinking: "abc".into(),
                }),
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
            message: Some(ClaudeAssistantMessage {
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
                    tool_name: "Read".into(),
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
                    tool_name: "Read".into(),
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
                tool_name: "Edit".into(),
            })]
        );

        let delta_one = ClaudeChunk {
            kind: "stream_event".into(),
            event: Some(ClaudeStreamEvent {
                kind: "content_block_delta".into(),
                index: Some(1),
                content_block: None,
                delta: Some(ClaudeDelta::InputJson {
                    partial_json: r#"{"file_path":"/tmp/a""#.into(),
                }),
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
                delta: Some(ClaudeDelta::InputJson {
                    partial_json: r#", "old_string":"x"}"#.into(),
                }),
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
                    tool_name: "Edit".into(),
                    input: json!({"file_path":"/tmp/a", "old_string":"x"}),
                }),
            ]
        );
    }
}
