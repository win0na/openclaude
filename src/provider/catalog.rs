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
    fn default_catalog_contains_expected_models() {
        let ids = default_models()
            .into_iter()
            .map(|model| model.id)
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["haiku", "sonnet", "opus"]);
    }

    #[test]
    fn default_model_lookup_finds_known_model() {
        let model = default_model("sonnet").unwrap();
        assert_eq!(model.display_name, "Claude Sonnet");
    }
}
