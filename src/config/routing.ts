import { getModelConfig, type ModelConfig } from "./model-config.js";

export interface RoutingConfig {
  modelMap: Record<string, string>;
  tiers: Record<string, string>;
}

export function resolveModel(config: RoutingConfig, alias: string): string {
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

export function resolveModelConfig(
  config: RoutingConfig,
  model: string,
): { model: string; config: ModelConfig } {
  const resolvedModel = resolveModel(config, model);
  const configSnapshot = getModelConfig(resolvedModel);

  return { model: resolvedModel, config: configSnapshot };
}
