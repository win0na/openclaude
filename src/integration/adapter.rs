use crate::provider::{
    ProviderRuntime, ProviderSession, SessionState, SessionStep, StreamPart, ToolCallPart,
    ToolResult,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AdapterEvent {
    Start,
    ReasoningDelta { id: String, delta: String },
    TextDelta { id: String, delta: String },
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

    pub fn submit_tool_result(&mut self, result: ToolResult) -> anyhow::Result<AdapterStep> {
        let step = self.session.submit_tool_result(result)?;
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

fn map_stream_part(part: StreamPart) -> Option<AdapterEvent> {
    match part {
        StreamPart::Start => Some(AdapterEvent::Start),
        StreamPart::ReasoningDelta(reasoning) => Some(AdapterEvent::ReasoningDelta {
            id: reasoning.id,
            delta: reasoning.delta,
        }),
        StreamPart::TextDelta(text) => Some(AdapterEvent::TextDelta {
            id: text.id,
            delta: text.delta,
        }),
        StreamPart::ToolCall(tool_call) => Some(AdapterEvent::ToolCall(map_tool_call(tool_call))),
        StreamPart::Finish { reason } => Some(AdapterEvent::Finish {
            reason: format!("{reason:?}"),
        }),
        StreamPart::Error { message } => Some(AdapterEvent::Error { message }),
        StreamPart::ReasoningStart { .. }
        | StreamPart::ReasoningEnd { .. }
        | StreamPart::TextStart { .. }
        | StreamPart::TextEnd { .. }
        | StreamPart::ToolInputStart(_)
        | StreamPart::ToolInputDelta(_)
        | StreamPart::ToolInputEnd { .. } => None,
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
    use crate::provider::{FinishReason, SessionState, TextPart};
    use serde_json::json;

    #[test]
    fn maps_waiting_step_to_adapter_shape() {
        let step = SessionStep {
            parts: vec![
                StreamPart::Start,
                StreamPart::TextDelta(TextPart {
                    id: "part-0".into(),
                    delta: "hello".into(),
                }),
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
        assert!(mapped.events.iter().any(
            |event| matches!(event, AdapterEvent::ToolCall(call) if call.tool_name == "Read")
        ));
    }
}
