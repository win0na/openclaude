use crate::provider::ToolResult;
use anyhow::Context;

pub fn format_tool_result(result: &ToolResult) -> anyhow::Result<String> {
    let body = serde_json::to_string_pretty(&result.output)
        .context("failed to serialize tool result output")?;

    let mut output = String::new();
    output.push_str("tool result:\n");
    output.push_str("- call id: ");
    output.push_str(&result.call_id);
    output.push('\n');

    if let Some(tool_name) = result
        .tool_name
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        output.push_str("- tool: ");
        output.push_str(tool_name);
        output.push('\n');
    }

    output.push_str("- output:\n");
    for line in body.lines() {
        output.push_str("  ");
        output.push_str(line);
        output.push('\n');
    }

    Ok(output.trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn formats_tool_result_as_stable_text() {
        let formatted = format_tool_result(&ToolResult {
            call_id: "toolu_1".into(),
            tool_name: Some("Read".into()),
            output: json!({"content": "hello"}),
        })
        .unwrap();

        assert!(formatted.starts_with("tool result:\n- call id: toolu_1\n- tool: Read"));
        assert!(formatted.contains("- output:\n  {"));
    }
}
