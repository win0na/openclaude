# opencode reference

This file is the project-local reference sheet for the OpenCode surfaces that `openclaude` is designed to integrate with. Keep it in the repo root so future users do not need a checkout at a machine-specific path like `~/claude/opencode`.

## core plugin and provider files

- `packages/plugin/src/index.ts`
  - canonical plugin contract
  - hook definitions
  - auth/tool/chat extension surface
- `packages/opencode/src/plugin/index.ts`
  - plugin loading and hook triggering
- `packages/opencode/src/provider/provider.ts`
  - provider loading
  - custom loaders
  - model resolution
- `packages/opencode/src/session/llm.ts`
  - chat params
  - headers
  - system transform usage
- `packages/opencode/src/session/prompt.ts`
  - message transform usage
- `packages/opencode/src/session/message-v2.ts`
  - persisted message and stream-part shape

## plugin hooks relevant to openclaude

- `auth`
  - best plugin surface for attaching provider behavior and custom fetch/loader logic
- `chat.params`
  - mutate model options before request dispatch
- `chat.headers`
  - inject request headers
- `experimental.chat.system.transform`
  - rewrite or extend system prompt content
- `experimental.chat.messages.transform`
  - rewrite full message history before provider execution
- `tool.execute.before`
  - inspect or modify tool arguments
- `tool.execute.after`
  - inspect or modify tool output
- `tool.definition`
  - adjust tool definitions shown to the model

## practical plugin limits

- plugins can attach auth and transport behavior to a provider
- plugins can add tools and transform chat input/output
- plugins do not provide a first-class way to register a brand-new provider runtime entirely on their own
- the likely no-patch path is a plugin shim that talks to `openclaude` over a standalone protocol

## project implications

- `openclaude` should keep exposing a standalone service and transport surface
- external plugin code should stay thin and use this backend for runtime/session logic
- model and provider metadata should be discoverable from the backend itself
- stream parts should stay aligned with what OpenCode already expects for reasoning, text, tool input, and tool calls

## maintenance

- treat this file as the portable replacement for machine-local `~/claude/opencode` references in project docs
- use `openclaude init` to refresh this file when project tooling supports it
