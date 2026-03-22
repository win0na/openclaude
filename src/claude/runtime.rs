use crate::claude::cli::ClaudeCli;
use crate::claude::prompt::build_claude_prompt;
use crate::claude::stream::ClaudeChunk;
use crate::claude::translate::ClaudeTranslator;
use crate::provider::{
    FinishReason, MessageRole, ProviderInfo, ProviderMessage, ProviderModel, ProviderRequest,
    ProviderRuntime, StreamPart, ToolCallPart, ToolResult,
};
use anyhow::Context;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
struct SuspendedToolSession {
    request: ProviderRequest,
    tool_call: ToolCallPart,
}

pub struct ClaudeCliRuntime {
    info: ProviderInfo,
    cli: ClaudeCli,
    models: Vec<ProviderModel>,
    suspended: Arc<Mutex<std::collections::HashMap<String, SuspendedToolSession>>>,
}

impl ClaudeCliRuntime {
    pub fn new(binary: impl Into<PathBuf>, models: Vec<ProviderModel>) -> Self {
        Self {
            info: ProviderInfo {
                id: "openclaude".into(),
                name: "OpenClaude".into(),
            },
            cli: ClaudeCli::new(binary),
            models,
            suspended: Arc::new(Mutex::new(std::collections::HashMap::new())),
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

    fn remember_suspended_tool_call(&self, request: &ProviderRequest, tool_call: &ToolCallPart) {
        let session = SuspendedToolSession {
            request: request.clone(),
            tool_call: tool_call.clone(),
        };
        if let Ok(mut suspended) = self.suspended.lock() {
            suspended.insert(tool_call.tool_call_id.clone(), session);
        }
    }

    fn build_continuation_request(
        &self,
        session: SuspendedToolSession,
        result: ToolResult,
    ) -> anyhow::Result<ProviderRequest> {
        let mut messages = session.request.messages.clone();

        if !session.request.prompt.trim().is_empty() {
            messages.push(ProviderMessage {
                role: MessageRole::User,
                content: session.request.prompt.clone(),
            });
        }

        messages.push(ProviderMessage {
            role: MessageRole::Assistant,
            content: format!(
                "tool call issued: {} ({})",
                result
                    .tool_name
                    .clone()
                    .unwrap_or_else(|| session.tool_call.tool_name.clone()),
                session.tool_call.tool_call_id
            ),
        });
        messages.push(ProviderMessage {
            role: MessageRole::Tool,
            content: serde_json::to_string_pretty(&result.output)
                .context("failed to serialize tool result output")?,
        });

        Ok(ProviderRequest {
            model: session.request.model,
            system_prompt: session.request.system_prompt,
            prompt: "continue from the tool result above".into(),
            messages,
        })
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
    ) -> anyhow::Result<std::vec::IntoIter<anyhow::Result<StreamPart>>> {
        let mut output = vec![Ok(StreamPart::Start)];
        let prompt = build_claude_prompt(&request);

        let mut child = Command::new(self.cli.binary())
            .args(self.command_args(&request))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to spawn Claude CLI")?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(prompt.user_prompt.as_bytes())
                .context("failed to write prompt to Claude stdin")?;
        }

        let completed = child
            .wait_with_output()
            .context("failed to wait for Claude CLI output")?;

        if !completed.status.success() {
            let stderr = String::from_utf8_lossy(&completed.stderr)
                .trim()
                .to_string();
            output.push(Ok(StreamPart::Error {
                message: if stderr.is_empty() {
                    format!("Claude CLI exited with status {}", completed.status)
                } else {
                    stderr
                },
            }));
            output.push(Ok(StreamPart::Finish {
                reason: FinishReason::Error,
            }));
            return Ok(output.into_iter());
        }

        let stdout =
            String::from_utf8(completed.stdout).context("Claude stdout was not valid UTF-8")?;
        let mut translator = ClaudeTranslator::new();

        for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
            let chunk: ClaudeChunk = serde_json::from_str(line)
                .with_context(|| format!("failed to parse Claude stream line: {line}"))?;
            let translated = translator.push_chunk(&chunk);
            for part in &translated {
                if let StreamPart::ToolCall(tool_call) = part {
                    self.remember_suspended_tool_call(&request, tool_call);
                }
            }
            output.extend(translated.into_iter().map(Ok));
        }

        output.push(Ok(StreamPart::Finish {
            reason: FinishReason::EndTurn,
        }));
        Ok(output.into_iter())
    }

    fn submit_tool_result(&self, result: ToolResult) -> anyhow::Result<Option<ProviderRequest>> {
        let session = self
            .suspended
            .lock()
            .map_err(|_| anyhow::anyhow!("suspended session mutex poisoned"))?
            .remove(&result.call_id);

        let Some(session) = session else {
            return Ok(None);
        };

        Ok(Some(self.build_continuation_request(session, result)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{MessageRole, ProviderMessage, StreamPart};
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn command_args_follow_request_model() {
        let runtime = ClaudeCliRuntime::new(
            "claude",
            vec![ProviderModel::claude("sonnet", "Claude Sonnet")],
        );
        let request = ProviderRequest {
            model: ProviderModel::claude("opus", "Claude Opus"),
            system_prompt: Some("system".into()),
            prompt: "hello".into(),
            messages: vec![],
        };

        let args = runtime.command_args(&request);
        assert!(args.contains(&"opus".to_string()));
        assert!(args.contains(&"--include-partial-messages".to_string()));
    }

    #[test]
    fn stream_runs_subprocess_and_translates_output() {
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
            prompt: "hello".into(),
            messages: vec![ProviderMessage {
                role: MessageRole::User,
                content: "earlier".into(),
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

    #[test]
    fn submit_tool_result_builds_continuation_request() {
        let runtime = ClaudeCliRuntime::new(
            "claude",
            vec![ProviderModel::claude("sonnet", "Claude Sonnet")],
        );
        let request = ProviderRequest {
            model: ProviderModel::claude("sonnet", "Claude Sonnet"),
            system_prompt: Some("system".into()),
            prompt: "latest question".into(),
            messages: vec![ProviderMessage {
                role: MessageRole::User,
                content: "earlier".into(),
            }],
        };

        runtime.remember_suspended_tool_call(
            &request,
            &ToolCallPart {
                id: "toolu_1".into(),
                tool_call_id: "toolu_1".into(),
                tool_name: "Read".into(),
                input: json!({"file_path": "/tmp/a"}),
            },
        );

        let continuation = runtime
            .submit_tool_result(ToolResult {
                call_id: "toolu_1".into(),
                tool_name: None,
                output: json!({"content": "file body"}),
            })
            .unwrap()
            .unwrap();

        assert_eq!(continuation.system_prompt.as_deref(), Some("system"));
        assert_eq!(continuation.prompt, "continue from the tool result above");
        assert!(
            continuation
                .messages
                .iter()
                .any(|msg| msg.role == MessageRole::Tool)
        );
    }
}
