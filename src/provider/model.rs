#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderModel {
    pub id: String,
    pub display_name: String,
    pub capabilities: ModelCapability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapability {
    pub reasoning: bool,
    pub tool_calls: bool,
    pub interleaved_reasoning: bool,
}

impl ProviderModel {
    pub fn claude(id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            capabilities: ModelCapability {
                reasoning: true,
                tool_calls: true,
                interleaved_reasoning: true,
            },
        }
    }
}
