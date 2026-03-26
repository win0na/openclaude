# OpenCode Reference

This file is the internal implementation reference for OpenCode behavior that matters to `clyde`.

It is intended to replace dependence on a full local `opencode-reference/` checkout for normal backend work.

## Purpose in This Project

`clyde` is a translation layer between OpenCode and Claude Code.

This means OpenCode is the intended owner of:

- conversation history
- session identity
- tool execution
- subagent orchestration
- memory and context
- frontend rendering semantics

`clyde` should adapt to OpenCode, not replace OpenCode.

## Key Architectural Conclusion

Based on current plugin and code-surface research:

- a no-patch integration is realistic
- provider routing should live in OpenCode configuration
- the plugin should stay thin
- the backend should stay stateless

In practical terms:

- config declares the provider endpoint/base URL
- plugin handles auth, headers, params, and transforms
- `clyde` handles native transport translation

## What Plugins Appear Able to Do

Relevant plugin capabilities and hook categories:

- auth-related hooks
  - credential loading
  - provider-specific auth setup
- chat request mutation
  - headers
  - params
  - system prompt transforms
  - message transforms
- tool interception hooks
  - before execution
  - after execution

These support a thin plugin frontend that forwards to `clyde`.

## What Plugins Should Not Be Assumed to Do

Do not assume a plugin can cleanly register an entirely new first-class provider runtime by itself.

Current guidance for this project is:

- provider routing in config
- thin plugin for request/auth shaping
- backend service for transport and translation

## Important OpenCode-Owned Concepts to Preserve

### Canonical History

OpenCode should remain the owner of canonical message history.

For `clyde`, this means:

- requests should contain full history
- backend should not require hidden continuation state
- tool continuation should be representable by replaying message history with tool results included

### Sessions

OpenCode should own:

- session IDs
- conversation grouping
- switching between providers/models

The backend should not become the canonical session manager.

### Tool Lifecycle

OpenCode should own the real tool execution lifecycle.

The backend should only:

- translate model-emitted tool intent
- return tool-call events in a shape the frontend can understand

### Rendering Semantics

OpenCode already has expectations for:

- reasoning blocks
- text chunks
- tool input lifecycle
- final tool calls

The backend should keep its event model aligned with those expectations.

## Project-Local Integration Guidance

Current internal layering in `clyde`:

- `provider/`
  - provider-facing types and stream parts
- `claude/`
  - Claude CLI transport and translation
- `integration/`
  - adapter and bridge boundary types
- `server/`
  - HTTP server with OpenAI-compatible endpoints
  - STDIO service for direct process communication
  - OpenAI-compatible request/response types

Expected eventual no-patch integration shape:

1. OpenCode config points a provider at an external backend or shim.
2. A thin plugin handles auth and request shaping.
3. The plugin sends full history to `clyde`.
4. `clyde` returns translated events.
5. OpenCode keeps owning sessions, tool execution, and visible history.

## HTTP Server Protocol

`clyde` exposes an OpenAI-compatible HTTP API for integration with OpenCode.

### Endpoints

- `POST /v1/chat/completions` - chat completions (streaming and non-streaming)
- `GET /v1/models` - list available models
- `GET /health` - health check

### Request Format

The server accepts standard OpenAI chat completion requests:

```json
{
  "model": "claude-sonnet",
  "messages": [
    {"role": "user", "content": "hello"}
  ],
  "stream": true
}
```

### Response Format

Non-streaming responses follow the OpenAI format:

```json
{
  "id": "chatcmpl-xxx",
  "object": "chat.completion",
  "created": 1234567890,
  "model": "claude-sonnet",
  "choices": [
    {
      "index": 0,
      "message": {"role": "assistant", "content": "hello!"},
      "finish_reason": "stop"
    }
  ]
}
```

Streaming responses use SSE with `data: {...}` lines and `data: [DONE]` at the end.

### Tool Calls

Tool calls are returned in the OpenAI format:

```json
{
  "choices": [{
    "message": {
      "role": "assistant",
      "content": null,
      "tool_calls": [{
        "id": "toolu_xxx",
        "type": "function",
        "function": {
          "name": "Read",
          "arguments": "{\"file_path\": \"/tmp/a\"}"
        }
      }]
    },
    "finish_reason": "tool_calls"
  }]
}
```

