use crate::claude::cli::ClaudeCli;
use crate::claude::prompt::build_claude_prompt;
use crate::claude::stream::ClaudeChunk;
use crate::claude::translate::ClaudeTranslator;
use crate::provider::{
    FinishReason, ProviderInfo, ProviderModel, ProviderRequest, ProviderRuntime, StreamPart,
};
use anyhow::Context;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use tracing::error;

#[derive(Clone)]
pub struct ClaudeCliRuntime {
    info: ProviderInfo,
    cli: ClaudeCli,
    models: Vec<ProviderModel>,
}

impl ClaudeCliRuntime {
    pub fn new(binary: impl Into<PathBuf>, models: Vec<ProviderModel>) -> Self {
        Self {
            info: ProviderInfo {
                id: "clyde".into(),
                name: "clyde".into(),
            },
            cli: ClaudeCli::new(binary),
            models,
        }
    }

    pub fn command_args(&self, request: &ProviderRequest) -> Vec<String> {
        let prompt = build_claude_prompt(request);
        self.cli
            .stream_args(&request.model.id, prompt.system_prompt.as_deref())
    }

    pub fn parse_stream_line(&self, line: &str) -> anyhow::Result<Vec<StreamPart>> {
        let chunk: ClaudeChunk =
            serde_json::from_str(line).context("failed to parse Claude stream line")?;
        let mut translator = ClaudeTranslator::new();
        Ok(translator.push_chunk(&chunk))
    }
}

impl ProviderRuntime for ClaudeCliRuntime {
    fn info(&self) -> ProviderInfo {
        self.info.clone()
    }

    fn models(&self) -> &[ProviderModel] {
        &self.models
    }

    fn stream(
        &self,
        request: ProviderRequest,
    ) -> anyhow::Result<Box<dyn Iterator<Item = anyhow::Result<StreamPart>> + Send>> {
        let prompt = build_claude_prompt(&request);

        let mut child = Command::new(self.cli.binary())
            .args(self.command_args(&request))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to spawn Claude CLI")?;

        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let (tx, rx) = mpsc::channel::<anyhow::Result<StreamPart>>();

        thread::spawn(move || {
            let _ = tx.send(Ok(StreamPart::Start));

            let stderr_handle = stderr.map(|stderr| {
                thread::spawn(move || {
                    let mut stderr = BufReader::new(stderr);
                    let mut text = String::new();
                    let _ = stderr.read_to_string(&mut text);
                    text
                })
            });

            let result: anyhow::Result<()> = (|| {
                if let Some(mut stdin) = stdin {
                    stdin
                        .write_all(prompt.user_prompt.as_bytes())
                        .context("failed to write prompt to Claude stdin")?;
                    drop(stdin);
                }

                let stdout = stdout.context("Claude stdout was not piped")?;
                let mut reader = BufReader::new(stdout);
                let mut translator = ClaudeTranslator::new();
                let mut line = String::new();

                loop {
                    line.clear();
                    let read = reader
                        .read_line(&mut line)
                        .context("failed to read Claude stdout")?;
                    if read == 0 {
                        break;
                    }

                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    let chunk: ClaudeChunk = serde_json::from_str(trimmed).with_context(|| {
                        format!("failed to parse Claude stream line: {trimmed}")
                    })?;

                    for part in translator.push_chunk(&chunk) {
                        if tx.send(Ok(part)).is_err() {
                            return Ok(());
                        }
                    }
                }

                let completed = child.wait().context("failed to wait for Claude CLI")?;
                if !completed.success() {
                    let stderr = stderr_handle
                        .and_then(|handle| handle.join().ok())
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    let message = if stderr.is_empty() {
                        format!("Claude CLI exited with status {completed}")
                    } else {
                        stderr
                    };
                    let _ = tx.send(Ok(StreamPart::Error { message }));
                    let _ = tx.send(Ok(StreamPart::Finish {
                        reason: FinishReason::Error,
                    }));
                    return Ok(());
                }

                let _ = tx.send(Ok(StreamPart::Finish {
                    reason: FinishReason::EndTurn,
                }));
                Ok(())
            })();

            if let Err(err) = result {
                error!(error = %err, "Claude CLI streaming failed");
                let _ = tx.send(Err(err));
            }
        });

        Ok(Box::new(rx.into_iter()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{MessagePart, MessageRole, ProviderMessage, StreamPart};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn args_model() {
        let runtime = ClaudeCliRuntime::new(
            "claude",
            vec![ProviderModel::claude("sonnet", "Claude Sonnet")],
        );
        let request = ProviderRequest {
            model: ProviderModel::claude("opus", "Claude Opus"),
            system_prompt: Some("system".into()),
            messages: vec![ProviderMessage {
                role: MessageRole::User,
                parts: vec![MessagePart::Text {
                    text: "hello".into(),
                }],
            }],
        };

        let args = runtime.command_args(&request);
        assert!(args.contains(&"opus".to_string()));
        assert!(args.contains(&"--include-partial-messages".to_string()));
    }

    #[test]
    fn stream_translates() {
        let dir = tempdir().unwrap();
        let script = dir.path().join("fake-claude.sh");
        fs::write(
            &script,
            "#!/bin/sh\ncat >/dev/null\nprintf '%s\n' '{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}}' '{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello\"}}}' '{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_stop\",\"index\":0}}'\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script, perms).unwrap();
        }

        let runtime = ClaudeCliRuntime::new(
            &script,
            vec![ProviderModel::claude("sonnet", "Claude Sonnet")],
        );
        let request = ProviderRequest {
            model: ProviderModel::claude("sonnet", "Claude Sonnet"),
            system_prompt: None,
            messages: vec![ProviderMessage {
                role: MessageRole::User,
                parts: vec![MessagePart::Text {
                    text: "earlier\n\nhello".into(),
                }],
            }],
        };

        let parts = runtime
            .stream(request)
            .unwrap()
            .collect::<anyhow::Result<Vec<_>>>()
            .unwrap();

        assert!(matches!(parts.first(), Some(StreamPart::Start)));
        assert!(
            parts
                .iter()
                .any(|part| matches!(part, StreamPart::TextDelta(text) if text.delta == "hello"))
        );
        assert!(matches!(
            parts.last(),
            Some(StreamPart::Finish {
                reason: FinishReason::EndTurn
            })
        ));
    }
}
