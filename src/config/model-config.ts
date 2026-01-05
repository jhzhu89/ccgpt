export type ReasoningEffort = "low" | "medium" | "high";

export interface ModelConfig {
  supportsParallelToolCalls: boolean;
  supportsReasoningSummaries: boolean;
  defaultReasoningEffort?: ReasoningEffort;
  contextWindow: number;
}

const MODEL_FAMILIES: ReadonlyArray<{ prefix: string; config: ModelConfig }> = [
  {
    prefix: "gpt-5.2-codex",
    config: {
      supportsParallelToolCalls: true,
      supportsReasoningSummaries: true,
      contextWindow: 272_000,
    },
  },
  {
    prefix: "gpt-5.1-codex-max",
    config: {
      supportsParallelToolCalls: false,
      supportsReasoningSummaries: true,
      defaultReasoningEffort: "medium",
      contextWindow: 272_000,
    },
  },
  {
    prefix: "gpt-5.1-codex",
    config: {
      supportsParallelToolCalls: false,
      supportsReasoningSummaries: true,
      contextWindow: 272_000,
    },
  },
  {
    prefix: "gpt-5-codex",
    config: {
      supportsParallelToolCalls: false,
      supportsReasoningSummaries: true,
      contextWindow: 272_000,
    },
  },
  {
    prefix: "codex-",
    config: {
      supportsParallelToolCalls: false,
      supportsReasoningSummaries: true,
      contextWindow: 272_000,
    },
  },
  {
    prefix: "gpt-5.2",
    config: {
      supportsParallelToolCalls: true,
      supportsReasoningSummaries: true,
      defaultReasoningEffort: "medium",
      contextWindow: 272_000,
    },
  },
  {
    prefix: "gpt-5.1",
    config: {
      supportsParallelToolCalls: true,
      supportsReasoningSummaries: true,
      defaultReasoningEffort: "medium",
      contextWindow: 272_000,
    },
  },
  {
    prefix: "gpt-5",
    config: {
      supportsParallelToolCalls: false,
      supportsReasoningSummaries: true,
      contextWindow: 272_000,
    },
  },
];

const DEFAULT_CONFIG: ModelConfig = {
  supportsParallelToolCalls: false,
  supportsReasoningSummaries: false,
  contextWindow: 128_000,
};

export function getModelConfig(slug: string): ModelConfig {
  const lower = slug.toLowerCase();
  for (const { prefix, config } of MODEL_FAMILIES) {
    if (lower.startsWith(prefix)) {
      return config;
    }
  }
  return DEFAULT_CONFIG;
}
