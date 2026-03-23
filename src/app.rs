use crate::claude::ClaudeCliRuntime;
use crate::cli::{Cli, Command};
use crate::config::RuntimeConfig;
use crate::integration::OpenCodeBridge;
use crate::provider::default_models;
use crate::reference::refresh_reference;
use crate::server::{OpenClaudeService, serve_stdio};
use tracing::info;

pub fn run(cli: Cli) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "openclaude=info".into()),
        )
        .with_ansi(false)
        .init();

    if let Some(Command::Reference { project_root }) = &cli.command {
        let result = refresh_reference(project_root)?;
        info!(
            project_root = %project_root.display(),
            reference_path = %result.path.display(),
            repo_url = %result.repo_url,
            status = ?result.status,
            "refreshed optional opencode code reference checkout"
        );
        return Ok(());
    }

    let config = RuntimeConfig::from_cli(&cli);
    let models = default_models();
    let runtime = ClaudeCliRuntime::new(config.claude_bin.clone(), models.clone());
    let bridge = OpenCodeBridge::new(runtime, models);
    let mut service = OpenClaudeService::new(bridge);

    info!(
        model = %config.default_model,
        provider_id = %config.provider_id,
        runtime_models = 3,
        integration_mode = "standalone_bridge",
        "openclaude initialized"
    );

    serve_stdio(&mut service, std::io::stdin(), std::io::stdout())
}
