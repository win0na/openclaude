use crate::cli::Cli;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub provider_id: String,
    pub default_model: String,
    pub claude_bin: PathBuf,
    pub workdir: PathBuf,
}

impl RuntimeConfig {
    pub fn from_cli(cli: Cli) -> Self {
        Self {
            provider_id: cli.provider_id,
            default_model: cli.default_model,
            claude_bin: cli.claude_bin,
            workdir: cli.workdir,
        }
    }
}
