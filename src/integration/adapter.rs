use crate::provider::{
    ProviderRuntime, ProviderSession, SessionState, SessionStep, StreamPart, ToolCallPart,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AdapterEvent {
    Start,
    ReasoningStart { id: String },
    ReasoningDelta { id: String, delta: String },
    ReasoningEnd { id: String },
    TextStart { id: String },
    TextDelta { id: String, delta: String },
    TextEnd { id: String },
    ToolInputStart { id: String, tool_name: String },
    ToolInputDelta { id: String, delta: String },
    ToolInputEnd { id: String },
    ToolCall(AdapterToolCall),
    Finish { reason: String },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdapterToolCall {
    pub call_id: String,
    pub tool_name: String,
    pub input: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AdapterSessionState {
    Ready,
    WaitingForTool(AdapterToolCall),
    Finished,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdapterStep {
    pub events: Vec<AdapterEvent>,
    pub state: AdapterSessionState,
}

#[derive(Clone)]
pub struct OpenCodeAdapter<R: ProviderRuntime> {
    session: ProviderSession<R>,
}

impl<R: ProviderRuntime> OpenCodeAdapter<R> {
    pub fn new(runtime: R) -> Self {
        Self {
            session: ProviderSession::new(runtime),
        }
    }

    pub fn start(
        &mut self,
        request: crate::provider::ProviderRequest,
    ) -> anyhow::Result<AdapterStep> {
        let step = self.session.start(request)?;
        Ok(map_session_step(step))
    }
}

pub fn map_session_step(step: SessionStep) -> AdapterStep {
    AdapterStep {
        events: step.parts.into_iter().filter_map(map_stream_part).collect(),
        state: map_session_state(step.state),
    }
}

fn map_session_state(state: SessionState) -> AdapterSessionState {
    match state {
        SessionState::Ready => AdapterSessionState::Ready,
        SessionState::WaitingForTool(tool_call) => {
            AdapterSessionState::WaitingForTool(map_tool_call(tool_call))
        }
        SessionState::Finished => AdapterSessionState::Finished,
    }
}

pub(crate) fn map_stream_part(part: StreamPart) -> Option<AdapterEvent> {
    match part {
        StreamPart::Start => Some(AdapterEvent::Start),
        StreamPart::ReasoningStart { id } => Some(AdapterEvent::ReasoningStart { id }),
        StreamPart::ReasoningDelta(reasoning) => Some(AdapterEvent::ReasoningDelta {
            id: reasoning.id,
            delta: reasoning.delta,
        }),
        StreamPart::ReasoningEnd { id } => Some(AdapterEvent::ReasoningEnd { id }),
        StreamPart::TextStart { id } => Some(AdapterEvent::TextStart { id }),
        StreamPart::TextDelta(text) => Some(AdapterEvent::TextDelta {
            id: text.id,
            delta: text.delta,
        }),
        StreamPart::TextEnd { id } => Some(AdapterEvent::TextEnd { id }),
        StreamPart::ToolInputStart(tool) => Some(AdapterEvent::ToolInputStart {
            id: tool.id,
            tool_name: tool.tool_name,
        }),
        StreamPart::ToolInputDelta(tool) => Some(AdapterEvent::ToolInputDelta {
            id: tool.id,
            delta: tool.delta,
        }),
        StreamPart::ToolInputEnd { id } => Some(AdapterEvent::ToolInputEnd { id }),
        StreamPart::ToolCall(tool_call) => Some(AdapterEvent::ToolCall(map_tool_call(tool_call))),
        StreamPart::Finish { reason } => Some(AdapterEvent::Finish {
            reason: match reason {
                crate::provider::FinishReason::EndTurn => "end_turn".into(),
                crate::provider::FinishReason::ToolCall => "tool_call".into(),
                crate::provider::FinishReason::Error => "error".into(),
            },
        }),
        StreamPart::Error { message } => Some(AdapterEvent::Error { message }),
    }
}

fn map_tool_call(tool_call: ToolCallPart) -> AdapterToolCall {
    AdapterToolCall {
        call_id: tool_call.tool_call_id,
        tool_name: tool_call.tool_name,
        input: tool_call.input,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{
        FinishReason, ReasoningPart, SessionState, TextPart, ToolInputDeltaPart, ToolInputStartPart,
    };
    use serde_json::json;

    #[test]
    fn maps_waiting() {
        let step = SessionStep {
            parts: vec![
                StreamPart::Start,
                StreamPart::ReasoningStart {
                    id: "part-r".into(),
                },
                StreamPart::ReasoningDelta(ReasoningPart {
                    id: "part-r".into(),
                    delta: "thinking".into(),
                }),
                StreamPart::ReasoningEnd {
                    id: "part-r".into(),
                },
                StreamPart::TextStart {
                    id: "part-0".into(),
                },
                StreamPart::TextDelta(TextPart {
                    id: "part-0".into(),
                    delta: "hello".into(),
                }),
                StreamPart::TextEnd {
                    id: "part-0".into(),
                },
                StreamPart::ToolInputStart(ToolInputStartPart {
                    id: "toolu_1".into(),
                    tool_name: "Read".into(),
                }),
                StreamPart::ToolInputDelta(ToolInputDeltaPart {
                    id: "toolu_1".into(),
                    delta: "{\"file_path\":\"/tmp/a\"}".into(),
                }),
                StreamPart::ToolInputEnd {
                    id: "toolu_1".into(),
                },
                StreamPart::ToolCall(ToolCallPart {
                    id: "toolu_1".into(),
                    tool_call_id: "toolu_1".into(),
                    tool_name: "Read".into(),
                    input: json!({"file_path": "/tmp/a"}),
                }),
                StreamPart::Finish {
                    reason: FinishReason::ToolCall,
                },
            ],
            state: SessionState::WaitingForTool(ToolCallPart {
                id: "toolu_1".into(),
                tool_call_id: "toolu_1".into(),
                tool_name: "Read".into(),
                input: json!({"file_path": "/tmp/a"}),
            }),
        };

        let mapped = map_session_step(step);
        assert!(matches!(
            mapped.state,
            AdapterSessionState::WaitingForTool(_)
        ));
        assert!(mapped
            .events
            .iter()
            .any(|event| matches!(event, AdapterEvent::ReasoningStart { id } if id == "part-r")));
        assert!(mapped.events.iter().any(
            |event| matches!(event, AdapterEvent::ToolInputDelta { id, .. } if id == "toolu_1")
        ));
        assert!(mapped.events.iter().any(
            |event| matches!(event, AdapterEvent::ToolCall(call) if call.tool_name == "Read")
        ));
        assert!(mapped.events.iter().any(
            |event| matches!(event, AdapterEvent::Finish { reason } if reason == "tool_call")
        ));
    }
}
