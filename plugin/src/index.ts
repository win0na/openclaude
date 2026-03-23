import type { Plugin } from "@opencode-ai/plugin"

/**
 * thin plugin frontend for the openclaude backend.
 *
 * this plugin handles:
 * - auth configuration for the openclaude provider
 * - request headers for session tracking
 * - request params for model routing
 *
 * the actual translation between OpenCode and Claude Code happens in the
 * openclaude Rust backend, which exposes an OpenAI-compatible HTTP API.
 *
 * to use this plugin:
 * 1. run `openclaude serve` to start the HTTP backend
 * 2. configure OpenCode to use openclaude as a provider with baseURL pointing to the backend
 * 3. install this plugin in OpenCode
 */
const plugin: Plugin = async () => {
  return {
    auth: {
      provider: "openclaude",
      methods: [
        {
          type: "api",
          label: "local openclaude backend (no key required)",
        },
      ],
    },
    "chat.headers": async (input, output) => {
      if (input.model.providerID !== "openclaude") return

      // pass session context to the backend for logging/debugging
      output.headers["x-openclaude-session-id"] = input.sessionID
      output.headers["x-openclaude-agent"] = input.agent
    },
    "chat.params": async (input, output) => {
      if (input.model.providerID !== "openclaude") return

      // the backend handles model routing internally
      // this is just for compatibility
      output.options.openclaude = {
        providerID: input.model.providerID,
        modelID: input.model.id,
      }
    },
    "experimental.chat.messages.transform": async () => {
      // no transformation needed - the backend handles message translation
    },
  }
}

export default plugin
