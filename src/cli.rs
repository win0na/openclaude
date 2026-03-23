use clap::{Parser, Subcommand};
use std::env;
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

#[derive(Clone, Copy)]
struct HelpStyle {
    enabled: bool,
}

impl HelpStyle {
    fn plain() -> Self {
        Self { enabled: false }
    }

    fn color() -> Self {
        Self { enabled: true }
    }

    fn paint(self, text: &str, ansi: &str) -> String {
        if self.enabled {
            format!("\x1b[{ansi}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    fn title(self, text: &str) -> String {
        self.paint(text, "1;96")
    }

    fn heading(self, text: &str) -> String {
        self.paint(text, "1;94")
    }

    fn command(self, text: &str) -> String {
        self.paint(text, "1;93")
    }

    fn option(self, text: &str) -> String {
        self.paint(text, "0;92")
    }
}

fn command_line(style: HelpStyle, name: &str, description: &str) -> String {
    format!("  {}\n      {description}", style.command(name))
}

fn option_line(style: HelpStyle, name: &str, description: &str) -> String {
    format!("  {}\n      {description}", style.option(name))
}

fn render_detailed_help(style: HelpStyle) -> String {
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
        String::new(),
        command_line(
            style,
            "reference",
            "refresh the optional local opencode checkout",
        ),
        String::new(),
        command_line(
            style,
            "serve",
            "start the HTTP server; primary integration command",
        ),
        String::new(),
        command_line(style, "stdio", "start the stdio bridge explicitly"),
        String::new(),
        style.heading("Options:"),
        option_line(
            style,
            "--provider-id <PROVIDER_ID>",
            "[env: OPENCLAUDE_PROVIDER_ID=] [default: openclaude]",
        ),
        String::new(),
        option_line(
            style,
            "--default-model <DEFAULT_MODEL>",
            "[env: OPENCLAUDE_MODEL=] [default: sonnet]",
        ),
        String::new(),
        option_line(
            style,
            "--claude-bin <CLAUDE_BIN>",
            "[env: OPENCLAUDE_CLAUDE_BIN=] [default: claude]",
        ),
        String::new(),
        option_line(
            style,
            "--workdir <WORKDIR>",
            "[env: OPENCLAUDE_WORKDIR=] [default: /tmp/openclaude]",
        ),
        String::new(),
        option_line(style, "-h, --help", "print help"),
    ]
    .join("\n")
}

pub fn detailed_help(color: bool) -> String {
    let style = if color && env::var_os("NO_COLOR").is_none() {
        HelpStyle::color()
    } else {
        HelpStyle::plain()
    };

    render_detailed_help(style)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detailed_help_mentions_explicit_commands() {
        let help = detailed_help(false);

        assert!(help.contains("Usage:"));
        assert!(help.contains("Example:"));
        assert!(help.contains("openclaude serve"));
        assert!(help.contains("  stdio\n      start the stdio bridge explicitly"));
        assert!(help.contains("primary integration command"));
        assert!(!help.contains("quick guide"));
    }

    #[test]
    fn detailed_help_adds_ansi_when_enabled() {
        let help = detailed_help(true);

        assert!(help.contains("\x1b["));
    }

    #[test]
    fn parses_help_subcommand() {
        let cli = Cli::try_parse_from(["openclaude", "help"]).expect("parse help");

        assert!(matches!(cli.command, Some(Command::Help)));
    }
}
