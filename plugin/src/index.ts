import { mkdir, readFile, writeFile } from "node:fs/promises"
import { homedir } from "node:os"
import path from "node:path"
import type { Plugin } from "@opencode-ai/plugin"
import { applyEdits, modify, parse as parseJsonc } from "jsonc-parser"

const DEFAULT_PROVIDER_ID = process.env.OPENCLAUDE_PROVIDER_ID ?? "openclaude"
const DEFAULT_BASE_URL = process.env.OPENCLAUDE_BASE_URL ?? "http://127.0.0.1:3000/v1"

type JsonObject = Record<string, unknown>

function globalConfigDir() {
  const xdgConfig = process.env.XDG_CONFIG_HOME
  return xdgConfig ? path.join(xdgConfig, "opencode") : path.join(homedir(), ".config", "opencode")
}

function globalConfigPath() {
  const dir = globalConfigDir()
  return ["opencode.jsonc", "opencode.json", "config.json"].map((name) => path.join(dir, name))
}

function isRecord(value: unknown): value is JsonObject {
  return !!value && typeof value === "object" && !Array.isArray(value)
}

function patchJsonc(input: string, patch: unknown, objectPath: string[] = []): string {
  if (!isRecord(patch)) {
    const edits = modify(input, objectPath, patch, {
      formattingOptions: {
        insertSpaces: true,
        tabSize: 2,
      },
    })
    return applyEdits(input, edits)
  }

  return Object.entries(patch).reduce((result, [key, value]) => {
    if (value === undefined) return result
    return patchJsonc(result, value, [...objectPath, key])
  }, input)
}

function ensureRecord(parent: JsonObject, key: string): JsonObject {
  const existing = parent[key]
  if (isRecord(existing)) return existing
  const next: JsonObject = {}
  parent[key] = next
  return next
}

function ensureProviderConfig(config: JsonObject, providerID: string, baseURL: string) {
  const provider = ensureRecord(ensureRecord(config, "provider"), providerID)
  let changed = false

  if (typeof provider.npm !== "string") {
    provider.npm = "@ai-sdk/openai-compatible"
    changed = true
  }

  if (typeof provider.name !== "string") {
    provider.name = "openclaude"
    changed = true
  }

  const options = ensureRecord(provider, "options")
  if (typeof options.baseURL !== "string") {
    options.baseURL = baseURL
    changed = true
  }

  const models = ensureRecord(provider, "models")
  for (const [id, name] of [
    ["haiku", "Claude Haiku"],
    ["sonnet", "Claude Sonnet"],
    ["opus", "Claude Opus"],
  ] as const) {
    const model = ensureRecord(models, id)
    if (typeof model.name !== "string") {
      model.name = name
      changed = true
    }
    if (typeof model.id !== "string") {
      model.id = id
      changed = true
    }
  }

  return changed
}

async function ensureGlobalProviderBootstrap(providerID: string, baseURL: string) {
  const dir = globalConfigDir()
  const candidates = globalConfigPath()
  await mkdir(dir, { recursive: true })

  let filePath = candidates[0]
  let text = "{}\n"
  for (const candidate of candidates) {
    try {
      text = await readFile(candidate, "utf8")
      filePath = candidate
      break
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error
    }
  }

  const parsed = parseJsonc(text)
  const config = isRecord(parsed) ? structuredClone(parsed) : {}
  const changed = ensureProviderConfig(config, providerID, baseURL)
  if (!changed) return

  if (filePath.endsWith(".jsonc")) {
    const next = patchJsonc(text, { provider: { [providerID]: (config.provider as JsonObject)[providerID] } })
    await writeFile(filePath, next)
    return
  }

  await writeFile(filePath, `${JSON.stringify(config, null, 2)}\n`)
}

const plugin: Plugin = async () => {
  await ensureGlobalProviderBootstrap(DEFAULT_PROVIDER_ID, DEFAULT_BASE_URL)

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
