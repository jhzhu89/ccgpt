import type { StopReason } from "./types.js";

export function generateId(prefix = "msg"): string {
  return `${prefix}_${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;
}

export function mapStopReason(
  status: string | undefined,
  hasToolCall: boolean,
): StopReason {
  if (hasToolCall) return "tool_use";
  if (status === "incomplete") return "max_tokens";
  return "end_turn";
}
