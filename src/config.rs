use crate::cli::Cli;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub provider_id: String,
    pub default_model: String,
    pub available_models: Vec<String>,
    pub claude_bin: PathBuf,
    pub workdir: PathBuf,
}

impl RuntimeConfig {
    pub fn from_cli(cli: &Cli) -> Self {
        Self {
            provider_id: cli.provider_id.clone(),
            default_model: cli.default_model.clone(),
            available_models: cli.available_models.clone(),
            claude_bin: cli.claude_bin.clone(),
            workdir: cli.workdir.clone(),
        }
    }
}
