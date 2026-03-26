use crate::console;
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Clone, Parser)]
#[command(
    name = "openclaude",
    disable_help_subcommand = true,
    disable_help_flag = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(
        short = 'c',
        long = "opencode-arguments",
        num_args = 0..=1,
        default_missing_value = "",
        allow_hyphen_values = true
    )]
    pub opencode_arguments: Option<String>,

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

    #[arg(long, default_value = "http://127.0.0.1:43123")]
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
    Alias,
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

struct HelpEntry {
    styled_name: String,
    plain_name: String,
    description: String,
    indent: usize,
}

fn command_entry(style: console::Style, name: &str, description: &str) -> HelpEntry {
    HelpEntry {
        styled_name: style.command(name),
        plain_name: name.to_string(),
        description: description.to_string(),
        indent: 2,
    }
}

fn option_entry(style: console::Style, name: &str, description: &str) -> HelpEntry {
    HelpEntry {
        styled_name: style.option(name),
        plain_name: name.to_string(),
        description: description.to_string(),
        indent: 2,
    }
}

fn detail_entry(style: console::Style, name: &str, description: &str) -> HelpEntry {
    HelpEntry {
        styled_name: style.option(name),
        plain_name: name.to_string(),
        description: description.to_string(),
        indent: 4,
    }
}

fn render_help_entries(entries: &[HelpEntry]) -> Vec<String> {
    const MIN_COLUMN_GAP: usize = 8;
    let description_column = entries
        .iter()
        .map(|entry| entry.indent + entry.plain_name.len() + MIN_COLUMN_GAP)
        .max()
        .unwrap_or(MIN_COLUMN_GAP);

    entries
        .iter()
        .map(|entry| {
            let padding = description_column
                .saturating_sub(entry.indent + entry.plain_name.len())
                .max(MIN_COLUMN_GAP);
            format!(
                "{}{name}{}{desc}",
                " ".repeat(entry.indent),
                " ".repeat(padding),
                name = entry.styled_name,
                desc = entry.description
            )
        })
        .collect()
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
    lines.extend(render_help_entries(&[
        command_entry(
            style,
            "openclaude",
            "default: start the server and launch opencode",
        ),
        command_entry(
            style,
            "openclaude -c [COMMAND]",
            "start the server and forward explicit opencode arguments",
        ),
    ]));
    lines.push(String::new());
    lines.push(style.heading("options:"));
    lines.extend(render_help_entries(&[
        option_entry(
            style,
            "-c, --opencode-arguments <OPENCODE_ARGUMENTS>",
            "forward remaining arguments to opencode",
        ),
        option_entry(
            style,
            "--provider-id <PROVIDER_ID>",
            "[default: openclaude]",
        ),
        option_entry(
            style,
            "--default-model <DEFAULT_MODEL>",
            "[default: sonnet]",
        ),
        option_entry(
            style,
            "--available-models <AVAILABLE_MODELS>",
            "[default: none] [comma-separated override]",
        ),
        option_entry(style, "--claude-bin <CLAUDE_BIN>", "[default: claude]"),
        option_entry(
            style,
            "--opencode-bin <OPENCODE_BIN>",
            "[default: opencode]",
        ),
        option_entry(
            style,
            "--base-url <BASE_URL>",
            "[default: http://127.0.0.1:43123]",
        ),
        option_entry(style, "--workdir <WORKDIR>", "[default: /tmp/openclaude]"),
        option_entry(style, "-h, --help", "print help"),
    ]));
    lines.push(String::new());
    lines.push(style.heading("commands:"));
    lines.extend(render_help_entries(&[
        command_entry(style, "help", "print the detailed help page"),
        command_entry(style, "benchmark", "run the live latency benchmark"),
        command_entry(
            style,
            "alias",
            "install a shell alias for opencode passthrough",
        ),
        command_entry(
            style,
            "bootstrap",
            "launch opencode without starting the server",
        ),
        command_entry(
            style,
            "reference",
            "refresh the optional local opencode checkout",
        ),
        command_entry(style, "serve", "start the HTTP backend server"),
        command_entry(style, "stdio", "start the STDIO bridge explicitly"),
    ]));
    lines.push(String::new());
    lines.push(style.heading("subcommand usage:"));
    lines.extend(render_help_entries(&[
        command_entry(
            style,
            "openclaude bootstrap [COMMAND]",
            "launch opencode with provider bootstrap only",
        ),
        command_entry(
            style,
            "openclaude reference [--project-root <PROJECT_ROOT>]",
            "refresh the optional local opencode checkout",
        ),
        command_entry(
            style,
            "openclaude serve [--host <HOST>] [--port <PORT>]",
            "start the HTTP backend server",
        ),
    ]));
    lines.join("\n")
}

pub fn detailed_help() -> String {
    render_detailed_help(console::stdout_style())
}

