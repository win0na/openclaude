import type { Plugin } from "@opencode-ai/plugin"

const DEFAULT_PROVIDER_ID = process.env.OPENCLAUDE_PROVIDER_ID ?? "openclaude"

const plugin: Plugin = async () => {
  return {
    auth: {
      provider: DEFAULT_PROVIDER_ID,
      methods: [
        {
          type: "api",
          label: "local openclaude backend (no key required)",
        },
      ],
    },
    "chat.headers": async (input, output) => {
      if (input.model.providerID !== DEFAULT_PROVIDER_ID) return

      output.headers["x-openclaude-session-id"] = input.sessionID
      output.headers["x-openclaude-agent"] = input.agent
    },
    "chat.params": async (input, output) => {
      if (input.model.providerID !== DEFAULT_PROVIDER_ID) return

      output.options.openclaude = {
        providerID: input.model.providerID,
        modelID: input.model.id,
      }
    },
    "experimental.chat.messages.transform": async () => {},
  }
}

export default plugin
