import { getModelConfig, type ModelConfig } from "./model-config.js";

export type { ModelConfig };
export { getModelConfig };

function parseModelMap(env: string | undefined): Record<string, string> {
  if (!env) return {};
  try {
    const parsed: unknown = JSON.parse(env);
    if (typeof parsed !== "object" || parsed === null) return {};
    const result: Record<string, string> = {};
    for (const [key, value] of Object.entries(parsed)) {
      if (typeof value === "string") result[key] = value;
    }
    return result;
  } catch {
    return {};
  }
}

const modelMap = parseModelMap(process.env.MODEL_MAP);

const tiers: Record<string, string> = {
  haiku: process.env.TIER_HAIKU || "gpt-5-mini",
  sonnet: process.env.TIER_SONNET || "gpt-5.2",
  opus: process.env.TIER_OPUS || "gpt-5.1-codex-max",
};

export function resolveModel(alias: string): string {
  if (modelMap[alias]) return modelMap[alias];

  const lower = alias.toLowerCase();
  for (const [tier, model] of Object.entries(tiers)) {
    if (lower.includes(tier)) return model;
  }
  return alias;
}

export function resolveModelConfig(model: string): {
  model: string;
  config: ModelConfig;
} {
  const resolved = resolveModel(model);
  return { model: resolved, config: getModelConfig(resolved) };
}

export const config = {
  port: parseInt(process.env.PROXY_PORT || "8000"),
  azure: {
    endpoint: process.env.AZURE_OPENAI_ENDPOINT || "",
    apiVersion: process.env.AZURE_OPENAI_API_VERSION || "2025-04-01-preview",
  },
  tiers,
};
