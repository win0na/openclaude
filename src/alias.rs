use crate::exec;
use anyhow::{bail, Context};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const START_MARKER: &str = "# >>> openclaude alias >>>";
const END_MARKER: &str = "# <<< openclaude alias <<<";

pub struct AliasInstall {
    pub shell: &'static str,
    pub rc_path: PathBuf,
}

pub fn install() -> anyhow::Result<AliasInstall> {
    let shell = active_shell()?;
    let rc_path = rc_path(shell)?;
    let existing = fs::read_to_string(&rc_path).unwrap_or_default();
    let updated = replace_managed_block(
        &existing,
        &managed_block(shell, &exec::shell_command_for_openclaude()?),
    );
    fs::write(&rc_path, updated)
        .with_context(|| format!("failed to update {}", rc_path.display()))?;
    Ok(AliasInstall { shell, rc_path })
}

fn active_shell() -> anyhow::Result<&'static str> {
    let shell = env::var("SHELL").context("SHELL is not set")?;
    if shell.ends_with("/zsh") {
        Ok("zsh")
    } else if shell.ends_with("/bash") {
        Ok("bash")
    } else {
        bail!("unsupported shell `{shell}` for openclaude alias installation")
    }
}

fn rc_path(shell: &str) -> anyhow::Result<PathBuf> {
    let home = env::var("HOME").context("HOME is not set")?;
    Ok(Path::new(&home).join(match shell {
        "zsh" => ".zshrc",
        "bash" => ".bashrc",
        _ => unreachable!(),
    }))
}

fn managed_block(shell: &str, openclaude_command: &str) -> String {
    let function = match shell {
        "zsh" | "bash" => {
            format!(
                "opencode() {{\n  command {} -c \"$(printf '%q ' \"$@\")\"\n}}",
                openclaude_command
            )
        }
        _ => unreachable!(),
    };

    format!("{START_MARKER}\n{function}\n{END_MARKER}\n")
}

fn replace_managed_block(existing: &str, block: &str) -> String {
    let mut trimmed = existing.to_string();
    if let (Some(start), Some(end)) = (trimmed.find(START_MARKER), trimmed.find(END_MARKER)) {
        let end = end + END_MARKER.len();
        trimmed.replace_range(start..end, "");
        trimmed = trimmed.trim_end().to_string();
    }
    if !trimmed.is_empty() {
        trimmed.push_str("\n\n");
    }
    trimmed.push_str(block);
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_block() {
        let existing = "first\n# >>> openclaude alias >>>\nold\n# <<< openclaude alias <<<\n";
        let updated = replace_managed_block(
            existing,
            "# >>> openclaude alias >>>\nnew\n# <<< openclaude alias <<<\n",
        );
        assert!(updated.contains("first"));
        assert!(updated.contains("new"));
        assert!(!updated.contains("old"));
    }

    #[test]
    fn zsh_block() {
        let block = managed_block("zsh", "openclaude");
        assert!(block.contains(START_MARKER));
        assert!(block.contains("opencode()"));
        assert!(block.contains("printf '%q ' \"$@\""));
        assert!(block.contains("command openclaude -c"));
        assert!(block.contains(END_MARKER));
    }

    #[test]
    fn absolute_block() {
        let block = managed_block("bash", "'/tmp/openclaude'");
        assert!(block.contains("command '/tmp/openclaude' -c"));
        assert!(block.contains("printf '%q ' \"$@\""));
    }
}
