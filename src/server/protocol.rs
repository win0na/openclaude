use crate::integration::{AdapterStep, BridgeRequest, BridgeToolResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerRequest {
    pub conversation: BridgeRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerContinueRequest {
    pub tool_result: BridgeToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerResponse {
    pub step: AdapterStep,
}
