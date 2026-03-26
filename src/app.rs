use crate::alias;
use crate::benchmark;
use crate::bootstrap::{launch_opencode, launch_opencode_with_server};
use crate::claude::{ClaudeCli, ClaudeCliRuntime};
use crate::cli::{BenchmarkCommand, Cli, Command};
use crate::config::RuntimeConfig;
use crate::console;
use crate::integration::OpenCodeBridge;
use crate::reference::refresh_reference;
use crate::server::{ClydeService, create_router, serve_stdio};
use std::io::{self, Write};
use std::net::SocketAddr;
use tracing::{info, warn};

pub fn run(cli: Cli) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "clyde=info".into()),
        )
        .with_ansi(console::stderr_color_enabled())
        .init();

    if let Some(raw) = cli.opencode_arguments.as_deref() {
        let args = parse_opencode_arguments(raw)?;
        return launch_opencode_with_server(&cli, &args);
    }

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
        Some(Command::Alias) => {
            let mut stdout = io::stdout().lock();
            let install = alias::install()?;
            writeln!(
                stdout,
                "installed clyde alias for {} in {}",
                install.shell,
                install.rc_path.display()
            )?;
            writeln!(
                stdout,
                "restart your shell or run: source {}",
                install.rc_path.display()
            )?;
            Ok(())
        }
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
                "clyde initialized"
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
            let mut service = ClydeService::new(bridge);

            info!(
                runtime_models,
                integration_mode = "standalone_bridge",
                "clyde initialized"
            );

            serve_stdio(&mut service, std::io::stdin(), std::io::stdout())
        }
    }
}

fn parse_opencode_arguments(raw: &str) -> anyhow::Result<Vec<std::ffi::OsString>> {
    if raw.is_empty() {
        return Ok(Vec::new());
    }

    let split = shlex::split(raw).ok_or_else(|| {
        anyhow::anyhow!("failed to parse --opencode-arguments as shell arguments")
    })?;
    Ok(split.into_iter().map(std::ffi::OsString::from).collect())
}

#[cfg(test)]
mod tests {
    use super::parse_opencode_arguments;
    use std::ffi::OsString;

    #[test]
    fn parses_shell_style_arguments() {
        let args = parse_opencode_arguments("run --model 'clyde/sonnet' \"hello world\"").unwrap();

        assert_eq!(
            args,
            vec![
                OsString::from("run"),
                OsString::from("--model"),
                OsString::from("clyde/sonnet"),
                OsString::from("hello world"),
            ]
        );
    }

    #[test]
    fn rejects_invalid_shell_arguments() {
        let err = parse_opencode_arguments("run 'unterminated")
            .unwrap_err()
            .to_string();
        assert!(err.contains("failed to parse --opencode-arguments"));
    }
}
