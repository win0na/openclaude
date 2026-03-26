use crate::claude::{build_claude_prompt, ClaudeCliRuntime};
use crate::cli::{BenchmarkMode, BenchmarkOptions, Cli};
use crate::console;
use crate::provider::{
    MessagePart, MessageRole, ProviderMessage, ProviderModel, ProviderRequest, StreamPart,
};
use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use uuid::Uuid;

#[derive(Clone, Debug)]
struct Sample {
    first_ms: f64,
    total_ms: f64,
}

#[derive(Debug)]
struct Summary {
    min_first_ms: f64,
    median_first_ms: f64,
    avg_first_ms: f64,
    min_total_ms: f64,
    median_total_ms: f64,
    avg_total_ms: f64,
}

struct BenchmarkWorkspace {
    _temp: TempDir,
    home_dir: PathBuf,
    config_dir: PathBuf,
    data_dir: PathBuf,
    cache_dir: PathBuf,
    work_root: PathBuf,
}

struct BenchmarkSidecar {
    port: u16,
    child: Child,
    logs: Arc<Mutex<Vec<String>>>,
}

struct ClaudeBenchWorker {
    child: Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

#[derive(Serialize, Deserialize)]
struct WorkerRequest {
    model: String,
    prompt: String,
}

#[derive(Serialize, Deserialize)]
struct WorkerResponse {
    first_ms: Option<f64>,
    total_ms: Option<f64>,
    error: Option<String>,
}

pub fn run(cli: &Cli, opts: &BenchmarkOptions) -> anyhow::Result<()> {
    let style = console::stdout_style();
    let modes = selected_modes(opts.mode);

    println!("{}", style.title("openclaude benchmark"));
    println!();
    println!("{}", style.heading("config:"));
    println!(
        "  {}  {}",
        style.option("mode"),
        mode_names(&modes).join(", ")
    );
    println!("  {}  {}", style.option("model"), opts.model);
    println!("  {}  {}", style.option("iterations"), opts.iterations);
    println!("  {}  {}", style.option("warmups"), opts.warmups);
    println!();
    println!("{}", style.heading("progress:"));

    println!(
        "  {}",
        style.command("starting raw claude benchmark worker")
    );
    let mut claude_worker = start_claude_worker(cli)?;
    println!("  {}", style.command("preflighting raw claude CLI"));
    if let Err(err) = bench_claude(&mut claude_worker, opts) {
        if opts.skip_live {
            stop_claude_worker(&mut claude_worker);
            skip_live(err.to_string());
            return Ok(());
        }
        return Err(live_error(err));
    }

    println!("  {}", style.command("collecting claude samples"));
    warmup(opts.warmups, || bench_claude(&mut claude_worker, opts))?;
    let claude_summary = summarize(&run_samples(opts.iterations, || {
        bench_claude(&mut claude_worker, opts)
    })?);

    let mut sidecar = if modes.iter().any(|mode| {
        matches!(
            mode,
            BenchmarkMode::Translation | BenchmarkMode::OpencodeSession
        )
    }) {
        println!(
            "  {}  {}",
            style.command("starting benchmark HTTP server"),
            cli.base_url
        );
        Some(start_sidecar(cli, opts)?)
    } else {
        None
    };

    let mut results = Vec::new();

    for mode in modes {
        match mode {
            BenchmarkMode::All => unreachable!(),
            BenchmarkMode::Translation => {
                let sidecar_port = sidecar.as_ref().unwrap().port;
                println!("  {}", style.command("preflighting translation path"));
                if let Err(err) = bench_request_path(sidecar_port, opts) {
                    if opts.skip_live {
                        if let Some(sidecar) = sidecar.as_mut() {
                            stop_sidecar(sidecar);
                        }
                        skip_live(err.to_string());
                        return Ok(());
                    }
                    return Err(live_error(err));
                }
                println!("  {}", style.command("collecting translation samples"));
                warmup(opts.warmups, || bench_request_path(sidecar_port, opts))?;
                let summary = summarize(&run_samples(opts.iterations, || {
                    bench_request_path(sidecar_port, opts)
                })?);
                results.push(("translation", summary));
            }
            BenchmarkMode::OpencodeSession => {
                let sidecar_port = sidecar.as_ref().unwrap().port;
                let workspace = BenchmarkWorkspace::new()?;
                println!("  {}", style.command("preflighting opencode session"));
                if let Err(err) = bench_session(sidecar_port, cli, opts, &workspace) {
                    if opts.skip_live {
                        if let Some(sidecar) = sidecar.as_mut() {
                            stop_sidecar(sidecar);
                        }
                        skip_live(err.to_string());
                        return Ok(());
                    }
                    return Err(live_error(err));
                }
                println!("  {}", style.command("collecting opencode session samples"));
                warmup(opts.warmups, || {
                    bench_session(sidecar_port, cli, opts, &workspace)
                })?;
                let summary = summarize(&run_samples(opts.iterations, || {
                    bench_session(sidecar_port, cli, opts, &workspace)
                })?);
                results.push(("opencode-session", summary));
            }
        }
    }

    if let Some(sidecar) = sidecar.as_mut() {
        stop_sidecar(sidecar);
    }
    stop_claude_worker(&mut claude_worker);

    println!();
    println!("{}", style.heading("results:"));
    report(style, "claude", &claude_summary);
    for (label, summary) in &results {
        report(style, label, summary);
        report_overhead(style, label, &claude_summary, summary);
        assert_thresholds(opts, label, &claude_summary, summary)?;
    }

    Ok(())
}

fn selected_modes(mode: BenchmarkMode) -> Vec<BenchmarkMode> {
    match mode {
        BenchmarkMode::All => vec![BenchmarkMode::Translation, BenchmarkMode::OpencodeSession],
        other => vec![other],
    }
}

fn mode_names(modes: &[BenchmarkMode]) -> Vec<&'static str> {
    modes
        .iter()
        .map(|mode| match mode {
            BenchmarkMode::All => "all",
            BenchmarkMode::Translation => "translation",
            BenchmarkMode::OpencodeSession => "opencode-session",
        })
        .collect()
}

