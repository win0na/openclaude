use crate::cli::Cli;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub available_models: Vec<String>,
    pub claude_bin: PathBuf,
}

impl RuntimeConfig {
    pub fn from_cli(cli: &Cli) -> Self {
        Self {
            available_models: cli.available_models.clone(),
            claude_bin: cli.claude_bin.clone(),
        }
    }
}
