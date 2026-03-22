use anyhow::Context;
use std::path::{Path, PathBuf};

const EMBEDDED_REFERENCE: &str = include_str!("../OPENCODE_REFERENCE.md");
const REFERENCE_FILENAME: &str = "OPENCODE_REFERENCE.md";

const SOURCE_URLS: &[&str] = &[
    "https://raw.githubusercontent.com/sst/opencode/dev/packages/plugin/src/index.ts",
    "https://raw.githubusercontent.com/sst/opencode/dev/packages/opencode/src/plugin/index.ts",
    "https://raw.githubusercontent.com/sst/opencode/dev/packages/opencode/src/provider/provider.ts",
    "https://raw.githubusercontent.com/sst/opencode/dev/packages/opencode/src/session/llm.ts",
    "https://raw.githubusercontent.com/sst/opencode/dev/packages/opencode/src/session/prompt.ts",
    "https://raw.githubusercontent.com/sst/opencode/dev/packages/opencode/src/session/message-v2.ts",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFetchResult {
    pub url: String,
    pub status: ReferenceStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceStatus {
    Downloaded { bytes: usize },
    Failed { message: String },
}

pub fn reference_path(project_root: &Path) -> PathBuf {
    project_root.join(REFERENCE_FILENAME)
}

pub fn refresh_reference(project_root: &Path) -> anyhow::Result<Vec<SourceFetchResult>> {
    let results = fetch_sources();
    let rendered = render_reference(&results);
    let output_path = reference_path(project_root);

    std::fs::write(&output_path, rendered)
        .with_context(|| format!("failed to write {}", output_path.display()))?;

    Ok(results)
}

fn fetch_sources() -> Vec<SourceFetchResult> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("openclaude-init/0.1")
        .build();

    let Ok(client) = client else {
        return SOURCE_URLS
            .iter()
            .map(|url| SourceFetchResult {
                url: (*url).to_string(),
                status: ReferenceStatus::Failed {
                    message: "failed to initialize HTTP client".into(),
                },
            })
            .collect();
    };

    SOURCE_URLS
        .iter()
        .map(|url| match client.get(*url).send() {
            Ok(response) => match response.error_for_status() {
                Ok(ok) => match ok.text() {
                    Ok(body) => SourceFetchResult {
                        url: (*url).to_string(),
                        status: ReferenceStatus::Downloaded { bytes: body.len() },
                    },
                    Err(err) => SourceFetchResult {
                        url: (*url).to_string(),
                        status: ReferenceStatus::Failed {
                            message: err.to_string(),
                        },
                    },
                },
                Err(err) => SourceFetchResult {
                    url: (*url).to_string(),
                    status: ReferenceStatus::Failed {
                        message: err.to_string(),
                    },
                },
            },
            Err(err) => SourceFetchResult {
                url: (*url).to_string(),
                status: ReferenceStatus::Failed {
                    message: err.to_string(),
                },
            },
        })
        .collect()
}

fn render_reference(results: &[SourceFetchResult]) -> String {
    let mut output = String::from(EMBEDDED_REFERENCE.trim_end());
    output.push_str("\n\n## refresh metadata\n\n");
    output.push_str("This file can be refreshed with `openclaude init`. The latest refresh attempted to download these upstream OpenCode references:\n\n");

    for result in results {
        output.push_str("- `");
        output.push_str(&result.url);
        output.push_str("` — ");
        match &result.status {
            ReferenceStatus::Downloaded { bytes } => {
                output.push_str("downloaded ");
                output.push_str(&bytes.to_string());
                output.push_str(" bytes");
            }
            ReferenceStatus::Failed { message } => {
                output.push_str("failed (");
                output.push_str(message);
                output.push(')');
            }
        }
        output.push('\n');
    }

    output.push_str("\nIf downloads fail, the embedded project reference above remains the baseline fallback.\n");
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_path_targets_project_root() {
        let path = reference_path(Path::new("/tmp/openclaude"));
        assert_eq!(path, PathBuf::from("/tmp/openclaude/OPENCODE_REFERENCE.md"));
    }

    #[test]
    fn render_reference_includes_refresh_metadata() {
        let rendered = render_reference(&[SourceFetchResult {
            url: "https://example.test/ref".into(),
            status: ReferenceStatus::Downloaded { bytes: 42 },
        }]);

        assert!(rendered.contains("## refresh metadata"));
        assert!(rendered.contains("downloaded 42 bytes"));
    }
}
