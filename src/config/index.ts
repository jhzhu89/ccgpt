import {
  resolveModel as routingResolveModel,
  resolveModelConfig as routingResolveModelConfig,
} from "./routing.js";
import type { ModelConfig } from "./model-config.js";

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

const configSnapshot = {
  port: parseInt(process.env.PROXY_PORT || "8000"),
  azure: {
    endpoint: process.env.AZURE_OPENAI_ENDPOINT || "",
    apiVersion: process.env.AZURE_OPENAI_API_VERSION || "2025-04-01-preview",
  },
  modelMap: parseModelMap(process.env.MODEL_MAP),
  tiers: {
    haiku: process.env.TIER_HAIKU || "gpt-5-mini",
    sonnet: process.env.TIER_SONNET || "gpt-5.2",
    opus: process.env.TIER_OPUS || "gpt-5.1-codex-max",
  },
};

export const config = {
  ...configSnapshot,
  resolveModel: (alias: string): string =>
    routingResolveModel(configSnapshot, alias),
  resolveModelConfig: (model: string): { model: string; config: ModelConfig } =>
    routingResolveModelConfig(configSnapshot, model),
};

export const resolveModel = config.resolveModel;
export const resolveModelConfig = config.resolveModelConfig;
