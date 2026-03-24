use crate::claude::{build_claude_prompt, ClaudeCliRuntime};
use crate::cli::{BenchmarkMode, BenchmarkOptions, Cli};
use crate::console;
use crate::provider::{
    MessagePart, MessageRole, ProviderMessage, ProviderModel, ProviderRequest, StreamPart,
};
use reqwest::blocking::{Client, Response};
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

pub fn run(cli: &Cli, opts: &BenchmarkOptions) -> anyhow::Result<()> {
    let style = console::stdout_style();
    let request = request(opts);
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

    println!("  {}", style.command("preflighting raw claude CLI"));
    if let Err(err) = bench_claude(cli, opts, &request) {
        if opts.skip_live {
            skip_live(err.to_string());
            return Ok(());
        }
        return Err(live_error(err));
    }

    println!("  {}", style.command("collecting claude samples"));
    warmup(opts.warmups, || bench_claude(cli, opts, &request))?;
    let claude_summary = summarize(&run_samples(opts.iterations, || {
        bench_claude(cli, opts, &request)
    })?);

    let mut sidecar = if modes.iter().any(|mode| {
        matches!(
            mode,
            BenchmarkMode::WarmSession | BenchmarkMode::RequestPath
        )
    }) {
        println!("  {}", style.command("starting benchmark sidecar"));
        Some(start_sidecar(cli, opts)?)
    } else {
        None
    };

    let mut results = Vec::new();

    for mode in modes {
        match mode {
            BenchmarkMode::All => unreachable!(),
            BenchmarkMode::RequestPath => {
                let sidecar_port = sidecar.as_ref().unwrap().port;
                println!("  {}", style.command("preflighting request path"));
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
                println!("  {}", style.command("collecting request path samples"));
                warmup(opts.warmups, || bench_request_path(sidecar_port, opts))?;
                let summary = summarize(&run_samples(opts.iterations, || {
                    bench_request_path(sidecar_port, opts)
                })?);
                results.push(("request-path", summary));
            }
            BenchmarkMode::WarmSession => {
                let sidecar_port = sidecar.as_ref().unwrap().port;
                let workspace = BenchmarkWorkspace::new()?;
                println!("  {}", style.command("preflighting warm fresh session"));
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
                println!(
                    "  {}",
                    style.command("collecting warm fresh session samples")
                );
                warmup(opts.warmups, || {
                    bench_session(sidecar_port, cli, opts, &workspace)
                })?;
                let summary = summarize(&run_samples(opts.iterations, || {
                    bench_session(sidecar_port, cli, opts, &workspace)
                })?);
                results.push(("warm-session", summary));
            }
        }
    }

    if let Some(sidecar) = sidecar.as_mut() {
        stop_sidecar(sidecar);
    }

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
        BenchmarkMode::All => vec![BenchmarkMode::WarmSession, BenchmarkMode::RequestPath],
        other => vec![other],
    }
}

fn mode_names(modes: &[BenchmarkMode]) -> Vec<&'static str> {
    modes
        .iter()
        .map(|mode| match mode {
            BenchmarkMode::All => "all",
            BenchmarkMode::WarmSession => "warm-session",
            BenchmarkMode::RequestPath => "request-path",
        })
        .collect()
}

fn request(opts: &BenchmarkOptions) -> ProviderRequest {
    ProviderRequest {
        model: ProviderModel::claude(opts.model.clone(), opts.model.clone()),
        system_prompt: None,
        messages: vec![ProviderMessage {
            role: MessageRole::User,
            parts: vec![MessagePart::Text {
                text: opts.prompt.clone(),
            }],
        }],
    }
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

fn bench_claude(
    cli: &Cli,
    opts: &BenchmarkOptions,
    request: &ProviderRequest,
) -> anyhow::Result<Sample> {
    let runtime = ClaudeCliRuntime::new(&cli.claude_bin, vec![request.model.clone()]);
    let prompt = build_claude_prompt(request);
    let mut child = Command::new(&cli.claude_bin)
        .args(runtime.command_args(request))
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
    }

    let status = child.wait()?;
    let stderr = stderr_handle.join().unwrap_or_default();
    if !status.success() {
        anyhow::bail!("claude CLI failed with {status}: {stderr}");
    }

    let total_ms = start.elapsed().as_secs_f64() * 1000.0;
    let first_ms = first_ms.ok_or_else(|| {
        anyhow::anyhow!("claude CLI produced no text delta for model {}", opts.model)
    })?;
    Ok(Sample { first_ms, total_ms })
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
        .arg(format!("http://127.0.0.1:{port}/v1"))
        .arg("--available-models")
        .arg(&opts.model)
        .arg("--workdir")
        .arg(&workdir)
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
