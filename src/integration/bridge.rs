use crate::integration::{AdapterStep, OpenCodeAdapter};
use crate::provider::{
    MessagePart, MessageRole, ProviderInfo, ProviderMessage, ProviderModel, ProviderRequest,
    ProviderRuntime,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeMessage {
    pub role: BridgeRole,
    pub parts: Vec<BridgeMessagePart>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BridgeMessagePart {
    Text {
        text: String,
    },
    ToolCall {
        call_id: String,
        tool_name: String,
        input: Value,
    },
    ToolResult {
        call_id: String,
        tool_name: Option<String>,
        output: Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BridgeRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeRequest {
    pub model_id: String,
    pub system_prompt: Option<String>,
    pub prompt: String,
    pub messages: Vec<BridgeMessage>,
}

#[derive(Clone)]
pub struct OpenCodeBridge<R: ProviderRuntime> {
    adapter: OpenCodeAdapter<R>,
    provider: ProviderInfo,
    models: BTreeMap<String, ProviderModel>,
}

impl<R: ProviderRuntime> OpenCodeBridge<R> {
    pub fn new(runtime: R, models: impl IntoIterator<Item = ProviderModel>) -> Self {
        let provider = runtime.info();
        let models = models
            .into_iter()
            .map(|model| (model.id.clone(), model))
            .collect();

        Self {
            adapter: OpenCodeAdapter::new(runtime),
            provider,
            models,
        }
    }

    pub fn provider_info(&self) -> &ProviderInfo {
        &self.provider
    }

    pub fn models(&self) -> Vec<ProviderModel> {
        self.models.values().cloned().collect()
    }

    pub fn start(&mut self, request: BridgeRequest) -> anyhow::Result<AdapterStep> {
        self.adapter.start(self.to_provider_request(request)?)
    }

    fn to_provider_request(&self, request: BridgeRequest) -> anyhow::Result<ProviderRequest> {
        let model = self
            .models
            .get(&request.model_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown model id: {}", request.model_id))?;

        Ok(ProviderRequest {
            model,
            system_prompt: request.system_prompt,
            prompt: request.prompt,
            messages: request
                .messages
                .into_iter()
                .map(|message| ProviderMessage {
                    role: match message.role {
                        BridgeRole::System => MessageRole::System,
                        BridgeRole::User => MessageRole::User,
                        BridgeRole::Assistant => MessageRole::Assistant,
                        BridgeRole::Tool => MessageRole::Tool,
                    },
                    parts: message
                        .parts
                        .into_iter()
                        .map(|part| match part {
                            BridgeMessagePart::Text { text } => MessagePart::Text { text },
                            BridgeMessagePart::ToolCall {
                                call_id,
                                tool_name,
                                input,
                            } => MessagePart::ToolCall {
                                call_id,
                                tool_name,
                                input,
                            },
                            BridgeMessagePart::ToolResult {
                                call_id,
                                tool_name,
                                output,
                            } => MessagePart::ToolResult {
                                call_id,
                                tool_name,
                                output,
                            },
                        })
                        .collect(),
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integration::{AdapterEvent, AdapterSessionState};
    use crate::provider::{ProviderInfo, ProviderRuntime, StreamPart};

    #[derive(Clone)]
    struct EmptyRuntime {
        models: Vec<ProviderModel>,
    }

    impl ProviderRuntime for EmptyRuntime {
        fn info(&self) -> ProviderInfo {
            ProviderInfo {
                id: "mock".into(),
                name: "Mock".into(),
            }
        }

        fn models(&self) -> &[ProviderModel] {
            &self.models
        }

        fn stream(
            &self,
            _request: ProviderRequest,
        ) -> anyhow::Result<std::vec::IntoIter<anyhow::Result<StreamPart>>> {
            Ok(vec![
                Ok(StreamPart::Start),
                Ok(StreamPart::Finish {
                    reason: crate::provider::FinishReason::EndTurn,
                }),
            ]
            .into_iter())
        }
    }

    #[test]
    fn bridge_maps_wire_request_to_adapter_step() {
        let model = ProviderModel::claude("sonnet", "Claude Sonnet");
        let runtime = EmptyRuntime {
            models: vec![model.clone()],
        };
        let mut bridge = OpenCodeBridge::new(runtime, vec![model]);

        let step = bridge
            .start(BridgeRequest {
                model_id: "sonnet".into(),
                system_prompt: Some("system".into()),
                prompt: "hello".into(),
                messages: vec![BridgeMessage {
                    role: BridgeRole::User,
                    parts: vec![BridgeMessagePart::Text {
                        text: "earlier".into(),
                    }],
                }],
            })
            .unwrap();

        assert_eq!(step.state, AdapterSessionState::Finished);
        assert!(
            step.events
                .iter()
                .any(|event| matches!(event, AdapterEvent::Start))
        );
    }
}
