import type { StopReason } from "../ir/types.js";

export function mapOpenAIStopReason(
  status: string | undefined,
  hasToolCall: boolean,
): StopReason {
  if (hasToolCall) return "tool_use";
  if (status === "incomplete") return "max_tokens";
  return "end_turn";
}
