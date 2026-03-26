use anyhow::{Context, bail};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

pub fn resolve_spawn_path(program: &Path) -> anyhow::Result<PathBuf> {
    if has_path_separator(program) || program.is_absolute() {
        return Ok(program.to_path_buf());
    }

    find_in_path(program.to_string_lossy().as_ref()).with_context(|| {
        format!(
            "failed to resolve `{}` from PATH",
            program.to_string_lossy()
        )
    })
}

pub fn resolve_opencode_path(program: &Path) -> anyhow::Result<PathBuf> {
    let resolved = resolve_spawn_path(program)?;
    let clyde = fs::canonicalize(std::env::current_exe()?)?;
    let candidate = fs::canonicalize(&resolved).unwrap_or(resolved.clone());
    if candidate == clyde {
        bail!(
            "resolved opencode binary points back to clyde ({}); pass --opencode-bin with the real opencode executable",
            resolved.display()
        );
    }
    Ok(resolved)
}

fn has_path_separator(path: &Path) -> bool {
    path.components().count() > 1
}

fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .map(|meta| meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub fn shell_command_for_clyde() -> anyhow::Result<String> {
    let name = OsStr::new("clyde");
    if find_in_path(name.to_string_lossy().as_ref()).is_some() {
        return Ok(String::from("clyde"));
    }

    let current = std::env::current_exe()?;
    Ok(shell_escape(&current))
}

fn shell_escape(path: &Path) -> String {
    let text = path.to_string_lossy();
    format!("'{}'", text.replace('"', "\"").replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_shell_escape() {
        let escaped = shell_escape(Path::new("/tmp/open claude"));
        assert_eq!(escaped, "'/tmp/open claude'");
    }
}
