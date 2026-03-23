use crate::claude::ClaudeCliRuntime;
use crate::cli::{Cli, Command};
use crate::console;
use crate::config::RuntimeConfig;
use crate::integration::OpenCodeBridge;
use crate::provider::default_models;
use crate::reference::refresh_reference;
use crate::server::{OpenClaudeService, create_router, serve_stdio};
use std::io::{self, Write};
use std::net::SocketAddr;
use tracing::info;

pub fn run(cli: Cli) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "openclaude=info".into()),
        )
        .with_ansi(console::stderr_color_enabled())
        .init();

    match &cli.command {
        Some(Command::Help) | None => {
            let mut stdout = io::stdout().lock();
            let help = crate::cli::detailed_help();
            stdout.write_all(help.as_bytes())?;
            stdout.write_all(b"\n")?;
            Ok(())
        }
        Some(Command::Reference { project_root }) => {
            let result = refresh_reference(project_root)?;
            info!(
                project_root = %project_root.display(),
                reference_path = %result.path.display(),
                repo_url = %result.repo_url,
                status = ?result.status,
                "refreshed optional opencode code reference checkout"
            );
            Ok(())
        }
        Some(Command::Serve { host, port }) => {
            let config = RuntimeConfig::from_cli(&cli);
            let models = default_models();
            let runtime = ClaudeCliRuntime::new(config.claude_bin.clone(), models.clone());
            let bridge = OpenCodeBridge::new(runtime, models);

            info!(
                model = %config.default_model,
                provider_id = %config.provider_id,
                runtime_models = 3,
                integration_mode = "http_server",
                "openclaude initialized"
            );

            let router = create_router(bridge);
            let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
            info!(addr = %addr, "starting HTTP server");

            tokio::runtime::Runtime::new()?.block_on(async {
                let listener = tokio::net::TcpListener::bind(addr).await?;
                let local_addr = listener.local_addr()?;

                info!(
                    addr = %local_addr,
                    health = %format!("http://{local_addr}/health"),
                    completions = %format!("http://{local_addr}/v1/chat/completions"),
                    "HTTP server ready"
                );

                axum::serve(listener, router).await
            })?;
            Ok(())
        }
        Some(Command::Stdio) => {
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
    }
}
