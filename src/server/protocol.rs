use crate::integration::{AdapterStep, BridgeRequest};
use crate::provider::ProviderModel;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerCommand {
    Describe {
        request_id: String,
    },
    Complete {
        request_id: String,
        request: ServerRequest,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerRequest {
    pub conversation: BridgeRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerResponse {
    pub metadata: Option<ServerMetadata>,
    pub step: AdapterStep,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerMetadata {
    pub provider_id: String,
    pub provider_name: String,
    pub models: Vec<ServerModel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerModel {
    pub id: String,
    pub display_name: String,
    pub reasoning: bool,
    pub tool_calls: bool,
    pub interleaved_reasoning: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerEnvelope {
    Success {
        request_id: String,
        response: ServerResponse,
    },
    Error {
        request_id: Option<String>,
        message: String,
    },
}

impl From<ProviderModel> for ServerModel {
    fn from(value: ProviderModel) -> Self {
        Self {
            id: value.id,
            display_name: value.display_name,
            reasoning: value.capabilities.reasoning,
            tool_calls: value.capabilities.tool_calls,
            interleaved_reasoning: value.capabilities.interleaved_reasoning,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integration::{
        AdapterSessionState, AdapterStep, BridgeMessage, BridgeMessagePart, BridgeRequest,
        BridgeRole,
    };

    #[test]
    fn command_roundtrip() {
        let command = ServerCommand::Complete {
            request_id: "req-1".into(),
            request: ServerRequest {
                conversation: BridgeRequest {
                    model_id: "sonnet".into(),
                    system_prompt: Some("system".into()),
                    messages: vec![BridgeMessage {
                        role: BridgeRole::User,
                        parts: vec![BridgeMessagePart::Text {
                            text: "hello".into(),
                        }],
                    }],
                },
            },
        };

        let json = serde_json::to_string(&command).unwrap();
        let parsed: ServerCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, command);
    }

    #[test]
    fn envelope_error_serializes_request_id() {
        let envelope = ServerEnvelope::Error {
            request_id: Some("req-2".into()),
            message: "boom".into(),
        };

        let json = serde_json::to_value(&envelope).unwrap();
        assert_eq!(json["kind"], "error");
        assert_eq!(json["request_id"], "req-2");
        assert_eq!(json["message"], "boom");
    }

    #[test]
    fn success_envelope_roundtrip() {
        let envelope = ServerEnvelope::Success {
            request_id: "req-3".into(),
            response: ServerResponse {
                metadata: None,
                step: AdapterStep {
                    events: Vec::new(),
                    state: AdapterSessionState::Ready,
                },
            },
        };

        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: ServerEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, envelope);
    }
}