## OpenCode Configuration

To use `clyde` as a provider in OpenCode:

1. Start the HTTP server:
   ```bash
clyde serve --host 127.0.0.1 --port 3000
   ```

2. Configure OpenCode to use the provider (in `opencode.json` or via environment):
   - set `baseURL` to `http://127.0.0.1:3000/v1`
   - the provider will appear as an OpenAI-compatible endpoint

3. Install the plugin (optional):
   - the plugin handles auth headers and session context
   - it's thin and doesn't implement provider logic

### Provider Routing

OpenCode routes to providers based on:

- provider ID in the model selection
- baseURL in the provider configuration
- the AI SDK's `createOpenAICompatible` for custom endpoints

The plugin should not try to register a new provider runtime. Instead:

- let OpenCode's config define the routing
- use the plugin only for auth/headers/transforms

### Verified Plugin Limitation

Based on the local OpenCode code surface:

- plugins can contribute auth flows for existing provider IDs
- plugins can shape requests through hooks like `chat.headers` and `chat.params`
- plugins cannot dynamically register a brand-new provider runtime at startup

Relevant files in the optional local checkout:

- `opencode-reference/packages/plugin/src/index.ts`
- `opencode-reference/packages/opencode/src/plugin/index.ts`
- `opencode-reference/packages/opencode/src/provider/provider.ts`
- `opencode-reference/packages/opencode/src/session/llm.ts`

The practical implication is that `clyde` should assume one of the following setup patterns rather than true plugin-driven provider registration.

### Supported Setup Options

#### Option 1: Wrapper-Managed Bootstrap Config

`clyde` can generate temporary bootstrap config for the launched `opencode` process without editing user files.

- best fit for the current no-patch goal
- keeps the plugin thin at runtime
- keeps the user's normal `opencode` command unchanged
- uses `OPENCODE_CONFIG_CONTENT` so OpenCode merges the bootstrap entries with existing config for that process only
- current implementation target is provider ID `clyde` with model entries resolved at launch using this precedence: manual override (`CLYDE_AVAILABLE_MODELS` / `--available-models`) -> cached local discovery -> Claude CLI probing -> fallback `haiku`, `sonnet`, and `opus`, backed by `@ai-sdk/openai-compatible`

#### Option 2: Plugin-Managed Config Bootstrap

The plugin can create or update the user's OpenCode config so the `clyde` provider entry exists before chat starts.

- still possible in principle
- more invasive because it edits user config
- no longer the preferred implementation direction in this repository

#### Option 3: Reuse an Existing OpenAI-Compatible Provider Slot

The user or bootstrap flow points an existing custom/OpenAI-compatible provider entry at `http://127.0.0.1:3000/v1`.

- lowest engineering risk
- works with the current backend immediately
- still depends on config existing first

#### Option 4: Upstream Provider Hook Support

OpenCode could eventually add a real plugin hook for provider registration.

- only path to true dynamic provider registration
- not available in the current code surface
- outside the current no-patch integration plan unless accepted upstream

### Recommended Direction

For the current repository, the best practical path is:

1. Keep `clyde serve` as the backend runtime entrypoint.
2. Make bare `clyde` launch `opencode` with temporary bootstrap config.
3. Keep the plugin focused on auth, headers, params, and transforms.
4. Avoid editing user config by default.
5. Avoid designing around unsupported dynamic provider registration.

The current implementation direction is wrapper-managed bootstrap config plus automatic plugin loading.

## Current Backend Contract Expectations

The current direction in this repository is:

- stateless `complete` request
- full canonical history in the request
- translated event stream out

The backend should not require:

- transport-level resume IDs
- backend-owned suspended tool state
- provider-local hidden memory

## Important Implementation Takeaways for Future Work

### For Backend Changes

- prefer richer request history over hidden backend state
- prefer typed stream events over opaque text payloads
- keep the translation boundary narrow and testable

### For Plugin or Shim Work

- keep the frontend thin
- avoid duplicating backend transport logic in TypeScript if Rust already owns it well
- let config define provider routing rather than relying on plugin magic
