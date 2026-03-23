use crate::integration::OpenCodeBridge;
use crate::provider::ProviderRuntime;
use crate::server::{
    ServerContinueRequest, ServerMetadata, ServerModel, ServerRequest, ServerResponse,
};
use std::collections::HashMap;

pub struct OpenClaudeService<R: ProviderRuntime + Clone> {
    template: OpenCodeBridge<R>,
    sessions: HashMap<String, OpenCodeBridge<R>>,
    next_session_id: u64,
}

impl<R: ProviderRuntime + Clone> OpenClaudeService<R> {
    pub fn new(bridge: OpenCodeBridge<R>) -> Self {
        Self {
            template: bridge,
            sessions: HashMap::new(),
            next_session_id: 1,
        }
    }

    pub fn describe(&self) -> ServerResponse {
        ServerResponse {
            session_id: None,
            metadata: Some(ServerMetadata {
                provider_id: self.template.provider_info().id.clone(),
                provider_name: self.template.provider_info().name.clone(),
                models: self
                    .template
                    .models()
                    .into_iter()
                    .map(ServerModel::from)
                    .collect(),
            }),
            step: crate::integration::AdapterStep {
                events: Vec::new(),
                state: crate::integration::AdapterSessionState::Ready,
            },
        }
    }

    pub fn start(&mut self, request: ServerRequest) -> anyhow::Result<ServerResponse> {
        let session_id = format!("session-{}", self.next_session_id);
        self.next_session_id += 1;

        let mut bridge = self.template.clone();
        let response_step = bridge.start(request.conversation)?;
        let should_keep = !matches!(
            response_step.state,
            crate::integration::AdapterSessionState::Finished
        );
        if should_keep {
            self.sessions.insert(session_id.clone(), bridge);
        }

        Ok(ServerResponse {
            session_id: Some(session_id),
            metadata: None,
            step: response_step,
        })
    }

