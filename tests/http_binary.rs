use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn manifest_dir() -> &'static str {
    env!("CARGO_MANIFEST_DIR")
}

fn fixture(name: &str) -> String {
    fs::read_to_string(format!("{}/tests/fixtures/{name}", manifest_dir())).unwrap()
}

fn golden(name: &str) -> String {
    fs::read_to_string(format!("{}/tests/golden/{name}", manifest_dir())).unwrap()
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
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
cat >/dev/null
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"Read","input":{}}}}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"file_path\":\"/tmp/a\"}"}}}'
printf '%s\n' '{"type":"stream_event","event":{"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null}}}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_stop","index":0}}'
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

fn start_server(port: u16, script: &Path) -> (TempDir, Child, Arc<Mutex<Vec<String>>>) {
    let temp = TempDir::new().unwrap();
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
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    spawn_log_reader(stdout, logs.clone());
    spawn_log_reader(stderr, logs.clone());

    (temp, child, logs)
}

fn spawn_log_reader<R: Read + Send + 'static>(pipe: R, logs: Arc<Mutex<Vec<String>>>) {
    thread::spawn(move || {
        let reader = BufReader::new(pipe);
        for line in reader.lines().map_while(Result::ok) {
            logs.lock().unwrap().push(line);
        }
    });
}

fn wait_for_ready(port: u16, logs: &Arc<Mutex<Vec<String>>>) {
    let client = Client::builder()
        .timeout(Duration::from_millis(250))
        .build()
        .unwrap();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        if client
            .get(format!("http://127.0.0.1:{port}/health"))
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            return;
        }
        if logs
            .lock()
            .unwrap()
            .iter()
            .any(|line| line.contains("failed"))
        {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    panic!("server did not become ready: {:?}", *logs.lock().unwrap());
}

fn stop_server(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn normalize_sse(text: &str) -> String {
    let mut out = String::new();
    for block in text.split("\n\n") {
        if block.trim().is_empty() {
            continue;
        }
        if block.trim() == "data: [DONE]" {
            out.push_str("data: [DONE]\n\n");
            continue;
        }
        let json = block.strip_prefix("data: ").unwrap();
        let mut value: Value = serde_json::from_str(json).unwrap();
        value["id"] = json!("CHATCMPL_ID");
        value["created"] = json!(0);
        value["model"] = json!("MODEL");
        out.push_str("data: ");
        out.push_str(&serde_json::to_string(&sort_json(value)).unwrap());
        out.push_str("\n\n");
    }
    out
}

fn sort_json(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(sort_json).collect()),
        Value::Object(map) => {
            let mut entries = map.into_iter().collect::<Vec<_>>();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut sorted = serde_json::Map::new();
            for (key, value) in entries {
                sorted.insert(key, sort_json(value));
            }
            Value::Object(sorted)
        }
        other => other,
    }
}

#[test]
fn binary_http_basic_message_round_trip() {
    let port = free_port();
    let temp = TempDir::new().unwrap();
    let script = write_fake_claude_script(temp.path(), "text");
    let (_server_temp, mut child, logs) = start_server(port, &script);
    wait_for_ready(port, &logs);

    let client = Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
        .header("content-type", "application/json")
        .body(fixture("basic_chat.json"))
        .send()
        .unwrap();
    let json: Value = response.json().unwrap();

    stop_server(&mut child);

    assert_eq!(
        json["choices"][0]["message"]["content"],
        "hello from fake claude"
    );
    assert_eq!(json["choices"][0]["finish_reason"], "stop");
}

#[test]
fn binary_http_stream_matches_golden_text() {
    let port = free_port();
    let temp = TempDir::new().unwrap();
    let script = write_fake_claude_script(temp.path(), "text");
    let (_server_temp, mut child, logs) = start_server(port, &script);
    wait_for_ready(port, &logs);

    let client = Client::new();
    let text = client
        .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
        .header("content-type", "application/json")
        .body(fixture("stream_text.json"))
        .send()
        .unwrap()
        .text()
        .unwrap();

    stop_server(&mut child);

    assert_eq!(
        normalize_sse(&text).trim_end(),
        golden("basic_chat_stream.sse").trim_end()
    );
}

#[test]
fn binary_http_stream_matches_golden_tool_call() {
    let port = free_port();
    let temp = TempDir::new().unwrap();
    let script = write_fake_claude_script(temp.path(), "tool");
    let (_server_temp, mut child, logs) = start_server(port, &script);
    wait_for_ready(port, &logs);

    let client = Client::new();
    let text = client
        .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
        .header("content-type", "application/json")
        .body(fixture("tool_choice_function.json"))
        .send()
        .unwrap()
        .text()
        .unwrap();

    stop_server(&mut child);

    assert_eq!(
        normalize_sse(&text).trim_end(),
        golden("tool_call_stream.sse").trim_end()
    );
}
