import {
  getModelConfig,
  type CopilotModel,
  type ModelConfig,
} from "./model-config.js";

export type { CopilotModel, ModelConfig };

export type ModelTier = "high" | "balanced" | "fast";

export interface ResolvedModel {
  model: string;
  config: ModelConfig;
}

export interface ModelRouter {
  resolve(requested: string): ResolvedModel;
  targets: Record<ModelTier, string>;
}

export type BackendConfig =
  | { kind: "copilot" }
  | {
      kind: "openai";
      apiKey: string;
      baseURL: string;
      targets: Record<ModelTier, string>;
    };

const tierSuffixes: Record<ModelTier, string> = {
  high: "sol",
  balanced: "terra",
  fast: "luna",
};

function compareVersions(left: number[], right: number[]): number {
  const length = Math.max(left.length, right.length);
  for (let i = 0; i < length; i++) {
    const difference = (left[i] ?? 0) - (right[i] ?? 0);
    if (difference !== 0) return difference;
  }
  return 0;
}

function versionForTier(id: string, suffix: string): number[] | undefined {
  const match = new RegExp(
    `^gpt-(\\d+(?:\\.\\d+)*)-${suffix}(?:-|$)`,
    "i",
  ).exec(id);
  return match?.[1].split(".").map(Number);
}

function selectLatest(models: CopilotModel[], suffix: string): CopilotModel {
  const matches = models
    .map((model) => ({ model, version: versionForTier(model.id, suffix) }))
    .filter(
      (entry): entry is { model: CopilotModel; version: number[] } =>
        entry.version !== undefined,
    )
    .sort((left, right) => compareVersions(right.version, left.version));

  const selected = matches.at(0)?.model;
  if (!selected) {
    throw new Error(`Copilot has no Responses model in the ${suffix} tier`);
  }
  return selected;
}

function classifyClaudeModel(model: string): ModelTier {
  const normalized = model.toLowerCase();
  if (normalized.includes("haiku")) return "fast";
  if (normalized.includes("sonnet")) return "balanced";
  return "high";
}

export function createCopilotModelRouter(models: CopilotModel[]): ModelRouter {
  const responseModels = models.filter((model) =>
    model.supported_endpoints?.includes("/responses"),
  );
  const byId = new Map(
    responseModels.map((model) => [model.id.toLowerCase(), model]),
  );

  const tierModels: Record<ModelTier, CopilotModel> = {
    high: selectLatest(responseModels, tierSuffixes.high),
    balanced: selectLatest(responseModels, tierSuffixes.balanced),
    fast: selectLatest(responseModels, tierSuffixes.fast),
  };

  return {
    targets: {
      high: tierModels.high.id,
      balanced: tierModels.balanced.id,
      fast: tierModels.fast.id,
    },
    resolve(requested: string): ResolvedModel {
      const exact = byId.get(requested.toLowerCase());
      if (exact) return { model: exact.id, config: getModelConfig(exact) };
      if (requested.toLowerCase().startsWith("gpt-")) {
        throw new Error(`${requested} is not available from Copilot`);
      }

      const selected = tierModels[classifyClaudeModel(requested)];
      return { model: selected.id, config: getModelConfig(selected) };
    },
  };
}

const directModelConfig: ModelConfig = {
  supportsParallelToolCalls: true,
  reasoningEfforts: ["none", "low", "medium", "high", "xhigh", "max"],
};

function isClaudeModel(model: string): boolean {
  return /claude|fable|mythos|opus|sonnet|haiku/i.test(model);
}

export function createDirectModelRouter(
  targets: Record<ModelTier, string>,
): ModelRouter {
  return {
    targets,
    resolve(requested: string): ResolvedModel {
      const target = Object.values(targets).find(
        (model) => model.toLowerCase() === requested.toLowerCase(),
      );
      if (target || !isClaudeModel(requested)) {
        return { model: target ?? requested, config: directModelConfig };
      }

      return {
        model: targets[classifyClaudeModel(requested)],
        config: directModelConfig,
      };
    },
  };
}

export function readConfig(env: NodeJS.ProcessEnv = process.env): {
  port: number;
  backend: BackendConfig;
} {
  const apiKey = env.CCGPT_API_KEY?.trim();
  const baseURL = env.CCGPT_BASE_URL?.trim();
  if (baseURL && !apiKey) {
    throw new Error("CCGPT_BASE_URL requires CCGPT_API_KEY");
  }

  const backend: BackendConfig = apiKey
    ? {
        kind: "openai",
        apiKey,
        baseURL: baseURL || "https://api.openai.com/v1",
        targets: {
          high: env.CCGPT_MODEL_HIGH?.trim() || "gpt-5.6-sol",
          balanced: env.CCGPT_MODEL_BALANCED?.trim() || "gpt-5.6-terra",
          fast: env.CCGPT_MODEL_FAST?.trim() || "gpt-5.6-luna",
        },
      }
    : { kind: "copilot" };

  return {
    port: parseInt(env.CCGPT_PORT || "8000"),
    backend,
  };
}

export const config = readConfig();
