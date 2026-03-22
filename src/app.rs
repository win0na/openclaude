use crate::claude::ClaudeCliRuntime;
use crate::cli::Cli;
use crate::config::RuntimeConfig;
use crate::provider::{ProviderModel, ProviderRuntime};
use tracing::info;

pub fn run(cli: Cli) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "openclaude=info".into()),
        )
        .with_ansi(false)
        .init();

    let config = RuntimeConfig::from_cli(cli);
    let runtime = ClaudeCliRuntime::new(
        config.claude_bin.clone(),
        vec![
            ProviderModel::claude("haiku", "Claude Haiku"),
            ProviderModel::claude("sonnet", "Claude Sonnet"),
            ProviderModel::claude("opus", "Claude Opus"),
        ],
    );

    info!(
        model = %config.default_model,
        provider_id = %config.provider_id,
        runtime_models = runtime.models().len(),
        "openclaude initialized"
    );
    Ok(())
}
