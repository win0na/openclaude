use crate::claude::ClaudeCli;
use crate::cli::Cli;
use crate::exec::resolve_opencode_path;
use crate::provider::ProviderModel;
use crate::provider::catalog::default_model;
use anyhow::{Context, bail};
use reqwest::blocking::Client;
use serde_json::{Map, Value, json};
use std::env;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tracing::warn;

pub fn launch_opencode(cli: &Cli, args: &[OsString]) -> anyhow::Result<()> {
    let code = run_opencode(cli, args)?;
    std::process::exit(code)
}

pub fn launch_opencode_with_server(cli: &Cli, args: &[OsString]) -> anyhow::Result<()> {
    let target = server_target(&cli.base_url)?;
    ensure_server_port_available(&target.host, target.port)?;
    let mut sidecar = start_sidecar(cli, &target)?;
    let result = (|| -> anyhow::Result<i32> {
        wait_ready(&sidecar, &target)?;
        run_bootstrap_command(cli, args)
    })();
    stop_sidecar(&mut sidecar);
    let code = result?;
    std::process::exit(code)
}

pub fn run_opencode(cli: &Cli, args: &[OsString]) -> anyhow::Result<i32> {
    let models = requested_models(cli, args);
    let bootstrap_config = merged_bootstrap_config(cli, &models)?;
    let opencode_bin = resolve_opencode_path(&cli.opencode_bin)?;
    let status = Command::new(&opencode_bin)
        .args(args)
        .env("CLYDE_PROVIDER_ID", &cli.provider_id)
        .env("CLYDE_BASE_URL", &cli.base_url)
        .env(
            "OPENCODE_CONFIG_CONTENT",
            serde_json::to_string(&bootstrap_config)?,
        )
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to launch {}", opencode_bin.display()))?;

    match status.code() {
        Some(code) => Ok(code),
        None => bail!("opencode process terminated without an exit code"),
    }
}

fn run_bootstrap_command(cli: &Cli, args: &[OsString]) -> anyhow::Result<i32> {
    let binary = std::env::current_exe()?;
    let mut command = Command::new(binary);
    command
        .arg("--provider-id")
        .arg(&cli.provider_id)
        .arg("--default-model")
        .arg(&cli.default_model)
        .arg("--claude-bin")
        .arg(&cli.claude_bin)
        .arg("--opencode-bin")
        .arg(&cli.opencode_bin)
        .arg("--base-url")
        .arg(&cli.base_url)
        .arg("--workdir")
        .arg(&cli.workdir);
    if !cli.available_models.is_empty() {
        command
            .arg("--available-models")
            .arg(cli.available_models.join(","));
    }
    let status = command
        .arg("bootstrap")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| {
            format!(
                "failed to launch {} bootstrap",
                command.get_program().to_string_lossy()
            )
        })?;

    match status.code() {
        Some(code) => Ok(code),
        None => bail!("bootstrap process terminated without an exit code"),
    }
}

struct ServerTarget {
    host: String,
    port: u16,
    health_url: String,
}

struct ServerSidecar {
    child: Child,
    logs: Arc<Mutex<Vec<String>>>,
}

fn server_target(base_url: &str) -> anyhow::Result<ServerTarget> {
    let url = reqwest::Url::parse(base_url)
        .with_context(|| format!("invalid --base-url `{base_url}`"))?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("--base-url `{base_url}` is missing a host"))?
        .to_string();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| anyhow::anyhow!("--base-url `{base_url}` is missing a port"))?;
    Ok(ServerTarget {
        health_url: format!("{}://{}:{}/health", url.scheme(), host, port),
        host,
        port,
    })
}

fn provider_base_url(base_url: &str) -> anyhow::Result<String> {
    let mut url = reqwest::Url::parse(base_url)
        .with_context(|| format!("invalid --base-url `{base_url}`"))?;
    let path = url.path().trim_end_matches('/');
    if path.is_empty() || path == "/" {
        url.set_path("/v1");
    } else if path != "/v1" {
        url.set_path(&format!("{path}/v1"));
    }
    Ok(url.to_string().trim_end_matches('/').to_string())
}

fn ensure_server_port_available(host: &str, port: u16) -> anyhow::Result<()> {
    TcpListener::bind((host, port)).with_context(|| {
        format!(
            "cannot start the bundled clyde server because {host}:{port} is already in use; stop the existing process or run `clyde bootstrap` to use an already-running server"
        )
    })?;
    Ok(())
}

fn spawn_logs<R: Read + Send + 'static>(pipe: R, logs: Arc<Mutex<Vec<String>>>) {
    thread::spawn(move || {
        let reader = BufReader::new(pipe);
        for line in reader.lines().map_while(Result::ok) {
            logs.lock().unwrap().push(line);
        }
    });
}

