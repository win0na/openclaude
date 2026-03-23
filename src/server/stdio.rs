use crate::provider::ProviderRuntime;
use crate::server::{OpenClaudeService, ServerCommand, ServerEnvelope};
use anyhow::Context;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};

pub fn serve_stdio<R: ProviderRuntime + Clone, In: Read, Out: Write>(
    service: &mut OpenClaudeService<R>,
    input: In,
    output: Out,
) -> anyhow::Result<()> {
    let reader = BufReader::new(input);
    let mut writer = BufWriter::new(output);

    for line in reader.lines() {
        let line = line.context("failed to read stdio line")?;
        if line.trim().is_empty() {
            continue;
        }

        let envelope = match serde_json::from_str::<ServerCommand>(&line) {
            Ok(ServerCommand::Describe { request_id }) => ServerEnvelope::Success {
                request_id,
                response: service.describe(),
            },
            Ok(ServerCommand::Start {
                request_id,
                request,
            }) => match service.start(request) {
                Ok(response) => ServerEnvelope::Success {
                    request_id,
                    response,
                },
                Err(err) => ServerEnvelope::Error {
                    request_id: Some(request_id),
                    message: err.to_string(),
                },
            },
            Ok(ServerCommand::Resume {
                request_id,
                request,
            }) => match service.resume(request) {
                Ok(response) => ServerEnvelope::Success {
                    request_id,
                    response,
                },
                Err(err) => ServerEnvelope::Error {
                    request_id: Some(request_id),
                    message: err.to_string(),
                },
            },
            Err(err) => ServerEnvelope::Error {
                request_id: None,
                message: format!("invalid command: {err}"),
            },
        };

        serde_json::to_writer(&mut writer, &envelope).context("failed to write stdio response")?;
        writer
            .write_all(b"\n")
            .context("failed to write response newline")?;
        writer.flush().context("failed to flush stdio response")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integration::{
        BridgeMessage, BridgeRequest, BridgeRole, BridgeToolResult, OpenCodeBridge,
    };
    use crate::provider::{
        FinishReason, ProviderInfo, ProviderModel, ProviderRequest, ProviderRuntime, StreamPart,
        ToolResult,
    };
    use serde_json::json;

    #[derive(Clone)]
    struct DescribeRuntime {
        model: ProviderModel,
        resumed: std::sync::Arc<std::sync::Mutex<bool>>,
    }

    impl ProviderRuntime for DescribeRuntime {
        fn info(&self) -> ProviderInfo {
            ProviderInfo {
                id: "mock".into(),
                name: "Mock".into(),
            }
        }

        fn models(&self) -> &[ProviderModel] {
            std::slice::from_ref(&self.model)
        }

        fn stream(
            &self,
            request: ProviderRequest,
        ) -> anyhow::Result<std::vec::IntoIter<anyhow::Result<StreamPart>>> {
            if request.prompt == "continue" {
                *self.resumed.lock().unwrap() = true;
                return Ok(vec![
                    Ok(StreamPart::TextDelta(crate::provider::TextPart {
                        id: "part-0".into(),
                        delta: "done".into(),
                    })),
                    Ok(StreamPart::Finish {
                        reason: FinishReason::EndTurn,
                    }),
                ]
                .into_iter());
            }

            Ok(vec![
                Ok(StreamPart::ToolCall(crate::provider::ToolCallPart {
                    id: "toolu_1".into(),
                    tool_call_id: "toolu_1".into(),
                    tool_name: "Read".into(),
                    input: json!({"file_path": "/tmp/a"}),
                })),
                Ok(StreamPart::Finish {
                    reason: FinishReason::ToolCall,
                }),
            ]
            .into_iter())
        }

        fn submit_tool_result(
            &self,
            _result: ToolResult,
        ) -> anyhow::Result<Option<ProviderRequest>> {
            Ok(Some(ProviderRequest {
                model: self.model.clone(),
                system_prompt: None,
                prompt: "continue".into(),
                messages: vec![],
            }))
        }
    }

    #[test]
    fn serve_stdio_handles_describe_start_and_resume_with_session_ids() {
        let model = ProviderModel::claude("sonnet", "Claude Sonnet");
        let resumed = std::sync::Arc::new(std::sync::Mutex::new(false));
        let runtime = DescribeRuntime {
            model: model.clone(),
            resumed: resumed.clone(),
        };
        let bridge = OpenCodeBridge::new(runtime, vec![model]);
        let mut service = OpenClaudeService::new(bridge);

        let input = [
            serde_json::to_string(&ServerCommand::Describe {
                request_id: "req-1".into(),
            })
            .unwrap(),
            serde_json::to_string(&ServerCommand::Start {
                request_id: "req-2".into(),
                request: crate::server::ServerRequest {
                    conversation: BridgeRequest {
                        model_id: "sonnet".into(),
                        system_prompt: None,
                        prompt: "hello".into(),
                        messages: vec![BridgeMessage {
                            role: BridgeRole::User,
                            content: "earlier".into(),
                        }],
                    },
                },
            })
            .unwrap(),
            serde_json::to_string(&ServerCommand::Resume {
                request_id: "req-3".into(),
                request: crate::server::ServerContinueRequest {
                    session_id: "session-1".into(),
                    tool_result: BridgeToolResult {
                        call_id: "toolu_1".into(),
                        tool_name: Some("Read".into()),
                        output: json!({"content": "file"}),
                    },
                },
            })
            .unwrap(),
        ]
        .join("\n");

        let mut output = Vec::new();
        serve_stdio(&mut service, input.as_bytes(), &mut output).unwrap();
        let responses = String::from_utf8(output).unwrap();

        assert!(responses.contains("\"provider_id\":\"mock\""));
        assert!(responses.contains("\"kind\":\"success\""));
        assert!(responses.contains("\"request_id\":\"req-1\""));
        assert!(responses.contains("\"request_id\":\"req-2\""));
        assert!(responses.contains("\"request_id\":\"req-3\""));
        assert!(responses.contains("\"session_id\":\"session-1\""));
        assert!(*resumed.lock().unwrap());
    }
}
