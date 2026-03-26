# Claude Code Reference

This file is the internal implementation reference for Claude Code CLI behavior that matters to `clyde`.

It exists so backend work can be grounded in stable project-local documentation instead of ad hoc memory or machine-specific source checkouts.

## Purpose in This Project

`clyde` treats Claude Code as the model-facing transport.

The backend should use Claude Code for:

- model execution
- streamed reasoning and text output
- tool-use emission
- stable CLI invocation

The backend should not treat Claude Code as the canonical owner of:

- session history
- tool execution state
- OpenCode memory or context
- frontend conversation ownership

## CLI Invocation Model

The current project uses the CLI in non-interactive print mode.

Core flags used by `clyde` today:

```bash
claude \
  --print \
  --model <model> \
  --permission-mode bypassPermissions \
  --dangerously-skip-permissions \
  --output-format stream-json \
  --verbose \
  --include-partial-messages \
  [--system-prompt <prompt>]
```

### Current Meanings

- `--print`
  - non-interactive mode
  - stdout becomes machine-readable output instead of a TUI
- `--model`
  - selects Claude Code's target model

`clyde` may also issue short non-interactive Claude probes during launch to determine which model aliases are actually available in the local Claude environment.

- precedence order is: manual override -> cached discovery -> live probe -> static fallback
- manual override comes from `CLYDE_AVAILABLE_MODELS` / `--available-models`
- cache defaults to `$XDG_CACHE_HOME/clyde/models.json` or `~/.cache/clyde/models.json`
- `CLYDE_MODEL_CACHE` can override the cache file path
- current live probe targets are `haiku`, `sonnet`, and `opus`
- successful probes become the OpenCode-visible model list
- if all probes fail, `clyde` falls back to the static alias set

- `--permission-mode bypassPermissions`
  - disables Claude Code's interactive approval flow for this transport session
- `clyde` relies on OpenCode-owned tool execution instead of Claude-local permission UX
- `--dangerously-skip-permissions`
  - ensures Claude does not narrate or wait on local permission prompts
  - this is intentional here because Claude Code is acting as a stateless transport, not the canonical tool runner
- `--output-format stream-json`
  - newline-delimited JSON stream
- this is the most important mode for `clyde`
- `--verbose`
  - needed because the streamed event forms we care about appear in the verbose output path
- `--include-partial-messages`
  - enables token/delta-style streamed assistant output rather than only final assistant aggregates
- `--system-prompt`
  - allows backend-controlled system injection
  - should be derived from OpenCode-owned history, not backend-owned memory

## Important Known Constraints

### Claude Code Is Not the Conversation Owner

For this project, Claude Code must be treated as a stateless transport target.

That means:

- every backend request should be reconstructible from OpenCode-owned history
- no backend-owned suspended Claude session should be required
- no backend-specific resume IDs should be necessary for correctness

### Tool Use Is Emitted, Not Owned

Claude Code can emit tool-use blocks, but in `clyde` these should be translated into provider-like events and handed back to the frontend layer.

The backend should not become the canonical owner of tool continuation state.

### Prompt Replay Is Expected

If a turn needs to continue after a tool result, the stateless target architecture is:

- OpenCode/plugin reconstructs full history
- backend translates that full history into a Claude Code prompt
- Claude Code receives a fresh one-shot request

This is less like restoring an internal Claude session and more like deterministic replay.

## Stream-JSON Structures Relevant to clyde

### Top-Level Chunk Kinds

Observed and modeled in the codebase:

- `stream_event`
- `assistant`
- `result`

### `stream_event`

Modeled fields:

- `event.type`
- `event.index`
- `event.content_block`
- `event.delta`

Important event types:

- `content_block_start`
- `content_block_delta`
- `content_block_stop`

### Content Blocks

Modeled block types:

- `thinking`
- `text`
- `tool_use`

#### Thinking Block

```json
{
  "type": "thinking",
  "thinking": "..."
}
```

#### Text Block

```json
{
  "type": "text",
  "text": "..."
}
```

#### Tool-Use Block

```json
{
  "type": "tool_use",
  "id": "toolu_...",
  "name": "Read",
  "input": {}
}
```

### Deltas

Modeled delta types:

- `thinking_delta`
- `text_delta`
- `input_json_delta`

#### Thinking Delta

```json
{
  "type": "thinking_delta",
  "thinking": "partial reasoning"
}
```

#### Text Delta

```json
{
  "type": "text_delta",
  "text": "partial text"
}
```

#### Input JSON Delta

```json
{
  "type": "input_json_delta",
  "partial_json": "{\"file_path\""
}
```

This matters because tool input often arrives incrementally and must be accumulated until `content_block_stop`.

## Translation Rules Currently Assumed by clyde

### Reasoning

- `thinking` / `thinking_delta` -> reasoning lifecycle parts
- start and stop are tracked by content-block boundaries

### Text

- `text` / `text_delta` -> text lifecycle parts

### Tool Use

- `tool_use` block start -> tool input start
- `input_json_delta` -> tool input delta
- block stop -> tool input end + final tool call

## Backend Implementation Guidance

When changing backend code, preserve these rules:

1. Do not introduce backend-owned session IDs for Claude continuity.
2. Do not persist suspended Claude tool state as the source of truth.
3. Prefer replay from OpenCode-owned canonical history.
4. Keep stream translation typed and explicit.
5. Do not collapse tool-use deltas into opaque blobs if the frontend can use richer lifecycle data.

## Current Project Files That Implement This Behavior

- `src/claude/cli.rs`
  - flag construction and process target
- `src/claude/stream.rs`
  - deserialization of Claude Code stream-JSON chunks
- `src/claude/translate.rs`
  - stateful translation from Claude chunks to provider stream parts
- `src/claude/runtime.rs`
  - subprocess execution and one-shot translation runtime
- `src/claude/prompt.rs`
  - prompt replay from canonical history

## Things to Watch For When Editing This Project

- shell quoting in commit messages can eat backticks; quote carefully
- CLI output formats can drift across Claude Code versions; keep parsing narrow and tested
- avoid depending on undocumented continuity semantics when the project goal is a stateless backend
- if a new feature appears to require backend-owned memory, prefer enriching canonical request history first

## Recommended Future Checks

- verify whether newer Claude Code releases expose additional machine-readable fields useful for reasoning/tool provenance
- confirm whether model identifiers or stream event names changed across CLI versions
- keep this file updated whenever the backend starts relying on new Claude CLI behavior
