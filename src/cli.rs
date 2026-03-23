use crate::console;
use clap::{Parser, Subcommand};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Clone, Parser)]
#[command(name = "openclaude", disable_help_subcommand = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(long, env = "OPENCLAUDE_PROVIDER_ID", default_value = "openclaude")]
    pub provider_id: String,

    #[arg(long, env = "OPENCLAUDE_MODEL", default_value = "sonnet")]
    pub default_model: String,

    #[arg(long, env = "OPENCLAUDE_AVAILABLE_MODELS", value_delimiter = ',')]
    pub available_models: Vec<String>,

    #[arg(long, env = "OPENCLAUDE_CLAUDE_BIN", default_value = "claude")]
    pub claude_bin: PathBuf,

    #[arg(long, env = "OPENCLAUDE_OPENCODE_BIN", default_value = "opencode")]
    pub opencode_bin: PathBuf,

    #[arg(
        long,
        env = "OPENCLAUDE_BASE_URL",
        default_value = "http://127.0.0.1:3000/v1"
    )]
    pub base_url: String,

    #[arg(long, env = "OPENCLAUDE_WORKDIR", default_value = "/tmp/openclaude")]
    pub workdir: PathBuf,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    Help,
    Reference {
        #[arg(long, default_value = ".")]
        project_root: PathBuf,
    },
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value = "3000")]
        port: u16,
    },
    Stdio,
    #[command(external_subcommand)]
    External(Vec<OsString>),
}

fn command_line(style: console::Style, name: &str, description: &str) -> String {
    inline_help_line(style.command(name), name, description)
}

fn option_line(style: console::Style, name: &str, description: &str) -> String {
    inline_help_line(style.option(name), name, description)
}

fn inline_help_line(styled_name: String, plain_name: &str, description: &str) -> String {
    const DESCRIPTION_COLUMN: usize = 44;
    let padding = DESCRIPTION_COLUMN
        .saturating_sub(2 + plain_name.len())
        .max(1);

    format!("  {styled_name}{}{description}", " ".repeat(padding))
}

fn help_block(styled_name: String, description: &str) -> [String; 2] {
    [format!("  {styled_name}"), format!("    {description}")]
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
        "default: launch opencode with bootstrap config and plugin",
    ));
    lines.extend(help_block(
        style.command("openclaude serve"),
        "start the HTTP backend server",
    ));
    lines.extend([
        String::new(),
        style.heading("commands:"),
        command_line(style, "help", "print the detailed help page"),
        command_line(
            style,
            "reference",
            "refresh the optional local opencode checkout",
        ),
        command_line(style, "serve", "start the HTTP backend server"),
        command_line(style, "stdio", "start the STDIO bridge explicitly"),
        String::new(),
        style.heading("options:"),
        option_line(
            style,
            "--provider-id <PROVIDER_ID>",
            "[env: OPENCLAUDE_PROVIDER_ID=] [default: openclaude]",
        ),
        option_line(
            style,
            "--default-model <DEFAULT_MODEL>",
            "[env: OPENCLAUDE_MODEL=] [default: sonnet]",
        ),
        option_line(
            style,
            "--available-models <AVAILABLE_MODELS>",
            "[env: OPENCLAUDE_AVAILABLE_MODELS=] [comma-separated override]",
        ),
        option_line(
            style,
            "--claude-bin <CLAUDE_BIN>",
            "[env: OPENCLAUDE_CLAUDE_BIN=] [default: claude]",
        ),
        option_line(
            style,
            "--opencode-bin <OPENCODE_BIN>",
            "[env: OPENCLAUDE_OPENCODE_BIN=] [default: opencode]",
        ),
        option_line(
            style,
            "--base-url <BASE_URL>",
            "[env: OPENCLAUDE_BASE_URL=] [default: http://127.0.0.1:3000/v1]",
        ),
        option_line(
            style,
            "--workdir <WORKDIR>",
            "[env: OPENCLAUDE_WORKDIR=] [default: /tmp/openclaude]",
        ),
        option_line(style, "-h, --help", "print help"),
        String::new(),
        style.heading("subcommand usage:"),
    ]);
    lines.extend(help_block(
        style.command("openclaude serve [--host <HOST>] [--port <PORT>]"),
        "start the HTTP backend server",
    ));
    lines.extend(help_block(
        style.command("openclaude reference [--project-root <PROJECT_ROOT>]"),
        "refresh the optional local opencode checkout",
    ));
    lines.join("\n")
}

pub fn detailed_help() -> String {
    render_detailed_help(console::stdout_style())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detailed_help_mentions_explicit_commands() {
        let help = render_detailed_help(console::Style::plain());

        assert!(help.contains("usage:"));
        assert!(help.contains("example:"));
        assert!(help.contains("openclaude"));
        assert!(help.contains("default: launch opencode with bootstrap config and plugin"));
        assert!(help.contains("openclaude serve"));
        assert!(help.contains("start the HTTP backend server"));
        assert!(help.contains("subcommand usage:"));
        assert!(help.contains("openclaude serve [--host <HOST>] [--port <PORT>]"));
        assert!(help.contains("openclaude reference [--project-root <PROJECT_ROOT>]"));
        assert!(help.contains("launch opencode with bootstrap config and plugin"));
        assert!(!help.contains("  bootstrap"));
        assert!(help.contains("stdio"));
        assert!(help.contains("start the STDIO bridge explicitly"));
        assert!(!help.contains("quick guide"));
    }

    #[test]
    fn detailed_help_adds_ansi_when_enabled() {
        let help = render_detailed_help(console::Style::color());

        assert!(help.contains("\x1b["));
    }

    #[test]
    fn parses_help_subcommand() {
        let cli = Cli::try_parse_from(["openclaude", "help"]).expect("parse help");

        assert!(matches!(cli.command, Some(Command::Help)));
    }

    #[test]
    fn captures_external_subcommand_for_opencode_passthrough() {
        let cli = Cli::try_parse_from(["openclaude", "run", "hello"]).expect("parse passthrough");

        match cli.command {
            Some(Command::External(args)) => {
                assert_eq!(args, vec![OsString::from("run"), OsString::from("hello")]);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
