use std::env;
use std::process::Command;

#[test]
fn latency_live() {
    if env::var("CLYDE_RUN_BENCHMARK_TEST").ok().as_deref() != Some("1") {
        return;
    }

    let binary = env!("CARGO_BIN_EXE_clyde");
    let output = Command::new(binary)
        .arg("benchmark")
        .arg("--iterations")
        .arg("1")
        .arg("--warmups")
        .arg("0")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        text.contains("overhead") || text.contains("benchmark skipped:"),
        "unexpected benchmark output: {text}"
    );
}