impl BenchmarkWorkspace {
    fn new() -> anyhow::Result<Self> {
        let temp = TempDir::new()?;
        let home_dir = temp.path().join("home");
        let config_dir = temp.path().join("config");
        let data_dir = temp.path().join("data");
        let cache_dir = temp.path().join("cache");
        let work_root = temp.path().join("work");
        std::fs::create_dir_all(&home_dir)?;
        std::fs::create_dir_all(&config_dir)?;
        std::fs::create_dir_all(&data_dir)?;
        std::fs::create_dir_all(&cache_dir)?;
        std::fs::create_dir_all(&work_root)?;
        Ok(Self {
            _temp: temp,
            home_dir,
            config_dir,
            data_dir,
            cache_dir,
            work_root,
        })
    }

    fn next_workdir(&self) -> anyhow::Result<PathBuf> {
        let workdir = self.work_root.join(Uuid::new_v4().to_string());
        std::fs::create_dir_all(&workdir)?;
        Ok(workdir)
    }

    fn apply_env(&self, command: &mut Command) {
        command
            .env("HOME", &self.home_dir)
            .env("XDG_CONFIG_HOME", &self.config_dir)
            .env("XDG_DATA_HOME", &self.data_dir)
            .env("XDG_CACHE_HOME", &self.cache_dir);
    }
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

fn spawn_logs<R: Read + Send + 'static>(pipe: R, logs: Arc<Mutex<Vec<String>>>) {
    thread::spawn(move || {
        let reader = BufReader::new(pipe);
        for line in reader.lines().map_while(Result::ok) {
            logs.lock().unwrap().push(line);
        }
    });
}

fn start_sidecar(cli: &Cli, opts: &BenchmarkOptions) -> anyhow::Result<BenchmarkSidecar> {
    let port = free_port();
    let bin = std::env::current_exe()?;
    let mut child = Command::new(bin)
        .arg("--claude-bin")
        .arg(&cli.claude_bin)
        .arg("--available-models")
        .arg(&opts.model)
        .arg("serve")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let logs = Arc::new(Mutex::new(Vec::new()));
    spawn_logs(child.stdout.take().unwrap(), logs.clone());
    spawn_logs(child.stderr.take().unwrap(), logs.clone());
    let sidecar = BenchmarkSidecar { port, child, logs };
    wait_ready(&sidecar)?;
    Ok(sidecar)
}

