use crate::provider::{ProviderRequest, ProviderRuntime, StreamPart, ToolCallPart};

#[derive(Debug, Clone, PartialEq)]
pub enum SessionState {
    Ready,
    WaitingForTool(ToolCallPart),
    Finished,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionStep {
    pub parts: Vec<StreamPart>,
    pub state: SessionState,
}

#[derive(Clone)]
pub struct ProviderSession<R: ProviderRuntime> {
    runtime: R,
    state: SessionState,
}

impl<R: ProviderRuntime> ProviderSession<R> {
    pub fn new(runtime: R) -> Self {
        Self {
            runtime,
            state: SessionState::Ready,
        }
    }

    pub fn state(&self) -> &SessionState {
        &self.state
    }

    pub fn start(&mut self, request: ProviderRequest) -> anyhow::Result<SessionStep> {
        self.run_request(request)
    }

    fn run_request(&mut self, request: ProviderRequest) -> anyhow::Result<SessionStep> {
        let parts = self
            .runtime
            .stream(request)?
            .collect::<anyhow::Result<Vec<_>>>()?;

        if let Some(tool_call) = parts.iter().find_map(extract_tool_call).cloned() {
            self.state = SessionState::WaitingForTool(tool_call);
        } else {
            self.state = SessionState::Finished;
        }

        Ok(SessionStep {
            parts,
            state: self.state.clone(),
        })
    }
}

fn extract_tool_call(part: &StreamPart) -> Option<&ToolCallPart> {
    match part {
        StreamPart::ToolCall(tool_call) => Some(tool_call),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{
        FinishReason, MessagePart, MessageRole, ProviderInfo, ProviderMessage, ProviderModel,
        ProviderRequest,
    };
    use serde_json::json;
    use std::collections::VecDeque;

    #[derive(Clone)]
    struct MockRuntime {
        streams: std::sync::Arc<std::sync::Mutex<VecDeque<Vec<StreamPart>>>>,
        model: ProviderModel,
    }

    impl MockRuntime {
        fn new(streams: Vec<Vec<StreamPart>>) -> Self {
            Self {
                streams: std::sync::Arc::new(std::sync::Mutex::new(streams.into())),
                model: ProviderModel::claude("sonnet", "Claude Sonnet"),
            }
        }
    }

    impl ProviderRuntime for MockRuntime {
        fn info(&self) -> ProviderInfo {
            ProviderInfo {
                id: "mock".into(),
                name: "Mock".into(),
            }
        }

        fn models(&self) -> &[ProviderModel] {
            std::slice::from_ref(&self.model)
        }

        fn stream(
            &self,
            _request: ProviderRequest,
        ) -> anyhow::Result<std::vec::IntoIter<anyhow::Result<StreamPart>>> {
            let next = self.streams.lock().unwrap().pop_front().unwrap_or_default();
            Ok(next.into_iter().map(Ok).collect::<Vec<_>>().into_iter())
        }
    }

    fn base_request() -> ProviderRequest {
        ProviderRequest {
            model: ProviderModel::claude("sonnet", "Claude Sonnet"),
            system_prompt: None,
            messages: vec![ProviderMessage {
                role: MessageRole::User,
                parts: vec![MessagePart::Text {
                    text: "earlier\n\nhello".into(),
                }],
            }],
        }
    }

    #[test]
    fn session_enters_waiting_state_on_tool_call() {
        let runtime = MockRuntime::new(vec![vec![
            StreamPart::Start,
            StreamPart::ToolCall(ToolCallPart {
                id: "toolu_1".into(),
                tool_call_id: "toolu_1".into(),
                tool_name: "Read".into(),
                input: json!({"file_path": "/tmp/a"}),
            }),
            StreamPart::Finish {
                reason: FinishReason::ToolCall,
            },
        ]]);

        let mut session = ProviderSession::new(runtime);
        let step = session.start(base_request()).unwrap();

        assert!(matches!(step.state, SessionState::WaitingForTool(_)));
        assert!(matches!(session.state(), SessionState::WaitingForTool(_)));
    }
}
