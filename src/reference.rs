use anyhow::{Context, anyhow};
use std::path::{Path, PathBuf};
use std::process::Command;

const REFERENCE_DIRNAME: &str = "opencode-reference";
const REFERENCE_REPO_URL: &str = "https://github.com/sst/opencode.git";
const REFERENCE_BRANCH: &str = "dev";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceRefreshResult {
    pub repo_url: String,
    pub path: PathBuf,
    pub status: ReferenceStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceStatus {
    Cloned,
    Updated,
}

pub fn reference_path(project_root: &Path) -> PathBuf {
    project_root.join(REFERENCE_DIRNAME)
}

pub fn refresh_reference(project_root: &Path) -> anyhow::Result<ReferenceRefreshResult> {
    refresh_reference_from(project_root, REFERENCE_REPO_URL)
}

pub fn refresh_reference_from(
    project_root: &Path,
    repo_url: &str,
) -> anyhow::Result<ReferenceRefreshResult> {
    let output_path = reference_path(project_root);

    if output_path.join(".git").exists() {
        run_git(
            [
                "-C",
                output_path
                    .to_str()
                    .ok_or_else(|| anyhow!("invalid reference path"))?,
                "fetch",
                "origin",
                REFERENCE_BRANCH,
                "--depth",
                "1",
            ],
            project_root,
        )?;
        run_git(
            [
                "-C",
                output_path
                    .to_str()
                    .ok_or_else(|| anyhow!("invalid reference path"))?,
                "reset",
                "--hard",
                "FETCH_HEAD",
            ],
            project_root,
        )?;

        return Ok(ReferenceRefreshResult {
            repo_url: repo_url.into(),
            path: output_path,
            status: ReferenceStatus::Updated,
        });
    }

    if output_path.exists() {
        std::fs::remove_dir_all(&output_path)
            .with_context(|| format!("failed to remove {}", output_path.display()))?;
    }

    run_git(
        [
            "clone",
            "--depth",
            "1",
            "--branch",
            REFERENCE_BRANCH,
            repo_url,
            output_path
                .to_str()
                .ok_or_else(|| anyhow!("invalid reference path"))?,
        ],
        project_root,
    )?;

    Ok(ReferenceRefreshResult {
        repo_url: repo_url.into(),
        path: output_path,
        status: ReferenceStatus::Cloned,
    })
}

fn run_git<I, S>(args: I, workdir: &Path) -> anyhow::Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new("git")
        .args(args)
        .current_dir(workdir)
        .output()
        .context("failed to spawn git")?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(anyhow!(if stderr.is_empty() {
        format!("git exited with status {}", output.status)
    } else {
        stderr
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn reference_path_targets_project_root() {
        let path = reference_path(Path::new("/tmp/openclaude"));
        assert_eq!(path, PathBuf::from("/tmp/openclaude/opencode-reference"));
    }

    #[test]
    fn refresh_reference_from_clones_local_repo() {
        let repo_root = tempdir().unwrap();
        let source = repo_root.path().join("source");
        let project = repo_root.path().join("project");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&project).unwrap();

        Command::new("git")
            .args(["init", "--initial-branch", "dev"])
            .current_dir(&source)
            .output()
            .unwrap();
        fs::write(source.join("README.md"), "hello\n").unwrap();
        Command::new("git")
            .args(["add", "README.md"])
            .current_dir(&source)
            .output()
            .unwrap();
        Command::new("git")
            .args([
                "-c",
                "user.name=test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "init",
            ])
            .current_dir(&source)
            .output()
            .unwrap();

        let result = refresh_reference_from(&project, source.to_str().unwrap()).unwrap();
        assert_eq!(result.status, ReferenceStatus::Cloned);
        assert!(project.join("opencode-reference/.git").exists());
    }
}
