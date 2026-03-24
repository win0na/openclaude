use crate::console;
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Clone, Parser)]
#[command(name = "openclaude", disable_help_subcommand = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(long, default_value = "openclaude")]
    pub provider_id: String,

    #[arg(long, default_value = "sonnet")]
    pub default_model: String,

    #[arg(long, value_delimiter = ',')]
    pub available_models: Vec<String>,

    #[arg(long, default_value = "claude")]
    pub claude_bin: PathBuf,

    #[arg(long, default_value = "opencode")]
    pub opencode_bin: PathBuf,

    #[arg(long, default_value = "http://127.0.0.1:43123/v1")]
    pub base_url: String,

    #[arg(long, default_value = "/tmp/openclaude")]
    pub workdir: PathBuf,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    Help,
    Benchmark(Benchmark),
    #[command(hide = true)]
    BenchmarkClaudeWorker,
    Bootstrap {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    Reference {
        #[arg(long, default_value = ".")]
        project_root: PathBuf,
    },
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value = "43123")]
        port: u16,
    },
    Stdio,
    #[command(external_subcommand)]
    External(Vec<OsString>),
}

#[derive(Debug, Clone, Args)]
pub struct Benchmark {
    #[command(subcommand)]
    pub command: Option<BenchmarkCommand>,

    #[command(flatten)]
    pub options: BenchmarkOptions,
}

#[derive(Debug, Clone, Subcommand)]
pub enum BenchmarkCommand {
    Help,
}

#[derive(Debug, Clone, Args)]
pub struct BenchmarkOptions {
    #[arg(long, value_enum, default_value_t = BenchmarkMode::Translation)]
    pub mode: BenchmarkMode,

    #[arg(long, default_value = "sonnet")]
    pub model: String,

    #[arg(long, default_value = "Reply with exactly: latency benchmark")]
    pub prompt: String,

    #[arg(long, default_value_t = 10)]
    pub iterations: usize,

    #[arg(long, default_value_t = 1)]
    pub warmups: usize,

    #[arg(long, default_value_t = false)]
    pub skip_live: bool,

    #[arg(long)]
    pub max_first_ms: Option<f64>,

