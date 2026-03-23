# openclaude

Translation layer between OpenCode and Claude Code, using a plugin-based frontend and a native translation backend.

## Goal

Use Claude Code CLI as the model transport while preserving OpenCode-owned behavior for:

- tool execution
- subagents and background tasks
- reasoning/thinking parts
- session rendering and tool lifecycle

The purpose of this project is to fit more cleanly within Anthropic's guidelines for model usage outside of Claude Code while still preserving the OpenCode experience.

## No-Patch Integration Direction

`openclaude` is intentionally being built as a translation layer between OpenCode and Claude Code rather than as a patch inside OpenCode itself.

The intended shape is:

- `openclaude` owns Claude CLI execution, stream translation, session orchestration, and bridge/service APIs
- a plugin-based frontend can talk to `openclaude` over a stable protocol
- provider routing should be declared in OpenCode configuration, while the plugin stays thin and handles auth and request shaping
- OpenCode itself remains unmodified on our side

This means the project is optimizing for a no-patch, plugin-based integration surface rather than private hooks into OpenCode internals.

Based on current plugin research, the plugin layer should not try to register a brand-new provider runtime by itself. The expected pattern is:

- provider routing and base URL in config
- a thin plugin frontend for auth, headers, params, and message transforms
- `openclaude` as the stateless native translation backend

True dynamic provider registration from a plugin is not currently supported by the verified OpenCode code surface.

The current direction uses wrapper-managed bootstrap instead of editing user config.

When you run `openclaude`, it now:

- prepares bootstrap config entries for the `openclaude` provider and plugin
- merges them into the launched process through `OPENCODE_CONFIG_CONTENT`
- starts `opencode` as a wrapper command replacement

This keeps the user's normal `opencode` setup unchanged while making `openclaude` behave like a preconfigured entrypoint.

## Status

The project currently provides:

- a library-first Rust layout
- typed provider stream parts
- typed Claude stream-JSON parsing
- provider runtime and session orchestration layers
- adapter and bridge entrypoints
- a standalone service core for start/resume flows
- a thin plugin scaffold under `plugin/` for OpenCode-facing hooks
- tracked internal reference docs under `docs/`
- an optional local OpenCode checkout under `opencode-reference/` for direct source inspection
- a stateless complete-request protocol that expects full OpenCode-owned history on every call

## Architecture

`openclaude` now has three separate responsibilities.

### Backend

The Rust backend is the translation layer.

- `openclaude serve` starts an OpenAI-compatible HTTP server
- OpenCode sends requests to that server as a provider endpoint
- the backend translates requests into Claude Code CLI execution
- the backend translates Claude Code output back into OpenCode-facing responses
- the backend stays stateless and does not own canonical session state

### Plugin

The plugin is a thin runtime shim.

- it declares auth metadata for the `openclaude` provider
- it adds request headers like session ID and agent name
- it adds small provider-specific request params
- it does not register providers dynamically
- it does not own transport or session logic

### Bootstrap Wrapper

The wrapper is what bare `openclaude` does by default.

- it prepares temporary bootstrap config for the launched process
- it injects the `openclaude` provider entry
- it injects the local plugin entry
- it launches the real `opencode` binary
- it leaves the user's normal OpenCode config files unchanged

### Runtime Flow

1. The user runs `openclaude`
2. `openclaude` builds bootstrap config for the process
3. `openclaude` sets `OPENCODE_CONFIG_CONTENT`
4. `openclaude` launches `opencode`
5. OpenCode loads its usual config sources, then merges the injected inline config
6. OpenCode loads the injected plugin and provider entry
7. The plugin shapes requests at runtime
8. OpenCode sends provider traffic to `openclaude serve`
9. The backend translates to Claude Code CLI and returns responses

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

## Plugin Frontend

The intended frontend lives in `plugin/` and should stay thin.

Its job is to:

- integrate with OpenCode's plugin hooks
- handle auth, headers, params, and message transforms
- forward full history to the Rust backend

It should not reimplement backend transport or session logic that already belongs in `openclaude`.

The wrapper command is responsible for loading the plugin automatically.

## Commands

```bash
cargo fmt
cargo test
cargo build
cargo run -- help
cargo run --
cargo run -- serve
```

For the plugin scaffold:

```bash
cd plugin
npm install
npm run check
```
