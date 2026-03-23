use crate::console;
use clap::{Parser, Subcommand};
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

    #[arg(long, env = "OPENCLAUDE_CLAUDE_BIN", default_value = "claude")]
    pub claude_bin: PathBuf,

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
}

fn command_line(style: console::Style, name: &str, description: &str) -> String {
    format!("  {:<16} {description}", style.command(name))
}

fn option_line(style: console::Style, name: &str, description: &str) -> String {
    format!("  {:<32} {description}", style.option(name))
}

fn render_detailed_help(style: console::Style) -> String {
    [
        style.title("openclaude"),
        String::new(),
        style.heading("Usage:"),
        format!("  {}", style.command("openclaude [OPTIONS] [COMMAND]")),
        String::new(),
        style.heading("Example:"),
        format!("  {}", style.command("openclaude serve")),
        String::new(),
        style.heading("Commands:"),
        command_line(style, "help", "print the detailed help page"),
        command_line(
            style,
            "reference",
            "refresh the optional local opencode checkout",
        ),
        command_line(
            style,
            "serve",
            "start the HTTP server; primary integration command",
        ),
        command_line(style, "stdio", "start the stdio bridge explicitly"),
        String::new(),
        style.heading("Options:"),
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
            "--claude-bin <CLAUDE_BIN>",
            "[env: OPENCLAUDE_CLAUDE_BIN=] [default: claude]",
        ),
        option_line(
            style,
            "--workdir <WORKDIR>",
            "[env: OPENCLAUDE_WORKDIR=] [default: /tmp/openclaude]",
        ),
        option_line(style, "-h, --help", "print help"),
    ]
    .join("\n")
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

        assert!(help.contains("Usage:"));
        assert!(help.contains("Example:"));
        assert!(help.contains("openclaude serve"));
        assert!(help.contains("stdio"));
        assert!(help.contains("start the stdio bridge explicitly"));
        assert!(help.contains("primary integration command"));
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
}
