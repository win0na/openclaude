use crate::provider::ProviderRuntime;
use crate::server::{OpenClaudeService, ServerCommand, ServerEnvelope};
use anyhow::Context;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};

pub fn serve_stdio<R: ProviderRuntime, In: Read, Out: Write>(
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
            Ok(ServerCommand::Describe) => ServerEnvelope::Success {
                response: service.describe(),
            },
            Ok(ServerCommand::Start { request }) => match service.start(request) {
                Ok(response) => ServerEnvelope::Success { response },
                Err(err) => ServerEnvelope::Error {
                    message: err.to_string(),
                },
            },
            Ok(ServerCommand::Resume { request }) => match service.resume(request) {
                Ok(response) => ServerEnvelope::Success { response },
                Err(err) => ServerEnvelope::Error {
                    message: err.to_string(),
                },
            },
            Err(err) => ServerEnvelope::Error {
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
    use crate::integration::{BridgeMessage, BridgeRequest, BridgeRole, OpenCodeBridge};
    use crate::provider::{
        FinishReason, ProviderInfo, ProviderModel, ProviderRequest, ProviderRuntime, StreamPart,
    };

    #[derive(Clone)]
    struct DescribeRuntime {
        model: ProviderModel,
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
            _request: ProviderRequest,
        ) -> anyhow::Result<std::vec::IntoIter<anyhow::Result<StreamPart>>> {
            Ok(vec![
                Ok(StreamPart::Start),
                Ok(StreamPart::Finish {
                    reason: FinishReason::EndTurn,
                }),
            ]
            .into_iter())
        }
    }

    #[test]
    fn serve_stdio_handles_describe_and_start() {
        let model = ProviderModel::claude("sonnet", "Claude Sonnet");
        let runtime = DescribeRuntime {
            model: model.clone(),
        };
        let bridge = OpenCodeBridge::new(runtime, vec![model]);
        let mut service = OpenClaudeService::new(bridge);

        let input = [
            serde_json::to_string(&ServerCommand::Describe).unwrap(),
            serde_json::to_string(&ServerCommand::Start {
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
        ]
        .join("\n");

        let mut output = Vec::new();
        serve_stdio(&mut service, input.as_bytes(), &mut output).unwrap();
        let responses = String::from_utf8(output).unwrap();

        assert!(responses.contains("\"provider_id\":\"mock\""));
        assert!(responses.contains("\"kind\":\"success\""));
    }
}