fn wait_ready(sidecar: &BenchmarkSidecar) -> anyhow::Result<()> {
    let client = Client::builder()
        .timeout(Duration::from_millis(250))
        .build()?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        if client
            .get(format!("http://127.0.0.1:{}/health", sidecar.port))
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
            .any(|line| line.contains("failed"))
        {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!(
        "server did not become ready: {:?}",
        *sidecar.logs.lock().unwrap()
    )
}

fn stop_sidecar(sidecar: &mut BenchmarkSidecar) {
    let _ = sidecar.child.kill();
    let _ = sidecar.child.wait();
}

fn warmup<F>(count: usize, mut sample: F) -> anyhow::Result<()>
where
    F: FnMut() -> anyhow::Result<Sample>,
{
    for _ in 0..count {
        let _ = sample()?;
    }
    Ok(())
}

fn bench_claude(worker: &mut ClaudeBenchWorker, opts: &BenchmarkOptions) -> anyhow::Result<Sample> {
    let request = WorkerRequest {
        model: opts.model.clone(),
        prompt: opts.prompt.clone(),
    };
    serde_json::to_writer(&mut worker.stdin, &request)?;
    worker.stdin.write_all(b"\n")?;
    worker.stdin.flush()?;

    let mut line = String::new();
    worker.stdout.read_line(&mut line)?;
    let response: WorkerResponse = serde_json::from_str(line.trim())?;
    if let Some(error) = response.error {
        anyhow::bail!(error);
    }
    Ok(Sample {
        first_ms: response
            .first_ms
            .ok_or_else(|| anyhow::anyhow!("claude benchmark worker produced no first_ms"))?,
        total_ms: response
            .total_ms
            .ok_or_else(|| anyhow::anyhow!("claude benchmark worker produced no total_ms"))?,
    })
}

fn bench_request_path(port: u16, opts: &BenchmarkOptions) -> anyhow::Result<Sample> {
    let client = Client::builder()
        .timeout(Duration::from_secs(300))
        .build()?;
    let start = Instant::now();
    let response = client
        .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
        .header("content-type", "application/json")
        .json(&json!({
            "model": opts.model,
            "messages": [{"role": "user", "content": opts.prompt}],
            "stream": true,
        }))
        .send()?;
    parse_stream(response, start, "openclaude request path")
}

fn parse_stream(response: Response, start: Instant, label: &str) -> anyhow::Result<Sample> {
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("{label} failed with {status}");
    }

    let mut reader = BufReader::new(response);
    let mut line = String::new();
    let mut first_ms = None;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim();
        if !trimmed.starts_with("data: ") {
            continue;
        }
        let data = &trimmed[6..];
        if data == "[DONE]" {
            break;
        }
        let value: Value = serde_json::from_str(data)?;
        if first_ms.is_none()
            && value["choices"][0]["delta"]["content"]
                .as_str()
                .is_some_and(|text| !text.is_empty())
        {
            first_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
        }
    }

    let total_ms = start.elapsed().as_secs_f64() * 1000.0;
    let first_ms = first_ms.ok_or_else(|| anyhow::anyhow!("{label} produced no content chunk"))?;
    Ok(Sample { first_ms, total_ms })
}

fn bench_session(
    port: u16,
    cli: &Cli,
    opts: &BenchmarkOptions,
    workspace: &BenchmarkWorkspace,
) -> anyhow::Result<Sample> {
    bench_session_from(Instant::now(), port, cli, opts, workspace)
}

fn bench_session_from(
    start: Instant,
    port: u16,
    cli: &Cli,
    opts: &BenchmarkOptions,
    workspace: &BenchmarkWorkspace,
) -> anyhow::Result<Sample> {
    let binary = std::env::current_exe()?;
    let workdir = workspace.next_workdir()?;
    let mut command = Command::new(binary);
    command
        .arg("--claude-bin")
        .arg(&cli.claude_bin)
        .arg("--opencode-bin")
        .arg(&cli.opencode_bin)
        .arg("--base-url")
        .arg(format!("http://127.0.0.1:{port}"))
        .arg("--available-models")
        .arg(&opts.model)
        .arg("--workdir")
        .arg(&workdir)
        .arg("bootstrap")
        .arg("run")
        .arg("--format")
        .arg("json")
        .arg("-m")
        .arg(format!("openclaude/{}", opts.model))
        .arg(&opts.prompt)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    workspace.apply_env(&mut command);
    let mut child = command.spawn()?;

    let stderr = child.stderr.take().unwrap();
    let stderr_handle = thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut text = String::new();
        let _ = reader.read_to_string(&mut text);
        text
    });

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    let mut first_ms = None;
    let mut finish_ms = None;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if first_ms.is_none()
            && value["type"].as_str() == Some("text")
            && value["part"]["text"]
                .as_str()
                .is_some_and(|text| !text.is_empty())
        {
            first_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
        }
        if finish_ms.is_none()
            && value["type"].as_str() == Some("step_finish")
            && value["part"]["reason"].as_str() == Some("stop")
        {
            finish_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
        }
    }

    let status = child.wait()?;
    let stderr = stderr_handle.join().unwrap_or_default();
    if !status.success() {
        anyhow::bail!("openclaude session failed with {status}: {stderr}");
    }

    let first_ms =
        first_ms.ok_or_else(|| anyhow::anyhow!("openclaude session produced no text event"))?;
    let total_ms = finish_ms.unwrap_or_else(|| start.elapsed().as_secs_f64() * 1000.0);
    Ok(Sample { first_ms, total_ms })
}

