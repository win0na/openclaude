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
- a thin plugin scaffold under `plugin/` for OpenCode-facing hooks
- tracked internal reference docs under `docs/`
- an optional local OpenCode checkout under `opencode-reference/` for direct source inspection
- a stateless complete-request protocol that expects full OpenCode-owned history on every call

## reference docs

Use the tracked reference docs in `docs/` when implementing backend or integration changes:

- `docs/CLAUDE_CODE_REFERENCE.md`
- `docs/OPENCODE_REFERENCE.md`

## optional local code reference

If you want a direct source checkout for inspection, use:

```bash
cargo run -- reference
```

This recreates or refreshes a gitignored `opencode-reference/` checkout at the project root.

The tracked docs in `docs/` remain the canonical portable references; the checkout is optional and local-only.

## plugin frontend

The intended frontend lives in `plugin/` and should stay thin.

Its job is to:

- integrate with OpenCode's plugin hooks
- handle auth, headers, params, and message transforms
- forward full history to the Rust backend

It should not reimplement backend transport or session logic that already belongs in `openclaude`.

## commands

```bash
cargo fmt
cargo test
cargo build
cargo run -- --help
```

For the plugin scaffold:

```bash
cd plugin
npm install
npm run check
```
