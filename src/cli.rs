use clap::{CommandFactory, Parser, Subcommand};
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

pub fn detailed_help() -> String {
    let mut command = Cli::command();
    let mut help = Vec::new();
    command.write_long_help(&mut help).expect("write help");
    let help = String::from_utf8(help).expect("utf8 help");

    format!(
        "{help}\n\nquick guide\n\nopenclaude is a translation layer between OpenCode and Claude Code. commands stay explicit: nothing starts a server or stdio bridge unless you invoke that command directly.\n\ncommands\n\n- serve\n  start the OpenAI-compatible HTTP server. use this for real OpenCode provider integration.\n\n- stdio\n  run the line-oriented stdio bridge. use this for direct subprocess integration or debugging.\n\n- reference\n  refresh the optional local `opencode-reference/` checkout for source inspection.\n\n- help\n  print this detailed help page.\n\ncommon examples\n\n- `openclaude serve`\n  start the HTTP server on `127.0.0.1:3000`.\n\n- `openclaude serve --host 0.0.0.0 --port 3000`\n  expose the HTTP server on a custom interface and port.\n\n- `openclaude stdio`\n  start the stdio bridge explicitly.\n\n- `openclaude reference`\n  refresh the optional local OpenCode checkout.\n\nnotes\n\n- bare `openclaude` prints help instead of starting a transport.\n- `serve` is the primary integration path for no-patch OpenCode usage.\n- `stdio` remains available as an explicit developer transport."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detailed_help_mentions_explicit_commands() {
        let help = detailed_help();

        assert!(help.contains("openclaude serve"));
        assert!(help.contains("openclaude stdio"));
        assert!(help.contains("bare `openclaude` prints help"));
    }

    #[test]
    fn parses_help_subcommand() {
        let cli = Cli::try_parse_from(["openclaude", "help"]).expect("parse help");

        assert!(matches!(cli.command, Some(Command::Help)));
    }
}
