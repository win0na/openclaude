# claude code reference

This file is the internal implementation reference for Claude Code CLI behavior that matters to `openclaude`.

It exists so backend work can be grounded in stable project-local documentation instead of ad hoc memory or machine-specific source checkouts.

## purpose in this project

`openclaude` treats Claude Code as the model-facing transport.

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

## CLI invocation model

The current project uses the CLI in non-interactive print mode.

Core flags used by `openclaude` today:

```bash
claude \
  --print \
  --model <model> \
  --output-format stream-json \
  --verbose \
  --include-partial-messages \
  [--system-prompt <prompt>]
```

### current meanings

- `--print`
  - non-interactive mode
  - stdout becomes machine-readable output instead of a TUI
- `--model`
  - selects Claude Code's target model
- `--output-format stream-json`
  - newline-delimited JSON stream
  - this is the most important mode for `openclaude`
- `--verbose`
  - needed because the streamed event forms we care about appear in the verbose output path
- `--include-partial-messages`
  - enables token/delta-style streamed assistant output rather than only final assistant aggregates
- `--system-prompt`
  - allows backend-controlled system injection
  - should be derived from OpenCode-owned history, not backend-owned memory

## important known constraints

### Claude Code is not the conversation owner

For this project, Claude Code must be treated as a stateless transport target.

That means:

- every backend request should be reconstructible from OpenCode-owned history
- no backend-owned suspended Claude session should be required
- no backend-specific resume ids should be necessary for correctness

### tool use is emitted, not owned

Claude Code can emit tool-use blocks, but in `openclaude` these should be translated into provider-like events and handed back to the frontend layer.

The backend should not become the canonical owner of tool continuation state.

### prompt replay is expected

If a turn needs to continue after a tool result, the stateless target architecture is:

- OpenCode/plugin reconstructs full history
- backend translates that full history into a Claude Code prompt
- Claude Code receives a fresh one-shot request

This is less like restoring an internal Claude session and more like deterministic replay.

## stream-json structures relevant to openclaude

### top-level chunk kinds

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

### content blocks

Modeled block types:

- `thinking`
- `text`
- `tool_use`

#### thinking block

```json
{
  "type": "thinking",
  "thinking": "..."
}
```

#### text block

```json
{
  "type": "text",
  "text": "..."
}
```

#### tool_use block

```json
{
  "type": "tool_use",
  "id": "toolu_...",
  "name": "Read",
  "input": {}
}
```

### deltas

Modeled delta types:

- `thinking_delta`
- `text_delta`
- `input_json_delta`

#### thinking delta

```json
{
  "type": "thinking_delta",
  "thinking": "partial reasoning"
}
```

#### text delta

```json
{
  "type": "text_delta",
  "text": "partial text"
}
```

#### input json delta

```json
{
  "type": "input_json_delta",
  "partial_json": "{\"file_path\""
}
```

This matters because tool input often arrives incrementally and must be accumulated until `content_block_stop`.

## translation rules currently assumed by openclaude

### reasoning

- `thinking` / `thinking_delta` -> reasoning lifecycle parts
- start and stop are tracked by content-block boundaries

### text

- `text` / `text_delta` -> text lifecycle parts

### tool use

- `tool_use` block start -> tool input start
- `input_json_delta` -> tool input delta
- block stop -> tool input end + final tool call

## backend implementation guidance

When changing backend code, preserve these rules:

1. do not introduce backend-owned session ids for Claude continuity
2. do not persist suspended Claude tool state as the source of truth
3. prefer replay from OpenCode-owned canonical history
4. keep stream translation typed and explicit
5. do not collapse tool-use deltas into opaque blobs if the frontend can use richer lifecycle data

## current project files that implement this behavior

- `src/claude/cli.rs`
  - flag construction and process target
- `src/claude/stream.rs`
  - deserialization of Claude Code stream-json chunks
- `src/claude/translate.rs`
  - stateful translation from Claude chunks to provider stream parts
- `src/claude/runtime.rs`
  - subprocess execution and one-shot translation runtime
- `src/claude/prompt.rs`
  - prompt replay from canonical history

## things to watch for when editing this project

- shell quoting in commit messages can eat backticks; quote carefully
- CLI output formats can drift across Claude Code versions; keep parsing narrow and tested
- avoid depending on undocumented continuity semantics when the project goal is a stateless backend
- if a new feature appears to require backend-owned memory, prefer enriching canonical request history first

## recommended future checks

- verify whether newer Claude Code releases expose additional machine-readable fields useful for reasoning/tool provenance
- confirm whether model identifiers or stream event names changed across CLI versions
- keep this file updated whenever the backend starts relying on new Claude CLI behavior
