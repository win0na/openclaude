use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderModel {
    pub id: String,
    pub display_name: String,
    pub capabilities: ModelCapability,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_constructor() {
        let model = ProviderModel::claude("sonnet", "Claude Sonnet");
        assert_eq!(model.id, "sonnet");
        assert_eq!(model.display_name, "Claude Sonnet");
        assert!(model.capabilities.reasoning);
        assert!(model.capabilities.tool_calls);
        assert!(model.capabilities.interleaved_reasoning);
    }
}
