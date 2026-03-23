use serde_json::Value;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

fn spawn_log_reader<R: Read + Send + 'static>(pipe: R, logs: Arc<Mutex<Vec<String>>>) {
    thread::spawn(move || {
        let reader = BufReader::new(pipe);
        for line in reader.lines().map_while(Result::ok) {
            logs.lock().unwrap().push(line);
        }
    });
}

fn start_server(port: u16, script: &Path) -> (Child, Arc<Mutex<Vec<String>>>) {
    let binary = env!("CARGO_BIN_EXE_openclaude");
    let mut child = Command::new(binary)
        .arg("serve")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .env("OPENCLAUDE_CLAUDE_BIN", script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let logs = Arc::new(Mutex::new(Vec::new()));
    spawn_log_reader(child.stdout.take().unwrap(), logs.clone());
    spawn_log_reader(child.stderr.take().unwrap(), logs.clone());
    (child, logs)
}

fn wait_for_ready(_port: u16, logs: &Arc<Mutex<Vec<String>>>) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        if logs
            .lock()
            .unwrap()
            .iter()
            .any(|line| line.contains("HTTP server ready"))
        {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    panic!("server did not become ready: {:?}", *logs.lock().unwrap());
}

fn stop_server(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn run_wrapper(port: u16, script: &Path, args: &[&str], timeout_secs: u64) -> Output {
    let binary = env!("CARGO_BIN_EXE_openclaude");
    let temp = TempDir::new().unwrap();
    let home_dir = temp.path().join("home");
    let config_dir = temp.path().join("config");
    let data_dir = temp.path().join("data");
    let cache_dir = temp.path().join("cache");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&config_dir).unwrap();
    fs::create_dir_all(&data_dir).unwrap();
    fs::create_dir_all(&cache_dir).unwrap();
    let stdout_path = temp.path().join("wrapper.stdout");
    let stderr_path = temp.path().join("wrapper.stderr");
    let stdout_file = File::create(&stdout_path).unwrap();
    let stderr_file = File::create(&stderr_path).unwrap();
    let mut command = Command::new(binary);
    command
        .args(args)
        .env("OPENCLAUDE_CLAUDE_BIN", script)
        .env("OPENCLAUDE_BASE_URL", format!("http://127.0.0.1:{port}/v1"))
        .env("HOME", &home_dir)
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("XDG_DATA_HOME", &data_dir)
        .env("XDG_CACHE_HOME", &cache_dir)
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));

    let mut child = command.spawn().unwrap();
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return Output {
                status,
                stdout: fs::read(&stdout_path).unwrap(),
                stderr: fs::read(&stderr_path).unwrap(),
            };
        }
        if start.elapsed() > Duration::from_secs(timeout_secs) {
            let _ = child.kill();
            let status = child.wait().unwrap();
            return Output {
                status,
                stdout: fs::read(&stdout_path).unwrap(),
                stderr: fs::read(&stderr_path).unwrap(),
            };
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn parse_json_lines(text: &str) -> Vec<Value> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect()
}

fn write_fake_claude_script(dir: &Path, mode: &str) -> PathBuf {
    let script = dir.join("fake-claude.sh");
    let body = match mode {
        "text" => {
            r#"#!/usr/bin/env bash
set -euo pipefail
cat >/dev/null
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hello from fake claude"}}}'
printf '%s\n' '{"type":"stream_event","event":{"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null}}}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_stop","index":0}}'
"#
        }
        "tool" => {
            r#"#!/usr/bin/env bash
set -euo pipefail
input=$(cat)
if printf '%s' "$input" | grep -q 'tool_result:'; then
  printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}'
  printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"tool flow finished"}}}'
  printf '%s\n' '{"type":"stream_event","event":{"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null}}}'
  printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_stop","index":0}}'
else
  printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"bash","input":{"command":"printf tool-pass","description":"print test text"}}}}'
  printf '%s\n' '{"type":"stream_event","event":{"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null}}}'
  printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_stop","index":0}}'
fi
"#
        }
        other => panic!("unknown fake claude mode: {other}"),
    };

    fs::write(&script, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();
    }
    script
}

#[test]
fn opencode_run_basic_message_finishes_once() {
    let temp = TempDir::new().unwrap();
    let script = write_fake_claude_script(temp.path(), "text");
    let port = free_port();
    let (mut server, logs) = start_server(port, &script);
    wait_for_ready(port, &logs);

    let output = run_wrapper(
        port,
        &script,
        &[
            "run",
            "--format",
            "json",
            "-m",
            "openclaude/sonnet",
            "hello",
        ],
        30,
    );

    stop_server(&mut server);

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}\nserver logs: {:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        *logs.lock().unwrap()
    );
    let events = parse_json_lines(&String::from_utf8_lossy(&output.stdout));
    let texts = events
        .iter()
        .filter_map(|event| {
            event["type"]
                .as_str()
                .filter(|t| *t == "text")
                .map(|_| event["part"]["text"].as_str().unwrap().to_string())
        })
        .collect::<Vec<_>>();
    let finishes = events
        .iter()
        .filter(|event| event["type"] == "step_finish")
        .collect::<Vec<_>>();

    assert_eq!(texts, vec!["hello from fake claude"]);
    assert_eq!(finishes.len(), 1);
    assert_eq!(finishes[0]["part"]["reason"], "stop");
}

#[test]
fn opencode_run_tool_flow_finishes_without_looping() {
    let temp = TempDir::new().unwrap();
    let script = write_fake_claude_script(temp.path(), "tool");
    let port = free_port();
    let (mut server, logs) = start_server(port, &script);
    wait_for_ready(port, &logs);

    let output = run_wrapper(
        port,
        &script,
        &[
            "run",
            "--format",
            "json",
            "-m",
            "openclaude/sonnet",
            "run a tool",
        ],
        30,
    );

    stop_server(&mut server);

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}\nserver logs: {:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        *logs.lock().unwrap()
    );
    let events = parse_json_lines(&String::from_utf8_lossy(&output.stdout));
    let texts = events
        .iter()
        .filter_map(|event| {
            event["type"]
                .as_str()
                .filter(|t| *t == "text")
                .map(|_| event["part"]["text"].as_str().unwrap().to_string())
        })
        .collect::<Vec<_>>();
    let finishes = events
        .iter()
        .filter(|event| event["type"] == "step_finish")
        .collect::<Vec<_>>();
    let finish_reasons = finishes
        .iter()
        .map(|event| event["part"]["reason"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();

    assert_eq!(texts, vec!["tool flow finished"]);
    assert_eq!(finish_reasons, vec!["tool-calls", "stop"]);
}
