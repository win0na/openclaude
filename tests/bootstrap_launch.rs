use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

fn write_script(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }
}

fn fake_claude_script(dir: &Path) -> PathBuf {
    let script = dir.join("fake-claude.sh");
    write_script(
        &script,
        r#"#!/usr/bin/env bash
set -euo pipefail
cat >/dev/null
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ok"}}}'
printf '%s\n' '{"type":"stream_event","event":{"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null}}}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_stop","index":0}}'
"#,
    );
    script
}

fn fake_opencode_script(dir: &Path) -> PathBuf {
    let script = dir.join("fake-opencode.sh");
    write_script(
        &script,
        r#"#!/usr/bin/env bash
set -euo pipefail
python3 - <<'PY'
import json
import os
import urllib.request

base = os.environ["OPENCLAUDE_BASE_URL"]
config = json.loads(os.environ["OPENCODE_CONFIG_CONTENT"])
provider = config["provider"][os.environ["OPENCLAUDE_PROVIDER_ID"]]
assert provider["options"]["baseURL"] == base
urllib.request.urlopen(base.rsplit('/v1', 1)[0] + "/health", timeout=5).read()
print("bootstrap-ok")
PY
"#,
    );
    script
}

#[test]
fn launch_combined() {
    let temp = TempDir::new().unwrap();
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let claude = fake_claude_script(temp.path());
    let opencode = fake_opencode_script(temp.path());
    let binary = env!("CARGO_BIN_EXE_openclaude");

    let output = Command::new(binary)
        .arg("--base-url")
        .arg(&base_url)
        .arg("--claude-bin")
        .arg(&claude)
        .arg("--opencode-bin")
        .arg(&opencode)
        .arg("--available-models")
        .arg("sonnet")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("bootstrap-ok"));
}

#[test]
fn launch_conflict() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let binary = env!("CARGO_BIN_EXE_openclaude");

    let output = Command::new(binary)
        .arg("--base-url")
        .arg(&base_url)
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("already in use"));
}