fn render_benchmark_help(style: console::Style) -> String {
    let mut lines = vec![
        style.title("openclaude benchmark"),
        String::new(),
        style.heading("usage:"),
        format!("  {}", style.command("openclaude benchmark [OPTIONS]")),
        String::new(),
        style.heading("commands:"),
    ];
    lines.extend(render_help_entries(&[command_entry(
        style,
        "help",
        "print benchmark help",
    )]));
    lines.push(String::new());
    lines.push(style.heading("options:"));
    lines.extend(render_help_entries(&[
        detail_entry(
            style,
            "--mode <MODE>",
            "benchmark mode [possible values: all, translation, opencode-session]",
        ),
        detail_entry(style, "--model <MODEL>", "benchmark model id"),
        detail_entry(style, "--prompt <PROMPT>", "benchmark prompt"),
        detail_entry(style, "--iterations <ITERATIONS>", "sample count"),
        detail_entry(style, "--warmups <WARMUPS>", "warmup runs"),
        detail_entry(
            style,
            "--skip-live",
            "skip instead of failing when live Claude access is unavailable",
        ),
        detail_entry(
            style,
            "--max-first-ms <MAX_FIRST_MS>",
            "max allowed first-token overhead",
        ),
        detail_entry(
            style,
            "--max-total-ms <MAX_TOTAL_MS>",
            "max allowed total overhead",
        ),
    ]));
    lines.join("\n")
}

pub fn benchmark_help() -> String {
    render_benchmark_help(console::stdout_style())
}

pub fn help_from_args<I, S>(args: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect::<Vec<_>>();

    if args.len() == 2 && is_help_flag(&args[1]) {
        return Some(detailed_help());
    }

    if args.len() == 3 && args[1] == "benchmark" && is_help_flag(&args[2]) {
        return Some(benchmark_help());
    }

    None
}

fn is_help_flag(value: &str) -> bool {
    matches!(value, "-h" | "--help")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_commands() {
        let help = render_detailed_help(console::Style::plain());

        assert!(help.contains("usage:"));
        assert!(help.contains("options:"));
        assert!(help.contains("commands:"));
        assert!(help.contains("subcommand usage:"));
        assert!(help.contains("openclaude -c [COMMAND]"));
        assert!(help.contains("openclaude bootstrap [COMMAND]"));
        assert!(help.contains("openclaude serve [--host <HOST>] [--port <PORT>]"));
        assert!(!help.contains("  openclaude alias                              install a shell alias for opencode passthrough"));
    }

    #[test]
    fn help_benchmark() {
        let help = render_benchmark_help(console::Style::plain());

        assert!(help.contains("openclaude benchmark"));
        assert!(help.contains("openclaude benchmark [OPTIONS]"));
        assert!(help.contains("--mode <MODE>"));
        assert!(help.contains("possible values: all, translation, opencode-session"));
        assert!(help.contains("--model <MODEL>"));
        assert!(help.contains("--skip-live"));
    }

    #[test]
    fn parses_help() {
        let cli = Cli::try_parse_from(["openclaude", "help"]).expect("parse help");

        assert!(matches!(cli.command, Some(Command::Help)));
    }

    #[test]
    fn routes_root_help_flag() {
        let help = help_from_args(["openclaude", "--help"]).expect("root help");
        assert!(help.contains("usage:"));
        assert!(help.contains("openclaude bootstrap [COMMAND]"));
    }

    #[test]
    fn routes_benchmark_help_flag() {
        let help = help_from_args(["openclaude", "benchmark", "--help"]).expect("benchmark help");
        assert!(help.contains("openclaude benchmark"));
        assert!(help.contains("--mode <MODE>"));
    }

    #[test]
    fn ignores_passthrough_help_flag() {
        assert!(help_from_args(["openclaude", "-c", "--help"]).is_none());
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
    fn parses_alias() {
        let cli = Cli::try_parse_from(["openclaude", "alias"]).expect("parse alias");

        assert!(matches!(cli.command, Some(Command::Alias)));
    }

    #[test]
    fn parses_opencode_arguments() {
        let cli = Cli::try_parse_from(["openclaude", "-c", "run hello"])
            .expect("parse opencode arguments");

        assert!(cli.command.is_none());
        assert_eq!(cli.opencode_arguments, Some(String::from("run hello")));
    }

    #[test]
    fn parses_empty_opencode_arguments() {
        let cli =
            Cli::try_parse_from(["openclaude", "-c"]).expect("parse empty opencode arguments");

        assert!(cli.command.is_none());
        assert_eq!(cli.opencode_arguments, Some(String::new()));
    }

    #[test]
    fn parses_quoted_opencode_arguments_before_openclaude_flags() {
        let cli = Cli::try_parse_from([
            "openclaude",
            "-c",
            "--help --other-argument",
            "--provider-id",
            "temp",
        ])
        .expect("parse quoted opencode arguments");

        assert_eq!(
            cli.opencode_arguments,
            Some(String::from("--help --other-argument"))
        );
        assert_eq!(cli.provider_id, "temp");
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
