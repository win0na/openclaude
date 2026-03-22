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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    pub call_id: String,
    pub tool_name: Option<String>,
    pub output: Value,
}

pub trait ProviderRuntime {
    fn info(&self) -> ProviderInfo;
    fn models(&self) -> &[ProviderModel];
    fn stream(
        &self,
        request: ProviderRequest,
    ) -> anyhow::Result<std::vec::IntoIter<anyhow::Result<StreamPart>>>;
    fn submit_tool_result(&self, _result: ToolResult) -> anyhow::Result<Option<ProviderRequest>> {
        Ok(None)
    }
}
