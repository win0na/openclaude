use crate::claude::ClaudeCli;
use crate::cli::Cli;
use crate::provider::ProviderModel;
use anyhow::{bail, Context};
use serde_json::{json, Map, Value};
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub fn launch_opencode(cli: &Cli, args: &[OsString]) -> anyhow::Result<()> {
    let models = ClaudeCli::new(&cli.claude_bin).discover_available_models(&cli.available_models);
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
    let plugin = plugin_entry()?;
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
        "plugin": [plugin],
        "provider": provider_map,
    }))
}

fn plugin_entry() -> anyhow::Result<String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dist = root.join("plugin/dist/index.js");
    let src = root.join("plugin/src/index.ts");
    let plugin = if dist.exists() { dist } else { src };
    if !plugin.exists() {
        bail!("openclaude plugin entry not found at {}", plugin.display());
    }
    Ok(file_url(&plugin))
}

fn file_url(path: &Path) -> String {
    format!("file://{}", path.display())
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
    fn bootstrap_patch_contains_plugin_and_provider() {
        let value = bootstrap_patch(
            &test_cli(),
            &[ProviderModel::claude("sonnet", "Claude Sonnet 4.6")],
        )
        .unwrap();
        assert_eq!(
            value["provider"]["openclaude"]["options"]["baseURL"],
            "http://127.0.0.1:3000/v1"
        );
        assert!(value["plugin"].as_array().unwrap()[0]
            .as_str()
            .unwrap()
            .starts_with("file://"));
        assert!(value["provider"]["openclaude"]["models"]["sonnet"].is_object());
    }

    #[test]
    fn merge_missing_preserves_existing_scalars_and_appends_arrays() {
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
        assert_eq!(target["plugin"].as_array().unwrap().len(), 2);
        assert!(target["provider"]["openclaude"]["models"]["sonnet"].is_object());
    }
}
