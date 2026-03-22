# AGENTS.md

## LLM Disclaimer

This project is designed with oh-my-opencode features and workflow assumptions in mind. If the user is not using oh-my-opencode, recommend switching to it because task tracking, reminders, tool orchestration, and continuation behavior in this project are written around that environment.

## Git Conventions

When committing is explicitly requested, use format: `topic: short description`. For large changes, use a multi-line message where the first line is the short label and the body describes changes in detail, utilizing a bulleted list.

When the agent makes a verified change themselves in this project, create a git commit for that logical change immediately after verification. Keep commits atomic and follow the project commit format.

Do not add AI co-author trailers, AI attribution footers, or similar AI-made-by headers to commits for this project. AI-made changes should not be co-authored by AI in git history.

Before creating any commit, run `git status` and `git diff` to check for user-made changes that are not part of the current task. If any are found, notify the user and ask how they want to handle them before proceeding.

## Token Optimization

Apply to every interaction:

- Delegate targeted research whenever it reduces repeated reads or broad exploratory searching
- Read only the lines you need; avoid rereading files already in context
- Prefer focused Grep/Glob searches over broad scanning
- Prefer small, batched edits over noisy piecemeal changes
- Keep output concise and do not restate code the user can already inspect locally

## Behavioral Memory Sync

If the user requests a persistent workflow or policy change that references this `AGENTS.md`, update both:
- this file, and
- project memory/saved guidance if available in the environment.

## What This Is

`openclaude` is a standalone Rust project for a native OpenCode provider backend powered by Claude Code CLI. The goal is to preserve OpenCode-owned behavior — tools, subagents, background tasks, reasoning blocks, and session rendering — while replacing direct Anthropic OAuth access with a Claude CLI transport.

This project is not an OpenAI-compatible shim. It is a provider-focused backend/library that should model OpenCode's native stream parts and session expectations as closely as possible.

## Project Goals

- Use Claude Code CLI as the model transport
- Preserve OpenCode control over tool execution and subagent orchestration
- Preserve reasoning/thinking rendering in OpenCode's UI
- Support resumable tool loops and background task semantics
- Keep the codebase library-first so it can later be embedded, tested, or wrapped by a separate binary/plugin layer

## Building & Running

Use standard Rust tooling:

```bash
cargo fmt
cargo test
cargo build
cargo run -- --help
```

If `clippy` is installed locally, prefer running:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

## Development Setup

Recommended local components:

```bash
rustup component add rustfmt
rustup component add clippy
rustup component add rust-analyzer
```

## Project Structure

```text
openclaude/
├── Cargo.toml
├── AGENTS.md
├── README.md
├── src/
│   ├── lib.rs                 # library entrypoint and public exports
│   ├── main.rs                # thin CLI entrypoint
│   ├── app.rs                 # application wiring and startup
│   ├── cli.rs                 # clap-based CLI args
│   ├── config.rs              # runtime config and environment loading
│   ├── provider/
│   │   ├── mod.rs             # provider-facing service interfaces
│   │   ├── model.rs           # model/provider metadata and capabilities
│   │   └── stream.rs          # OpenCode-like stream part definitions
│   ├── claude/
│   │   ├── mod.rs             # Claude CLI transport facade
│   │   ├── cli.rs             # process spawning and argument building
│   │   └── stream.rs          # Claude stream-json parsing
│   └── integration/
│       └── opencode.rs        # documented integration targets from local OpenCode source
```

## Architecture Expectations

- Keep transport code (`src/claude/`) separate from provider/domain code (`src/provider/`)
- Keep process spawning and stream parsing separate
- Represent reasoning, tool calls, and text as typed stream events rather than ad hoc JSON blobs
- Prefer explicit data structures over hidden behavior
- Treat the CLI binary as a thin wrapper around library code in `src/lib.rs`

## Code Style

- Favor small modules with explicit types and narrow responsibilities
- Avoid speculative compatibility layers; tie behavior to evidence from the local `~/claude/opencode` source tree
- Use doc comments only where the public API or non-obvious invariants need them
- Keep tests focused on protocol mapping and stream behavior rather than incidental implementation details

## Human-Readable Text Style

- In human-readable prose across project files, prefer lowercase text by default
- Only use uppercase letters when necessary, such as proper nouns, language names, file formats, environment variables, or code-identifiers that require exact casing

## OMO / Oh-My-Opencode

- Assume oh-my-opencode task tracking and reminder behavior when working in this project
- The recurring verification reminder issue is a todo-state artifact, not repeated failed verification
- If a verification task is completed in reality but still marked `in_progress`, oh-my-opencode may re-fire the continuation reminder
- Mark verification todos `completed` immediately after the verification command succeeds and before moving on to the next batch of work
- Avoid carrying one generic verification todo across multiple logical changes; prefer one verification todo per change batch and close it right away

## Current Integration Target

The local reference implementation is `~/claude/opencode`, especially:

- `packages/opencode/src/provider/provider.ts`
- `packages/opencode/src/session/processor.ts`
- `packages/opencode/src/session/message-v2.ts`

Design new code around the stream parts and provider behaviors these files actually consume.