    pub fn resume(&mut self, request: ServerContinueRequest) -> anyhow::Result<ServerResponse> {
        let mut bridge = self
            .sessions
            .remove(&request.session_id)
            .ok_or_else(|| anyhow::anyhow!("unknown session id: {}", request.session_id))?;
        let response_step = bridge.submit_tool_result(request.tool_result)?;
        let session_id = request.session_id;
        let should_keep = !matches!(
            response_step.state,
            crate::integration::AdapterSessionState::Finished
        );
        if should_keep {
            self.sessions.insert(session_id.clone(), bridge);
        }

        Ok(ServerResponse {
            session_id: Some(session_id),
            metadata: None,
            step: response_step,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integration::{
        AdapterEvent, AdapterSessionState, BridgeMessage, BridgeRequest, BridgeRole,
        BridgeToolResult,
    };
    use crate::provider::{
        FinishReason, ProviderInfo, ProviderModel, ProviderRequest, ProviderRuntime, StreamPart,
        ToolResult,
    };
    use serde_json::json;

    #[derive(Clone)]
    struct MockRuntime {
        model: ProviderModel,
        continuation: std::sync::Arc<std::sync::Mutex<Option<ProviderRequest>>>,
        resumed: std::sync::Arc<std::sync::Mutex<bool>>,
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
            request: ProviderRequest,
        ) -> anyhow::Result<std::vec::IntoIter<anyhow::Result<StreamPart>>> {
            if request.prompt == "continue" {
                *self.resumed.lock().unwrap() = true;
                return Ok(vec![
                    Ok(StreamPart::TextDelta(crate::provider::TextPart {
                        id: "part-0".into(),
                        delta: "done".into(),
                    })),
                    Ok(StreamPart::Finish {
                        reason: FinishReason::EndTurn,
                    }),
                ]
                .into_iter());
            }

            Ok(vec![
                Ok(StreamPart::ToolCall(crate::provider::ToolCallPart {
                    id: "toolu_1".into(),
                    tool_call_id: "toolu_1".into(),
                    tool_name: "Read".into(),
                    input: json!({"file_path": "/tmp/a"}),
                })),
                Ok(StreamPart::Finish {
                    reason: FinishReason::ToolCall,
                }),
            ]
            .into_iter())
        }

        fn submit_tool_result(
            &self,
            _result: ToolResult,
        ) -> anyhow::Result<Option<ProviderRequest>> {
            Ok(self.continuation.lock().unwrap().take())
        }
    }

    #[test]
    fn service_starts_and_resumes_bridge_flow() {
        let model = ProviderModel::claude("sonnet", "Claude Sonnet");
        let runtime = MockRuntime {
            model: model.clone(),
            continuation: std::sync::Arc::new(std::sync::Mutex::new(Some(ProviderRequest {
                model: model.clone(),
                system_prompt: None,
                prompt: "continue".into(),
                messages: vec![],
            }))),
            resumed: std::sync::Arc::new(std::sync::Mutex::new(false)),
        };
        let resumed = runtime.resumed.clone();
        let bridge = OpenCodeBridge::new(runtime, vec![model]);
        let mut service = OpenClaudeService::new(bridge);

        let first = service
            .start(ServerRequest {
                conversation: BridgeRequest {
                    model_id: "sonnet".into(),
                    system_prompt: None,
                    prompt: "hello".into(),
                    messages: vec![BridgeMessage {
                        role: BridgeRole::User,
                        content: "earlier".into(),
                    }],
                },
            })
            .unwrap();

        assert!(matches!(
            first.step.state,
            AdapterSessionState::WaitingForTool(_)
        ));
        assert!(
            first
                .step
                .events
                .iter()
                .any(|event| matches!(event, AdapterEvent::ToolCall(_)))
        );
        let session_id = first.session_id.clone().unwrap();

        let second = service
            .resume(ServerContinueRequest {
                session_id: session_id.clone(),
                tool_result: BridgeToolResult {
                    call_id: "toolu_1".into(),
                    tool_name: Some("Read".into()),
                    output: json!({"content": "file"}),
                },
            })
            .unwrap();

        assert_eq!(second.step.state, AdapterSessionState::Finished);
        assert_eq!(second.session_id.as_deref(), Some(session_id.as_str()));
        assert!(*resumed.lock().unwrap());
    }

    #[test]
    fn service_describe_reports_provider_and_models() {
        let model = ProviderModel::claude("sonnet", "Claude Sonnet");
        let runtime = MockRuntime {
            model: model.clone(),
            continuation: std::sync::Arc::new(std::sync::Mutex::new(None)),
            resumed: std::sync::Arc::new(std::sync::Mutex::new(false)),
        };
        let bridge = OpenCodeBridge::new(runtime, vec![model]);
        let service = OpenClaudeService::new(bridge);

        let response = service.describe();
        let metadata = response.metadata.unwrap();

        assert_eq!(metadata.provider_id, "mock");
        assert_eq!(metadata.models.len(), 1);
        assert_eq!(metadata.models[0].id, "sonnet");
        assert_eq!(response.session_id, None);
        assert_eq!(response.step.state, AdapterSessionState::Ready);
    }

    #[test]
    fn service_rejects_unknown_session_on_resume() {
        let model = ProviderModel::claude("sonnet", "Claude Sonnet");
        let runtime = MockRuntime {
            model: model.clone(),
            continuation: std::sync::Arc::new(std::sync::Mutex::new(None)),
            resumed: std::sync::Arc::new(std::sync::Mutex::new(false)),
        };
        let bridge = OpenCodeBridge::new(runtime, vec![model]);
        let mut service = OpenClaudeService::new(bridge);

        let err = service
            .resume(ServerContinueRequest {
                session_id: "missing".into(),
                tool_result: BridgeToolResult {
                    call_id: "toolu_1".into(),
                    tool_name: Some("Read".into()),
                    output: json!({"content": "file"}),
                },
            })
            .unwrap_err();

        assert!(err.to_string().contains("unknown session id"));
    }
}