    #[arg(long)]
    pub max_total_ms: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BenchmarkMode {
    All,
    Translation,
    OpencodeSession,
}

fn command_line(style: console::Style, name: &str, description: &str) -> String {
    inline_help_line(style.command(name), name, description, 2)
}

fn option_line(style: console::Style, name: &str, description: &str) -> String {
    inline_help_line(style.option(name), name, description, 2)
}

fn inline_help_line(
    styled_name: String,
    plain_name: &str,
    description: &str,
    indent: usize,
) -> String {
    const DESCRIPTION_COLUMN: usize = 44;
    let padding = DESCRIPTION_COLUMN
        .saturating_sub(indent + plain_name.len())
        .max(1);

    format!(
        "{}{styled_name}{}{description}",
        " ".repeat(indent),
        " ".repeat(padding)
    )
}

fn help_block(styled_name: String, description: &str) -> [String; 2] {
    [format!("  {styled_name}"), format!("    {description}")]
}

fn detail_line(style: console::Style, name: &str, description: &str) -> String {
    inline_help_line(style.option(name), name, description, 4)
}

fn render_detailed_help(style: console::Style) -> String {
    let mut lines = vec![
        style.title("openclaude"),
        String::new(),
        style.heading("usage:"),
        format!("  {}", style.command("openclaude [OPTIONS] [COMMAND]")),
        String::new(),
        style.heading("example:"),
    ];
    lines.extend(help_block(
        style.command("openclaude"),
        "default: start the server and launch opencode",
    ));
    lines.extend(help_block(
        style.command("openclaude bootstrap"),
        "launch opencode with provider bootstrap only",
    ));
    lines.extend(help_block(
        style.command("openclaude serve"),
        "start the HTTP backend server",
    ));
    lines.extend([
        String::new(),
        style.heading("options:"),
        option_line(
            style,
            "--provider-id <PROVIDER_ID>",
            "[default: openclaude]",
        ),
        option_line(
            style,
            "--default-model <DEFAULT_MODEL>",
            "[default: sonnet]",
        ),
        option_line(
            style,
            "--available-models <AVAILABLE_MODELS>",
            "[default: none] [comma-separated override]",
        ),
        option_line(style, "--claude-bin <CLAUDE_BIN>", "[default: claude]"),
        option_line(
            style,
            "--opencode-bin <OPENCODE_BIN>",
            "[default: opencode]",
        ),
        option_line(
            style,
            "--base-url <BASE_URL>",
            "[default: http://127.0.0.1:43123/v1]",
        ),
        option_line(style, "--workdir <WORKDIR>", "[default: /tmp/openclaude]"),
        option_line(style, "-h, --help", "print help"),
        String::new(),
        style.heading("commands:"),
        command_line(style, "help", "print the detailed help page"),
        command_line(style, "benchmark", "run the live latency benchmark"),
        command_line(
            style,
            "bootstrap",
            "launch opencode without starting the server",
        ),
        command_line(
            style,
            "reference",
            "refresh the optional local opencode checkout",
        ),
        command_line(style, "serve", "start the HTTP backend server"),
        command_line(style, "stdio", "start the STDIO bridge explicitly"),
        String::new(),
        style.heading("subcommand usage:"),
    ]);
    lines.extend(help_block(
        style.command("openclaude bootstrap [COMMAND]"),
        "launch opencode with provider bootstrap only",
    ));
    lines.extend(help_block(
        style.command("openclaude reference [--project-root <PROJECT_ROOT>]"),
        "refresh the optional local opencode checkout",
    ));
    lines.extend(help_block(
        style.command("openclaude serve [--host <HOST>] [--port <PORT>]"),
        "start the HTTP backend server",
    ));
    lines.join("\n")
}

pub fn detailed_help() -> String {
    render_detailed_help(console::stdout_style())
}

fn render_benchmark_help(style: console::Style) -> String {
    let lines = vec![
        style.title("openclaude benchmark"),
        String::new(),
        style.heading("usage:"),
        format!("  {}", style.command("openclaude benchmark [OPTIONS]")),
        String::new(),
        style.heading("commands:"),
        command_line(style, "help", "print benchmark help"),
        String::new(),
        style.heading("options:"),
        detail_line(
            style,
            "--mode <MODE>",
            "benchmark mode [possible values: all, translation, opencode-session]",
        ),
        detail_line(style, "--model <MODEL>", "benchmark model id"),
        detail_line(style, "--prompt <PROMPT>", "benchmark prompt"),
        detail_line(style, "--iterations <ITERATIONS>", "sample count"),
        detail_line(style, "--warmups <WARMUPS>", "warmup runs"),
        detail_line(
            style,
            "--skip-live",
            "skip instead of failing when live Claude access is unavailable",
        ),
        detail_line(
            style,
            "--max-first-ms <MAX_FIRST_MS>",
            "max allowed first-token overhead",
        ),
        detail_line(
            style,
            "--max-total-ms <MAX_TOTAL_MS>",
            "max allowed total overhead",
        ),
    ];
    lines.join("\n")
}

pub fn benchmark_help() -> String {
    render_benchmark_help(console::stdout_style())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_commands() {
        let help = render_detailed_help(console::Style::plain());

        assert!(help.contains("usage:"));
        assert!(help.contains("example:"));
        assert!(help.contains("openclaude"));
        assert!(help.contains("openclaude bootstrap"));
        assert!(help.contains("benchmark"));
        assert!(help.contains("default: start the server and launch opencode"));
        assert!(help.contains("launch opencode with provider bootstrap only"));
        assert!(help.contains("run the live latency benchmark"));
        assert!(help.contains("openclaude serve"));
        assert!(help.contains("start the HTTP backend server"));
        assert!(help.contains("subcommand usage:"));
        assert!(help.contains("openclaude bootstrap [COMMAND]"));
        assert!(help.contains("openclaude reference [--project-root <PROJECT_ROOT>]"));
        assert!(help.contains("openclaude serve [--host <HOST>] [--port <PORT>]"));
        assert!(!help.contains("benchmark:"));
        assert!(!help.contains("openclaude benchmark [SUBCOMMAND]"));
        assert!(help.contains("launch opencode without starting the server"));
        assert!(help.contains("stdio"));
        assert!(help.contains("start the STDIO bridge explicitly"));
        assert!(!help.contains("quick guide"));
    }

    #[test]
    fn help_benchmark() {
        let help = render_benchmark_help(console::Style::plain());

        assert!(help.contains("openclaude benchmark"));
        assert!(help.contains("openclaude benchmark [OPTIONS]"));
        assert!(help.contains("print benchmark help"));
        assert!(help.contains("--mode <MODE>"));
        assert!(help.contains("possible values: all, translation, opencode-session"));
        assert!(help.contains("--model <MODEL>"));
        assert!(help.contains("benchmark model id"));
        assert!(help.contains("--prompt <PROMPT>"));
        assert!(help.contains("benchmark prompt"));
        assert!(help.contains("--iterations <ITERATIONS>"));
        assert!(help.contains("--warmups <WARMUPS>"));
        assert!(help.contains("--skip-live"));
        assert!(help.contains("--max-first-ms <MAX_FIRST_MS>"));
        assert!(help.contains("--max-total-ms <MAX_TOTAL_MS>"));
    }

    #[test]
    fn help_ansi() {
        let help = render_detailed_help(console::Style::color());

        assert!(help.contains("\x1b["));
    }

    #[test]
    fn parses_help() {
        let cli = Cli::try_parse_from(["openclaude", "help"]).expect("parse help");

        assert!(matches!(cli.command, Some(Command::Help)));
    }

    #[test]
    fn parses_bootstrap() {
        let cli = Cli::try_parse_from(["openclaude", "bootstrap", "run", "hello"])
            .expect("parse bootstrap");

        match cli.command {
            Some(Command::Bootstrap { args }) => {
                assert_eq!(args, vec![OsString::from("run"), OsString::from("hello")]);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_benchmark() {
        let cli = Cli::try_parse_from(["openclaude", "benchmark", "--iterations", "3"])
            .expect("parse benchmark");

        match cli.command {
            Some(Command::Benchmark(cmd)) => {
                assert!(cmd.command.is_none());
                assert!(matches!(cmd.options.mode, BenchmarkMode::Translation));
                assert_eq!(cmd.options.iterations, 3);
                assert_eq!(cmd.options.model, "sonnet");
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_benchmark_help() {
        let cli =
            Cli::try_parse_from(["openclaude", "benchmark", "help"]).expect("parse benchmark help");

        match cli.command {
            Some(Command::Benchmark(cmd)) => {
                assert!(matches!(cmd.command, Some(BenchmarkCommand::Help)));
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn captures_external() {
        let cli = Cli::try_parse_from(["openclaude", "run", "hello"]).expect("parse passthrough");

        match cli.command {
            Some(Command::External(args)) => {
                assert_eq!(args, vec![OsString::from("run"), OsString::from("hello")]);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
