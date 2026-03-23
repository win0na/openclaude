import type { Plugin } from "@opencode-ai/plugin"

const plugin: Plugin = async () => {
  return {
    auth: {
      provider: "openclaude",
      methods: [
        {
          type: "api",
          label: "local openclaude backend",
        },
      ],
    },
    "chat.headers": async (input, output) => {
      if (input.model.providerID !== "openclaude") return

      output.headers["x-openclaude-session-id"] = input.sessionID
      output.headers["x-openclaude-agent"] = input.agent
    },
    "chat.params": async (input, output) => {
      if (input.model.providerID !== "openclaude") return

      output.options.openclaude = {
        providerID: input.model.providerID,
        modelID: input.model.modelID,
      }
    },
    "experimental.chat.messages.transform": async () => {},
  }
}

export default plugin
