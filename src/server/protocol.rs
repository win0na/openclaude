use crate::integration::{AdapterStep, BridgeRequest, BridgeToolResult};
use crate::provider::ProviderModel;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerCommand {
    Describe {
        request_id: String,
    },
    Start {
        request_id: String,
        request: ServerRequest,
    },
    Resume {
        request_id: String,
        request: ServerContinueRequest,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerRequest {
    pub conversation: BridgeRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerContinueRequest {
    pub session_id: String,
    pub tool_result: BridgeToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerResponse {
    pub session_id: Option<String>,
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
