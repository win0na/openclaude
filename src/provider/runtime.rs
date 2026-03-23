use crate::provider::model::ProviderModel;
use crate::provider::stream::StreamPart;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderRequest {
    pub model: ProviderModel,
    pub system_prompt: Option<String>,
    pub prompt: String,
    pub messages: Vec<ProviderMessage>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderMessage {
    pub role: MessageRole,
    pub parts: Vec<MessagePart>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessagePart {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

pub trait ProviderRuntime {
    fn info(&self) -> ProviderInfo;
    fn models(&self) -> &[ProviderModel];
    fn stream(
        &self,
        request: ProviderRequest,
    ) -> anyhow::Result<std::vec::IntoIter<anyhow::Result<StreamPart>>>;
}
