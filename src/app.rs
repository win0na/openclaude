use crate::claude::{ClaudeCli, ClaudeCliRuntime};
use crate::bootstrap::{launch_opencode, launch_opencode_with_server};
use crate::benchmark;
use crate::cli::{BenchmarkCommand, Cli, Command};
use crate::console;
use crate::config::RuntimeConfig;
use crate::integration::OpenCodeBridge;
use crate::reference::refresh_reference;
use crate::server::{OpenClaudeService, create_router, serve_stdio};
use std::io::{self, Write};
use std::net::SocketAddr;
use tracing::{info, warn};

pub fn run(cli: Cli) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "openclaude=info".into()),
        )
        .with_ansi(console::stderr_color_enabled())
        .init();

    match &cli.command {
        None => launch_opencode_with_server(&cli, &[]),
        Some(Command::External(args)) => launch_opencode_with_server(&cli, args),
        Some(Command::Help) => {
            let mut stdout = io::stdout().lock();
            let help = crate::cli::detailed_help();
            stdout.write_all(help.as_bytes())?;
            stdout.write_all(b"\n")?;
            Ok(())
        }
        Some(Command::Benchmark(cmd)) => {
            if matches!(cmd.command, Some(BenchmarkCommand::Help)) {
                let mut stdout = io::stdout().lock();
                let help = crate::cli::benchmark_help();
                stdout.write_all(help.as_bytes())?;
                stdout.write_all(b"\n")?;
                Ok(())
            } else {
                benchmark::run(&cli, &cmd.options)
            }
        }
        Some(Command::BenchmarkClaudeWorker) => benchmark::run_claude_worker(&cli),
        Some(Command::Bootstrap { args }) => launch_opencode(&cli, args),
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
            let discovery = ClaudeCli::new(&config.claude_bin)
                .discover_available_models_report(&config.available_models);
            if let Some(message) = discovery.warning.as_deref() {
                warn!(claude_bin = %config.claude_bin.display(), "{message}");
            }
            let models = discovery.models;
            let runtime_models = models.len();
            let runtime = ClaudeCliRuntime::new(config.claude_bin.clone(), models.clone());
            let bridge = OpenCodeBridge::new(runtime, models);

            info!(
                runtime_models,
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
            let discovery = ClaudeCli::new(&config.claude_bin)
                .discover_available_models_report(&config.available_models);
            if let Some(message) = discovery.warning.as_deref() {
                warn!(claude_bin = %config.claude_bin.display(), "{message}");
            }
            let models = discovery.models;
            let runtime_models = models.len();
            let runtime = ClaudeCliRuntime::new(config.claude_bin.clone(), models.clone());
            let bridge = OpenCodeBridge::new(runtime, models);
            let mut service = OpenClaudeService::new(bridge);

            info!(
                runtime_models,
                integration_mode = "standalone_bridge",
                "openclaude initialized"
            );

            serve_stdio(&mut service, std::io::stdin(), std::io::stdout())
        }
    }
}
