use crate::integration::OpenCodeBridge;
use crate::provider::ProviderRuntime;
use crate::server::{ServerMetadata, ServerModel, ServerRequest, ServerResponse};

pub struct OpenClaudeService<R: ProviderRuntime + Clone> {
    template: OpenCodeBridge<R>,
}

impl<R: ProviderRuntime + Clone> OpenClaudeService<R> {
    pub fn new(bridge: OpenCodeBridge<R>) -> Self {
        Self { template: bridge }
    }

    pub fn describe(&self) -> ServerResponse {
        ServerResponse {
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

    pub fn complete(&mut self, request: ServerRequest) -> anyhow::Result<ServerResponse> {
        let mut bridge = self.template.clone();
        let response_step = bridge.start(request.conversation)?;

        Ok(ServerResponse {
            metadata: None,
            step: response_step,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integration::{
        AdapterEvent, AdapterSessionState, BridgeMessage, BridgeMessagePart, BridgeRequest,
        BridgeRole,
    };
    use crate::provider::{
        FinishReason, ProviderInfo, ProviderModel, ProviderRequest, ProviderRuntime, StreamPart,
    };

    #[derive(Clone)]
    struct MockRuntime {
        model: ProviderModel,
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
            Ok(vec![
                Ok(StreamPart::ToolCall(crate::provider::ToolCallPart {
                    id: "toolu_1".into(),
                    tool_call_id: "toolu_1".into(),
                    tool_name: "Read".into(),
                    input: serde_json::json!({"file_path": "/tmp/a"}),
                })),
                Ok(StreamPart::Finish {
                    reason: FinishReason::ToolCall,
                }),
            ]
            .into_iter())
        }
    }

    #[test]
    fn service_completes_one_request_from_full_history() {
        let model = ProviderModel::claude("sonnet", "Claude Sonnet");
        let runtime = MockRuntime {
            model: model.clone(),
        };
        let bridge = OpenCodeBridge::new(runtime, vec![model]);
        let mut service = OpenClaudeService::new(bridge);

        let first = service
            .complete(ServerRequest {
                conversation: BridgeRequest {
                    model_id: "sonnet".into(),
                    system_prompt: None,
                    messages: vec![BridgeMessage {
                        role: BridgeRole::User,
                        parts: vec![BridgeMessagePart::Text {
                            text: "earlier\n\nhello".into(),
                        }],
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
    }

    #[test]
    fn service_describe_reports_provider_and_models() {
        let model = ProviderModel::claude("sonnet", "Claude Sonnet");
        let runtime = MockRuntime {
            model: model.clone(),
        };
        let bridge = OpenCodeBridge::new(runtime, vec![model]);
        let service = OpenClaudeService::new(bridge);

        let response = service.describe();
        let metadata = response.metadata.unwrap();

        assert_eq!(metadata.provider_id, "mock");
        assert_eq!(metadata.models.len(), 1);
        assert_eq!(metadata.models[0].id, "sonnet");
        assert_eq!(response.step.state, AdapterSessionState::Ready);
    }
}
