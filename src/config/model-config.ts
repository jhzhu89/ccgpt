import type { ReasoningEffort } from "../ir/types.js";

export interface CopilotModel {
  id: string;
  supported_endpoints?: string[];
  capabilities?: {
    supports?: {
      parallel_tool_calls?: boolean;
      reasoning_effort?: string[];
    };
  };
}

export interface ModelConfig {
  supportsParallelToolCalls: boolean;
  reasoningEfforts: ReasoningEffort[];
}

const reasoningEfforts = new Set<ReasoningEffort>([
  "none",
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
]);

function isReasoningEffort(value: string): value is ReasoningEffort {
  return reasoningEfforts.has(value as ReasoningEffort);
}

export function getModelConfig(model: CopilotModel): ModelConfig {
  const supports = model.capabilities?.supports;
  return {
    supportsParallelToolCalls: supports?.parallel_tool_calls === true,
    reasoningEfforts: (supports?.reasoning_effort ?? []).filter(
      isReasoningEffort,
    ),
  };
}
