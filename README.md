# openclaude

Native OpenCode provider backend for Claude Code CLI.

## Goal

Use Claude Code CLI as the model transport while preserving OpenCode-owned behavior for:

- tool execution
- subagents and background tasks
- reasoning/thinking parts
- session rendering and tool lifecycle

## Status

Early scaffold. The project currently provides:

- a library-first Rust layout
- typed provider stream parts
- typed Claude stream-json parsing
- documented integration targets from the local OpenCode source tree

## Commands

```bash
cargo fmt
cargo test
cargo build
cargo run -- --help
```
