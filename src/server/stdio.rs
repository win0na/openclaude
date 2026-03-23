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