fn summarize(samples: &[Sample]) -> Summary {
    let mut first = samples
        .iter()
        .map(|sample| sample.first_ms)
        .collect::<Vec<_>>();
    let mut total = samples
        .iter()
        .map(|sample| sample.total_ms)
        .collect::<Vec<_>>();
    first.sort_by(f64::total_cmp);
    total.sort_by(f64::total_cmp);
    Summary {
        min_first_ms: first[0],
        median_first_ms: median(&first),
        avg_first_ms: average(&first),
        min_total_ms: total[0],
        median_total_ms: median(&total),
        avg_total_ms: average(&total),
    }
}

fn median(values: &[f64]) -> f64 {
    let mid = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

fn average(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn run_samples<F>(count: usize, mut sample: F) -> anyhow::Result<Vec<Sample>>
where
    F: FnMut() -> anyhow::Result<Sample>,
{
    let mut samples = Vec::with_capacity(count);
    for _ in 0..count {
        samples.push(sample()?);
    }
    Ok(samples)
}

fn report(style: console::Style, label: &str, summary: &Summary) {
    println!(
        "  {}  first min/median/avg = {:.1}/{:.1}/{:.1} ms, total min/median/avg = {:.1}/{:.1}/{:.1} ms",
        style.option(label),
        summary.min_first_ms,
        summary.median_first_ms,
        summary.avg_first_ms,
        summary.min_total_ms,
        summary.median_total_ms,
        summary.avg_total_ms,
    );
}

fn report_overhead(style: console::Style, label: &str, claude: &Summary, openclaude: &Summary) {
    println!(
        "  {}  {} first avg = {:.1} ms, total avg = {:.1} ms",
        style.option("overhead"),
        label,
        openclaude.avg_first_ms - claude.avg_first_ms,
        openclaude.avg_total_ms - claude.avg_total_ms,
    );
}

fn assert_thresholds(
    opts: &BenchmarkOptions,
    label: &str,
    claude: &Summary,
    openclaude: &Summary,
) -> anyhow::Result<()> {
    let first = openclaude.avg_first_ms - claude.avg_first_ms;
    let total = openclaude.avg_total_ms - claude.avg_total_ms;
    if let Some(max) = opts.max_first_ms {
        anyhow::ensure!(
            first <= max,
            "{label} first-content overhead {:.1} ms exceeded threshold {:.1} ms",
            first,
            max,
        );
    }
    if let Some(max) = opts.max_total_ms {
        anyhow::ensure!(
            total <= max,
            "{label} total overhead {:.1} ms exceeded threshold {:.1} ms",
            total,
            max,
        );
    }
    Ok(())
}

fn skip_live(reason: impl AsRef<str>) {
    let style = console::stdout_style();
    println!(
        "{}\n\n  {}  {}\n  {}  {}",
        style.heading("benchmark skipped:"),
        style.option("reason"),
        reason.as_ref(),
        style.option("hint"),
        "pass --skip-live only when intentionally opting out of live benchmarking"
    );
}

fn live_error(reason: impl std::fmt::Display) -> anyhow::Error {
    anyhow::anyhow!(
        "live benchmark failed; pass --skip-live to opt out when Claude auth or network access is unavailable: {}",
        reason
    )
}

fn start_claude_worker(cli: &Cli) -> anyhow::Result<ClaudeBenchWorker> {
    let binary = std::env::current_exe()?;
    let mut child = Command::new(binary)
        .arg("--claude-bin")
        .arg(&cli.claude_bin)
        .arg("benchmark-claude-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stderr = child.stderr.take().unwrap();
    let stderr_logs = Arc::new(Mutex::new(Vec::new()));
    spawn_logs(stderr, stderr_logs.clone());

    Ok(ClaudeBenchWorker {
        stdin: child.stdin.take().unwrap(),
        stdout: BufReader::new(child.stdout.take().unwrap()),
        child,
    })
}

fn stop_claude_worker(worker: &mut ClaudeBenchWorker) {
    let _ = worker.child.kill();
    let _ = worker.child.wait();
}

pub fn run_claude_worker(cli: &Cli) -> anyhow::Result<()> {
    let runtime = ClaudeCliRuntime::new(&cli.claude_bin, Vec::new());
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();
    let mut line = String::new();

    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: WorkerRequest = serde_json::from_str(trimmed)?;
        let response = match sample_claude_request(&runtime, &cli.claude_bin, &request) {
            Ok(sample) => WorkerResponse {
                first_ms: Some(sample.first_ms),
                total_ms: Some(sample.total_ms),
                error: None,
            },
            Err(err) => WorkerResponse {
                first_ms: None,
                total_ms: None,
                error: Some(err.to_string()),
            },
        };

        serde_json::to_writer(&mut writer, &response)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
    }

    Ok(())
}

fn sample_claude_request(
    runtime: &ClaudeCliRuntime,
    claude_bin: &PathBuf,
    req: &WorkerRequest,
) -> anyhow::Result<Sample> {
    let request = ProviderRequest {
        model: ProviderModel::claude(req.model.clone(), req.model.clone()),
        system_prompt: None,
        messages: vec![ProviderMessage {
            role: MessageRole::User,
            parts: vec![MessagePart::Text {
                text: req.prompt.clone(),
            }],
        }],
    };
    let prompt = build_claude_prompt(&request);
    let mut child = Command::new(claude_bin)
        .args(runtime.command_args(&request))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stderr = child.stderr.take().unwrap();
    let stderr_handle = thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut text = String::new();
        let _ = reader.read_to_string(&mut text);
        text
    });

    let start = Instant::now();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(prompt.user_prompt.as_bytes())?;

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    let mut first_ms = None;
    let mut finish_ms = None;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts = runtime.parse_stream_line(trimmed)?;
        if first_ms.is_none()
            && parts
                .iter()
                .any(|part| matches!(part, StreamPart::TextDelta(text) if !text.delta.is_empty()))
        {
            first_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
        }
        if finish_ms.is_none()
            && parts
                .iter()
                .any(|part| matches!(part, StreamPart::Finish { .. }))
        {
            finish_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
        }
    }

    let status = child.wait()?;
    let stderr = stderr_handle.join().unwrap_or_default();
    if !status.success() {
        anyhow::bail!("claude CLI failed with {status}: {stderr}");
    }

    Ok(Sample {
        first_ms: first_ms.ok_or_else(|| {
            anyhow::anyhow!("claude CLI produced no text delta for model {}", req.model)
        })?,
        total_ms: finish_ms.unwrap_or_else(|| start.elapsed().as_secs_f64() * 1000.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::blocking::Client;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn selects_all_modes() {
        assert_eq!(
            mode_names(&selected_modes(BenchmarkMode::All)),
            vec!["translation", "opencode-session"]
        );
        assert_eq!(
            mode_names(&selected_modes(BenchmarkMode::Translation)),
            vec!["translation"]
        );
    }

    #[test]
    fn threshold_failure() {
        let opts = BenchmarkOptions {
            mode: BenchmarkMode::Translation,
            model: "sonnet".into(),
            prompt: "hi".into(),
            iterations: 1,
            warmups: 0,
            skip_live: false,
            max_first_ms: Some(5.0),
            max_total_ms: Some(5.0),
        };
        let claude = Summary {
            min_first_ms: 10.0,
            median_first_ms: 10.0,
            avg_first_ms: 10.0,
            min_total_ms: 20.0,
            median_total_ms: 20.0,
            avg_total_ms: 20.0,
        };
        let openclaude = Summary {
            min_first_ms: 20.0,
            median_first_ms: 20.0,
            avg_first_ms: 20.0,
            min_total_ms: 30.0,
            median_total_ms: 30.0,
            avg_total_ms: 30.0,
        };

        let err = assert_thresholds(&opts, "translation", &claude, &openclaude)
            .unwrap_err()
            .to_string();
        assert!(err.contains("translation first-content overhead"));
    }

    #[test]
    fn parse_stream_reports_first_content() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let body = concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n",
                "data: [DONE]\n\n"
            );
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });

        let response = Client::new().get(format!("http://{addr}")).send().unwrap();
        let sample = parse_stream(response, Instant::now(), "label").unwrap();
        assert!(sample.first_ms >= 0.0);
        assert!(sample.total_ms >= sample.first_ms);
    }
}
