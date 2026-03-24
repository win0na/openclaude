use crate::claude::ClaudeCli;
use crate::cli::Cli;
use crate::provider::catalog::default_model;
use crate::provider::ProviderModel;
use anyhow::{bail, Context};
use serde_json::{json, Map, Value};
use std::env;
use std::ffi::OsString;
use std::process::{Command, Stdio};
use tracing::warn;

pub fn launch_opencode(cli: &Cli, args: &[OsString]) -> anyhow::Result<()> {
    let models = requested_models(cli, args);
    let bootstrap_config = merged_bootstrap_config(cli, &models)?;
    let status = Command::new(&cli.opencode_bin)
        .args(args)
        .env("OPENCLAUDE_PROVIDER_ID", &cli.provider_id)
        .env("OPENCLAUDE_BASE_URL", &cli.base_url)
        .env(
            "OPENCODE_CONFIG_CONTENT",
            serde_json::to_string(&bootstrap_config)?,
        )
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to launch {}", cli.opencode_bin.display()))?;

    match status.code() {
        Some(code) => std::process::exit(code),
        None => bail!("opencode process terminated without an exit code"),
    }
}

fn requested_models(cli: &Cli, args: &[OsString]) -> Vec<ProviderModel> {
    if !cli.available_models.is_empty() {
        return ClaudeCli::new(&cli.claude_bin).discover_available_models(&cli.available_models);
    }

    if let Some(model) = requested_model_from_args(args, &cli.provider_id) {
        return vec![model];
    }

    let discovery =
        ClaudeCli::new(&cli.claude_bin).discover_available_models_report(&cli.available_models);
    if let Some(message) = discovery.warning.as_deref() {
        warn!(claude_bin = %cli.claude_bin.display(), "{message}");
    }
    discovery.models
}

fn requested_model_from_args(args: &[OsString], provider_id: &str) -> Option<ProviderModel> {
    let mut iter = args.iter().map(|value| value.to_string_lossy());
    while let Some(arg) = iter.next() {
        let value = if arg == "-m" || arg == "--model" {
            iter.next().map(|value| value.into_owned())
        } else {
            arg.strip_prefix("--model=").map(str::to_string)
        };

        let Some(value) = value else {
            continue;
        };
        let prefix = format!("{provider_id}/");
        let Some(model_id) = value.strip_prefix(&prefix) else {
            continue;
        };
        return Some(
            default_model(model_id).unwrap_or_else(|| {
                ProviderModel::claude(model_id.to_string(), model_id.to_string())
            }),
        );
    }
    None
}

fn merged_bootstrap_config(cli: &Cli, models: &[ProviderModel]) -> anyhow::Result<Value> {
    let mut config = existing_inline_config()?;
    merge_missing(&mut config, bootstrap_patch(cli, models)?);
    Ok(config)
}

fn existing_inline_config() -> anyhow::Result<Value> {
    let Some(text) = env::var_os("OPENCODE_CONFIG_CONTENT") else {
        return Ok(json!({}));
    };

    serde_json::from_str(&text.to_string_lossy()).context(
        "openclaude cannot merge the existing OPENCODE_CONFIG_CONTENT because it is not valid JSON",
    )
}

fn bootstrap_patch(cli: &Cli, models: &[ProviderModel]) -> anyhow::Result<Value> {
    let mut model_map = Map::new();
    for model in models {
        model_map.insert(
            model.id.to_string(),
            json!({
                "name": model.display_name,
                "id": model.id,
            }),
        );
    }

    let mut provider_map = Map::new();
    provider_map.insert(
        cli.provider_id.clone(),
        json!({
            "npm": "@ai-sdk/openai-compatible",
            "name": "openclaude",
            "options": {
                "baseURL": cli.base_url,
            },
            "models": model_map,
        }),
    );

    Ok(json!({
        "provider": provider_map,
    }))
}

fn merge_missing(target: &mut Value, patch: Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                match target.get_mut(&key) {
                    Some(existing) => merge_missing(existing, value),
                    None => {
                        target.insert(key, value);
                    }
                }
            }
        }
        (Value::Array(target), Value::Array(patch)) => {
            for value in patch {
                if !target.contains(&value) {
                    target.push(value);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use crate::provider::ProviderModel;
    use std::ffi::OsString;

    fn test_cli() -> Cli {
        Cli {
            command: None,
            provider_id: "openclaude".into(),
            default_model: "sonnet".into(),
            available_models: Vec::new(),
            claude_bin: "claude".into(),
            opencode_bin: "opencode".into(),
            base_url: "http://127.0.0.1:3000/v1".into(),
            workdir: "/tmp/openclaude".into(),
        }
    }

    #[test]
    fn patch_provider() {
        let value = bootstrap_patch(
            &test_cli(),
            &[ProviderModel::claude("sonnet", "Claude Sonnet 4.6")],
        )
        .unwrap();
        assert_eq!(
            value["provider"]["openclaude"]["options"]["baseURL"],
            "http://127.0.0.1:3000/v1"
        );
        assert!(value.get("plugin").is_none());
        assert!(value["provider"]["openclaude"]["models"]["sonnet"].is_object());
    }

    #[test]
    fn merge_arrays() {
        let mut target = json!({
            "plugin": ["file:///existing.ts"],
            "provider": {
                "openclaude": {
                    "options": {
                        "baseURL": "http://custom"
                    }
                }
            }
        });
        let patch = bootstrap_patch(
            &test_cli(),
            &[ProviderModel::claude("sonnet", "Claude Sonnet 4.6")],
        )
        .unwrap();

        merge_missing(&mut target, patch);

        assert_eq!(
            target["provider"]["openclaude"]["options"]["baseURL"],
            "http://custom"
        );
        assert_eq!(target["plugin"].as_array().unwrap().len(), 1);
        assert!(target["provider"]["openclaude"]["models"]["sonnet"].is_object());
    }

    #[test]
    fn requested_model() {
        let args = vec![
            OsString::from("run"),
            OsString::from("-m"),
            OsString::from("openclaude/sonnet"),
            OsString::from("hello"),
        ];

        let model = requested_model_from_args(&args, "openclaude").unwrap();

        assert_eq!(model, ProviderModel::claude("sonnet", "Claude Sonnet"));
    }
}
