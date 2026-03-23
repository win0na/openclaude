# openclaude

translation layer between OpenCode and Claude Code, using a plugin-based frontend and a native translation backend.

## goal

use Claude Code CLI as the model transport while preserving OpenCode-owned behavior for:

- tool execution
- subagents and background tasks
- reasoning/thinking parts
- session rendering and tool lifecycle

the purpose of this project is to fit more cleanly within anthropic's guidelines for model usage outside of Claude Code while still preserving the OpenCode experience.

## no-patch integration direction

`openclaude` is intentionally being built as a translation layer between OpenCode and Claude Code rather than as a patch inside OpenCode itself.

the intended shape is:

- `openclaude` owns Claude CLI execution, stream translation, session orchestration, and bridge/service APIs
- a plugin-based frontend can talk to `openclaude` over a stable protocol
- provider routing should be declared in OpenCode configuration, while the plugin stays thin and handles auth and request shaping
- OpenCode itself remains unmodified on our side

this means the project is optimizing for a no-patch, plugin-based integration surface rather than private hooks into OpenCode internals

based on current plugin research, the plugin layer should not try to register a brand-new provider runtime by itself. the expected pattern is:

- provider routing and base URL in config
- a thin plugin frontend for auth, headers, params, and message transforms
- `openclaude` as the stateless native translation backend

## status

the project currently provides:

- a library-first Rust layout
- typed provider stream parts
- typed Claude stream-json parsing
- provider runtime and session orchestration layers
- adapter and bridge entrypoints
- a standalone service core for start/resume flows
- a project-local OpenCode reference checkout under `opencode-reference/` after initialization
- a stateless complete-request protocol that expects full OpenCode-owned history on every call

## reference checkout

use `cargo run -- reference` to create or refresh a local OpenCode reference checkout in `opencode-reference/` at the project root.

that checkout is ignored by git and exists only as local integration context for this project.

## commands

```bash
cargo fmt
cargo test
cargo build
cargo run -- --help
```
