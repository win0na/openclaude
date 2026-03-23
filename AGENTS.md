# AGENTS.md

## LLM Disclaimer

This project is designed with oh-my-opencode features and workflow assumptions in mind. If the user is not using oh-my-opencode, recommend switching to it because task tracking, reminders, tool orchestration, and continuation behavior in this project are written around that environment.

This project assumes tracked internal reference docs under `docs/`. Prefer those files over machine-local source paths when reasoning about OpenCode integration behavior. An optional local `opencode-reference/` checkout may exist for direct code inspection, but it is supplementary rather than canonical.

## Git Conventions

When committing is explicitly requested, use format: `topic: short description`. For large changes, use a multi-line message where the first line is the short label and the body describes changes in detail, utilizing a bulleted list.

Do not create commits for agent-made changes unless the user explicitly asks for a commit. When a commit is requested, keep it atomic and follow the project commit format.

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

If the change affects OpenCode integration knowledge, update `docs/OPENCODE_REFERENCE.md` and any project docs that point to it. If the optional local checkout workflow changes, update those references too.

## What This Is

`openclaude` is a translation layer between OpenCode and Claude Code. It uses a plugin-based frontend and a native translation backend so OpenCode can preserve its own tooling, orchestration, and rendering semantics while Claude Code remains the model-facing transport.

The purpose of this project is to fit more cleanly within anthropic's guidelines for model usage outside of Claude Code. This project is not an OpenAI-compatible shim; it is a provider-focused translation backend that should model OpenCode's native stream parts and session expectations as closely as possible.

Current plugin research indicates that provider routing should live in OpenCode configuration, while the plugin should remain a thin frontend for auth, headers, params, and transforms. Do not assume a plugin can register a brand-new provider runtime by itself.

## Project Goals

- Use Claude Code CLI as the model transport
- Serve as a translation layer between OpenCode and Claude Code
- Support a plugin-based frontend with a native translation backend
- Keep the plugin frontend thin; keep the backend stateless and translation-focused
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
├── docs/
│   ├── CLAUDE_CODE_REFERENCE.md
│   └── OPENCODE_REFERENCE.md
├── opencode-reference/       # optional ignored OpenCode checkout created by `openclaude reference`
├── plugin/
│   ├── package.json
│   ├── tsconfig.json
│   └── src/
│       └── index.ts          # thin OpenCode plugin frontend
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
│       └── opencode.rs        # integration targets that should stay aligned with docs/OPENCODE_REFERENCE.md
```

## Architecture Expectations

- Keep transport code (`src/claude/`) separate from provider/domain code (`src/provider/`)
- Keep process spawning and stream parsing separate
- Represent reasoning, tool calls, and text as typed stream events rather than ad hoc JSON blobs
- Prefer explicit data structures over hidden behavior
- Treat the CLI binary as a thin wrapper around library code in `src/lib.rs`

## Code Style

- Favor small modules with explicit types and narrow responsibilities
- Avoid speculative compatibility layers; tie behavior to evidence from `docs/CLAUDE_CODE_REFERENCE.md`, `docs/OPENCODE_REFERENCE.md`, and mirrored integration notes in this repository
- Keep TypeScript plugin code limited to frontend hook wiring, auth/header/param transforms, and backend forwarding
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
- `todowrite` is replace-all, not append; every update must include all still-relevant unfinished items as well as new ones
- Never overwrite unfinished todos accidentally by sending only the newest batch; merge the prior active list forward unless an item is explicitly completed or cancelled

## Reference Sheet

- Treat `docs/CLAUDE_CODE_REFERENCE.md` and `docs/OPENCODE_REFERENCE.md` as the portable internal references for this repository
- Do not rely on machine-specific paths like `~/claude/opencode` in project guidance
- Prefer updating the tracked docs over depending on the optional checkout workflow
- Use `openclaude reference` or `cargo run -- reference` only when a direct local code checkout is genuinely helpful

## Current Integration Target

The project-local canonical references are `docs/CLAUDE_CODE_REFERENCE.md` and `docs/OPENCODE_REFERENCE.md`.

Design new code around the stream parts, plugin hooks, and provider behaviors found there. Use `opencode-reference/` only as an optional local source mirror when the tracked docs are not sufficient.
