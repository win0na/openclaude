use crate::provider::ProviderModel;

pub fn default_models() -> Vec<ProviderModel> {
    vec![
        ProviderModel::claude("haiku", "Claude Haiku"),
        ProviderModel::claude("sonnet", "Claude Sonnet"),
        ProviderModel::claude("opus", "Claude Opus"),
    ]
}

pub fn default_model(model_id: &str) -> Option<ProviderModel> {
    default_models()
        .into_iter()
        .find(|model| model.id == model_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_models() {
        let ids = default_models()
            .into_iter()
            .map(|model| model.id)
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["haiku", "sonnet", "opus"]);
    }

    #[test]
    fn lookup_known() {
        let model = default_model("sonnet").unwrap();
        assert_eq!(model.display_name, "Claude Sonnet");
    }
}
