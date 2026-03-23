# opencode reference

This file is the internal implementation reference for OpenCode behavior that matters to `openclaude`.

It is intended to replace dependence on a full local `opencode-reference/` checkout for normal backend work.

## purpose in this project

`openclaude` is a translation layer between OpenCode and Claude Code.

This means OpenCode is the intended owner of:

- conversation history
- session identity
- tool execution
- subagent orchestration
- memory and context
- frontend rendering semantics

`openclaude` should adapt to OpenCode, not replace OpenCode.

## key architectural conclusion

Based on current plugin and code-surface research:

- a no-patch integration is realistic
- provider routing should live in OpenCode configuration
- the plugin should stay thin
- the backend should stay stateless

In practical terms:

- config declares the provider endpoint/base URL
- plugin handles auth, headers, params, and transforms
- `openclaude` handles native transport translation

## what plugins appear able to do

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

These support a thin plugin frontend that forwards to `openclaude`.

## what plugins should not be assumed to do

Do not assume a plugin can cleanly register an entirely new first-class provider runtime by itself.

Current guidance for this project is:

- provider routing in config
- thin plugin for request/auth shaping
- backend service for transport and translation

## important OpenCode-owned concepts to preserve

### canonical history

OpenCode should remain the owner of canonical message history.

For `openclaude`, this means:

- requests should contain full history
- backend should not require hidden continuation state
- tool continuation should be representable by replaying message history with tool results included

### sessions

OpenCode should own:

- session ids
- conversation grouping
- switching between providers/models

The backend should not become the canonical session manager.

### tool lifecycle

OpenCode should own the real tool execution lifecycle.

The backend should only:

- translate model-emitted tool intent
- return tool-call events in a shape the frontend can understand

### rendering semantics

OpenCode already has expectations for:

- reasoning blocks
- text chunks
- tool input lifecycle
- final tool calls

The backend should keep its event model aligned with those expectations.

## project-local integration guidance

Current internal layering in `openclaude`:

- `provider/`
  - provider-facing types and stream parts
- `claude/`
  - Claude CLI transport and translation
- `integration/`
  - adapter and bridge boundary types
- `server/`
  - HTTP server with OpenAI-compatible endpoints
  - stdio service for direct process communication
  - OpenAI-compatible request/response types

Expected eventual no-patch integration shape:

1. OpenCode config points a provider at an external backend or shim
2. a thin plugin handles auth and request shaping
3. the plugin sends full history to `openclaude`
4. `openclaude` returns translated events
5. OpenCode keeps owning sessions, tool execution, and visible history

## HTTP server protocol

`openclaude` exposes an OpenAI-compatible HTTP API for integration with OpenCode.

### endpoints

- `POST /v1/chat/completions` - chat completions (streaming and non-streaming)
- `GET /v1/models` - list available models
- `GET /health` - health check

### request format

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

### response format

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

### tool calls

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

## OpenCode configuration

To use `openclaude` as a provider in OpenCode:

1. Start the HTTP server:
   ```bash
   openclaude serve --host 127.0.0.1 --port 3000
   ```

2. Configure OpenCode to use the provider (in `opencode.json` or via environment):
   - set `baseURL` to `http://127.0.0.1:3000/v1`
   - the provider will appear as an OpenAI-compatible endpoint

3. Install the plugin (optional):
   - the plugin handles auth headers and session context
   - it's thin and doesn't implement provider logic

### provider routing

OpenCode routes to providers based on:
- provider ID in the model selection
- baseURL in the provider configuration
- the AI SDK's `createOpenAICompatible` for custom endpoints

The plugin should not try to register a new provider runtime. Instead:
- let OpenCode's config define the routing
- use the plugin only for auth/headers/transforms

## current backend contract expectations

The current direction in this repository is:

- stateless `complete` request
- full canonical history in the request
- translated event stream out

The backend should not require:

- transport-level resume ids
- backend-owned suspended tool state
- provider-local hidden memory

## important implementation takeaways for future work

### for backend changes

- prefer richer request history over hidden backend state
- prefer typed stream events over opaque text payloads
- keep the translation boundary narrow and testable

### for plugin or shim work

- keep the frontend thin
- avoid duplicating backend transport logic in TypeScript if Rust already owns it well
- let config define provider routing rather than relying on plugin magic

## recommended future references when implementing

When details are needed beyond this reference, the most relevant OpenCode areas are still conceptually:

- plugin hook definitions
- provider resolution/loading
- session request construction
- message and stream-part persistence

If this project begins depending on new OpenCode behavior, update this file rather than reintroducing machine-specific checkout assumptions.
