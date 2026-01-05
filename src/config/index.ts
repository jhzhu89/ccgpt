import "dotenv/config";

function parseModelMap(env: string | undefined): Record<string, string> {
  if (!env) return {};
  const parsed: unknown = JSON.parse(env);
  if (typeof parsed !== "object" || parsed === null) return {};
  const result: Record<string, string> = {};
  for (const [key, value] of Object.entries(parsed)) {
    if (typeof value === "string") {
      result[key] = value;
    }
  }
  return result;
}

export const config = {
  port: parseInt(process.env.PROXY_PORT || "8000"),
  azure: {
    endpoint: process.env.AZURE_OPENAI_ENDPOINT || "",
    apiVersion: process.env.AZURE_OPENAI_API_VERSION || "2025-03-01-preview",
  },
  modelMap: parseModelMap(process.env.MODEL_MAP),
  tiers: {
    haiku: process.env.TIER_HAIKU || "gpt-5-nano",
    sonnet: process.env.TIER_SONNET || "gpt-5",
    opus: process.env.TIER_OPUS || "gpt-5-codex",
  },
};

export function resolveModel(alias: string): string {
  if (config.modelMap[alias]) {
    return config.modelMap[alias];
  }

  const lower = alias.toLowerCase();
  for (const [tier, model] of Object.entries(config.tiers)) {
    if (lower.includes(tier)) {
      return model;
    }
  }

  return alias;
}