fn start_sidecar(cli: &Cli, target: &ServerTarget) -> anyhow::Result<ServerSidecar> {
    let binary = std::env::current_exe()?;
    let mut command = Command::new(binary);
    command
        .arg("--provider-id")
        .arg(&cli.provider_id)
        .arg("--default-model")
        .arg(&cli.default_model)
        .arg("--claude-bin")
        .arg(&cli.claude_bin)
        .arg("--opencode-bin")
        .arg(&cli.opencode_bin)
        .arg("--base-url")
        .arg(&cli.base_url)
        .arg("--workdir")
        .arg(&cli.workdir);
    if !cli.available_models.is_empty() {
        command
            .arg("--available-models")
            .arg(cli.available_models.join(","));
    }
    let mut child = command
        .arg("serve")
        .arg("--host")
        .arg(&target.host)
        .arg("--port")
        .arg(target.port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| "failed to start bundled clyde server".to_string())?;

    let logs = Arc::new(Mutex::new(Vec::new()));
    spawn_logs(child.stdout.take().unwrap(), logs.clone());
    spawn_logs(child.stderr.take().unwrap(), logs.clone());
    Ok(ServerSidecar { child, logs })
}

fn wait_ready(sidecar: &ServerSidecar, target: &ServerTarget) -> anyhow::Result<()> {
    let client = Client::builder()
        .timeout(Duration::from_millis(250))
        .build()?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        if client
            .get(&target.health_url)
            .send()
            .map(|response| response.status().is_success())
            .unwrap_or(false)
        {
            return Ok(());
        }
        if sidecar
            .logs
            .lock()
            .unwrap()
            .iter()
            .any(|line| line.contains("failed") || line.contains("Address already in use"))
        {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }

    let logs = sidecar.logs.lock().unwrap().join("\n");
    bail!(
        "bundled clyde server did not become ready at {}; logs:\n{}",
        target.health_url,
        logs
    )
}

fn stop_sidecar(sidecar: &mut ServerSidecar) {
    let _ = sidecar.child.kill();
    let _ = sidecar.child.wait();
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
        "clyde cannot merge the existing OPENCODE_CONFIG_CONTENT because it is not valid JSON",
    )
}

fn bootstrap_patch(cli: &Cli, models: &[ProviderModel]) -> anyhow::Result<Value> {
    let provider_base_url = provider_base_url(&cli.base_url)?;
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
            "name": "clyde",
            "options": {
                "baseURL": provider_base_url,
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
            opencode_arguments: None,
            provider_id: "clyde".into(),
            default_model: "sonnet".into(),
            available_models: Vec::new(),
            claude_bin: "claude".into(),
            opencode_bin: "opencode".into(),
            base_url: "http://127.0.0.1:43123".into(),
            workdir: "/tmp/clyde".into(),
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
            value["provider"]["clyde"]["options"]["baseURL"],
            "http://127.0.0.1:43123/v1"
        );
        assert!(value.get("plugin").is_none());
        assert!(value["provider"]["clyde"]["models"]["sonnet"].is_object());
    }

    #[test]
    fn merge_arrays() {
        let mut target = json!({
            "plugin": ["file:///existing.ts"],
            "provider": {
                "clyde": {
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
            target["provider"]["clyde"]["options"]["baseURL"],
            "http://custom"
        );
        assert_eq!(target["plugin"].as_array().unwrap().len(), 1);
        assert!(target["provider"]["clyde"]["models"]["sonnet"].is_object());
    }

    #[test]
    fn requested_model() {
        let args = vec![
            OsString::from("run"),
            OsString::from("-m"),
            OsString::from("clyde/sonnet"),
            OsString::from("hello"),
        ];

        let model = requested_model_from_args(&args, "clyde").unwrap();

        assert_eq!(model, ProviderModel::claude("sonnet", "Claude Sonnet"));
    }

    #[test]
    fn server_url() {
        let target = server_target("http://127.0.0.1:43123").unwrap();

        assert_eq!(target.host, "127.0.0.1");
        assert_eq!(target.port, 43123);
        assert_eq!(target.health_url, "http://127.0.0.1:43123/health");
    }

    #[test]
    fn provider_url() {
        assert_eq!(
            provider_base_url("http://127.0.0.1:43123").unwrap(),
            "http://127.0.0.1:43123/v1"
        );
        assert_eq!(
            provider_base_url("http://127.0.0.1:43123/v1").unwrap(),
            "http://127.0.0.1:43123/v1"
        );
    }

    #[test]
    fn rejects_recursive_opencode() {
        let mut cli = test_cli();
        cli.opencode_bin = std::env::current_exe().unwrap();

        let err = run_opencode(&cli, &[]).unwrap_err().to_string();
        assert!(err.contains("points back to clyde"));
    }
}
