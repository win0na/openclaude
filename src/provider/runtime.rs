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
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    pub call_id: String,
    pub output: Value,
}

pub trait ProviderRuntime {
    fn info(&self) -> ProviderInfo;
    fn models(&self) -> &[ProviderModel];
    fn stream(
        &self,
        request: ProviderRequest,
    ) -> anyhow::Result<std::vec::IntoIter<anyhow::Result<StreamPart>>>;
    fn submit_tool_result(&self, _result: ToolResult) -> anyhow::Result<()> {
        Ok(())
    }
}
