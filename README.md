# openclaude

standalone bridge and runtime for Claude Code CLI that is designed to integrate with OpenCode without patching the OpenCode codebase.

## Goal

Use Claude Code CLI as the model transport while preserving OpenCode-owned behavior for:

- tool execution
- subagents and background tasks
- reasoning/thinking parts
- session rendering and tool lifecycle

## no-patch integration direction

`openclaude` is intentionally being built as a standalone runtime, bridge, and service layer rather than as a patch inside OpenCode itself.

the intended shape is:

- `openclaude` owns Claude CLI execution, stream translation, session orchestration, and bridge/service APIs
- a thin external integration layer can talk to `openclaude` over a stable protocol
- OpenCode itself remains unmodified on our side

this means the project is optimizing for a no-patch integration surface, not for private hooks into OpenCode internals

## status

the project currently provides:

- a library-first Rust layout
- typed provider stream parts
- typed Claude stream-json parsing
- provider runtime and session orchestration layers
- adapter and bridge entrypoints
- a standalone service core for start/resume flows
- a project-local OpenCode reference checkout under `opencode-reference/` after initialization

## reference checkout

Use `cargo run -- init` to create or refresh a local OpenCode reference checkout in `opencode-reference/` at the project root.

That checkout is ignored by git and exists only as local integration context for this project.

## commands

```bash
cargo fmt
cargo test
cargo build
cargo run -- --help
```
