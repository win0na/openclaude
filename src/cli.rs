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

const DETAILED_HELP_GUIDE: &str = r#"openclaude

Usage:
  openclaude [OPTIONS] [COMMAND]

Example:
  openclaude serve

Commands:
  help
      print the detailed help page

  reference
      refresh the optional local opencode checkout

  serve
      start the HTTP server; primary integration command

  stdio
      start the stdio bridge explicitly

Options:
  --provider-id <PROVIDER_ID>
      [env: OPENCLAUDE_PROVIDER_ID=] [default: openclaude]

  --default-model <DEFAULT_MODEL>
      [env: OPENCLAUDE_MODEL=] [default: sonnet]

  --claude-bin <CLAUDE_BIN>
      [env: OPENCLAUDE_CLAUDE_BIN=] [default: claude]

  --workdir <WORKDIR>
      [env: OPENCLAUDE_WORKDIR=] [default: /tmp/openclaude]

  -h, --help
      print help"#;

pub fn detailed_help() -> String {
    DETAILED_HELP_GUIDE.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detailed_help_mentions_explicit_commands() {
        let help = detailed_help();

        assert!(help.contains("Usage:"));
        assert!(help.contains("Example:"));
        assert!(help.contains("openclaude serve"));
        assert!(help.contains("  stdio\n      start the stdio bridge explicitly"));
        assert!(help.contains("primary integration command"));
        assert!(!help.contains("quick guide"));
    }

    #[test]
    fn parses_help_subcommand() {
        let cli = Cli::try_parse_from(["openclaude", "help"]).expect("parse help");

        assert!(matches!(cli.command, Some(Command::Help)));
    }
}
