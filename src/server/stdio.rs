use crate::provider::ProviderRuntime;
use crate::server::{ClydeService, ServerCommand, ServerEnvelope};
use anyhow::Context;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};

pub fn serve_stdio<R: ProviderRuntime + Clone, In: Read, Out: Write>(
    service: &mut ClydeService<R>,
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
            Ok(ServerCommand::Complete {
                request_id,
                request,
            }) => match service.complete(request) {
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
    use crate::integration::{BridgeRequest, OpenCodeBridge};
    use crate::provider::{
        ProviderInfo, ProviderModel, ProviderRequest, ProviderRuntime, StreamPart,
    };
    use crate::server::ServerRequest;

    #[derive(Clone)]
    struct MockRuntime {
        model: ProviderModel,
    }

    impl ProviderRuntime for MockRuntime {
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
        ) -> anyhow::Result<Box<dyn Iterator<Item = anyhow::Result<StreamPart>> + Send>> {
            Ok(Box::new(std::iter::once(Ok(StreamPart::Finish {
                reason: crate::provider::FinishReason::EndTurn,
            }))))
        }
    }

    fn service() -> ClydeService<MockRuntime> {
        let model = ProviderModel::claude("sonnet", "Claude Sonnet");
        let runtime = MockRuntime {
            model: model.clone(),
        };
        ClydeService::new(OpenCodeBridge::new(runtime, vec![model]))
    }

    #[test]
    fn skips_blank_lines() {
        let input = b"\n\n{\"kind\":\"describe\",\"request_id\":\"req-1\"}\n";
        let mut output = Vec::new();
        serve_stdio(&mut service(), &input[..], &mut output).unwrap();

        let text = String::from_utf8(output).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(text.contains("\"request_id\":\"req-1\""));
    }

    #[test]
    fn invalid_json_returns_error_envelope() {
        let input = b"{not-json}\n";
        let mut output = Vec::new();
        serve_stdio(&mut service(), &input[..], &mut output).unwrap();

        let envelope: ServerEnvelope = serde_json::from_slice(&output).unwrap();
        match envelope {
            ServerEnvelope::Error {
                request_id,
                message,
            } => {
                assert!(request_id.is_none());
                assert!(message.contains("invalid command"));
            }
            other => panic!("unexpected envelope: {other:?}"),
        }
    }

    #[test]
    fn complete_failure_preserves_request_id() {
        let request = ServerCommand::Complete {
            request_id: "req-2".into(),
            request: ServerRequest {
                conversation: BridgeRequest {
                    model_id: "unknown".into(),
                    system_prompt: None,
                    messages: Vec::new(),
                },
            },
        };
        let input = format!("{}\n", serde_json::to_string(&request).unwrap());
        let mut output = Vec::new();
        serve_stdio(&mut service(), input.as_bytes(), &mut output).unwrap();

        let envelope: ServerEnvelope = serde_json::from_slice(&output).unwrap();
        match envelope {
            ServerEnvelope::Error {
                request_id,
                message,
            } => {
                assert_eq!(request_id.as_deref(), Some("req-2"));
                assert!(message.contains("unknown model id"));
            }
            other => panic!("unexpected envelope: {other:?}"),
        }
    }
}
