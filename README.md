# clyde

Translation layer between OpenCode and Claude Code, using a bootstrap wrapper plus a native translation backend.

## Goal

Use Claude Code CLI as the model transport while preserving OpenCode-owned behavior for:

- tool execution
- subagents and background tasks
- reasoning/thinking parts
- session rendering and tool lifecycle

The purpose of this project is to fit more cleanly within Anthropic's guidelines for model usage outside of Claude Code while still preserving the OpenCode experience.

## No-Patch Integration Direction

`clyde` is intentionally being built as a translation layer between OpenCode and Claude Code rather than as a patch inside OpenCode itself.

The intended shape is:

- `clyde` owns Claude CLI execution, stream translation, session orchestration, and bridge/service APIs
- OpenCode can talk to `clyde` over a standard provider configuration
- provider routing should be declared in OpenCode configuration and wrapper-managed bootstrap
- OpenCode itself remains unmodified on our side

This means the project is optimizing for a no-patch, config-and-provider integration surface rather than private hooks into OpenCode internals.

Based on current provider research, the expected pattern is:

- provider routing and base URL in config
- `clyde` as the stateless native translation backend

True dynamic provider registration is not required for the current wrapper-managed bootstrap flow.

The current direction uses wrapper-managed bootstrap instead of editing user config.

When you run `clyde`, it now:

- prepares bootstrap config entries for the `clyde` provider
- merges them into the launched process through `OPENCODE_CONFIG_CONTENT`
- starts `opencode` as a wrapper command replacement

This keeps the user's normal `opencode` setup unchanged while making `clyde` behave like a preconfigured entrypoint.

## Status

The project currently provides:

- a library-first Rust layout
- typed provider stream parts
- typed Claude stream-JSON parsing
- provider runtime and session orchestration layers
- adapter and bridge entrypoints
- a standalone service core for start/resume flows
- tracked internal reference docs under `docs/`
- an optional local OpenCode checkout under `opencode-reference/` for direct source inspection
- a stateless complete-request protocol that expects full OpenCode-owned history on every call

## Architecture

`clyde` now has two separate responsibilities.

### Backend

The Rust backend is the translation layer.

- `clyde serve` starts an OpenAI-compatible HTTP server
- OpenCode sends requests to that server as a provider endpoint
- the backend translates requests into Claude Code CLI execution
- the backend translates Claude Code output back into OpenCode-facing responses
- the backend stays stateless and does not own canonical session state

### Bootstrap Wrapper

The wrapper is what bare `clyde` does by default.

- it prepares temporary bootstrap config for the launched process
- it injects the `clyde` provider entry
- it launches the real `opencode` binary
- it leaves the user's normal OpenCode config files unchanged

### Runtime Flow

1. The user runs `clyde`
2. `clyde` builds bootstrap config for the process
3. `clyde` sets `OPENCODE_CONFIG_CONTENT`
4. `clyde` launches `opencode`
5. OpenCode loads its usual config sources, then merges the injected inline config
6. OpenCode loads the injected provider entry
7. OpenCode sends provider traffic to `clyde serve`
8. The backend translates to Claude Code CLI and returns responses

## Reference Docs

Use the tracked reference docs in `docs/` when implementing backend or integration changes:

- `docs/CLAUDE_CODE_REFERENCE.md`
- `docs/OPENCODE_REFERENCE.md`

## Optional Local Code Reference

If you want a direct source checkout for inspection, use:

```bash
cargo run -- reference
```

This recreates or refreshes a gitignored `opencode-reference/` checkout at the project root.

The tracked docs in `docs/` remain the canonical portable references; the checkout is optional and local-only.

## Commands

```bash
cargo fmt
cargo test
cargo build
cargo run -- help
cargo run --
cargo run -- -c run hello
cargo run -- alias
cargo run -- bootstrap
cargo run -- serve
```

Bare `clyde` now starts the local Clyde HTTP server in the background and then launches OpenCode against it. Use `clyde bootstrap` to launch the bootstrap/client path without starting the server, and `clyde serve` to run only the provider server.

Use `clyde -c ...` to forward explicit OpenCode arguments through the default bundled-launch flow. Use `clyde alias` to install a shell-level `opencode` wrapper for your active shell that forwards to `clyde -c "$@"`.

The default local server URL is `http://127.0.0.1:43123`. Clyde automatically uses the provider API under `/v1` internally.

## Claude CLI Requirements

`clyde` can start even when the local `claude` CLI is missing, unauthenticated, or offline. In that case it falls back to the default Claude model catalog and logs a warning instead of silently assuming live Claude access is working.

## Benchmark

`clyde benchmark` compares raw `claude` CLI latency against `clyde` with translation-only benchmarking as the default path, while keeping OpenCode session overhead as a separate integration metric.

```bash
clyde benchmark
clyde benchmark --mode translation
clyde benchmark --mode opencode-session
clyde benchmark --iterations 10 --warmups 1
clyde benchmark --model sonnet --max-first-ms 250 --max-total-ms 300
```

Benchmark modes:

- `all`: run every mode below
- `translation`: default mode; measure only the `clyde serve` translation path against raw `claude`
- `opencode-session`: reuse one prepared benchmark workspace and one long-lived sidecar, but create a fresh OpenCode session for each sample

The benchmark assumes a real `claude` CLI environment with valid auth and network access. Missing `claude`, invalid Claude auth, or lack of network access is treated as a failure by default.

Pass `--skip-live` only when you intentionally want to opt out of live benchmarking on a machine without Claude access.

You can enforce latency budgets with:

- `--max-first-ms <MAX_FIRST_MS>`
- `--max-total-ms <MAX_TOTAL_MS>`
