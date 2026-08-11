import type { ReasoningItem } from "../ir/types.js";

const prefix = "ccgpt:";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export function encodeReasoningItem(item: ReasoningItem): string {
  return `${prefix}${Buffer.from(JSON.stringify(item)).toString("base64url")}`;
}

export function decodeReasoningItem(
  signature: string,
): ReasoningItem | undefined {
  if (!signature.startsWith(prefix)) return undefined;

  try {
    const json = Buffer.from(
      signature.slice(prefix.length),
      "base64url",
    ).toString("utf8");
    const parsed: unknown = JSON.parse(json) as unknown;
    if (
      !isRecord(parsed) ||
      parsed.type !== "reasoning" ||
      typeof parsed.id !== "string" ||
      !Array.isArray(parsed.summary) ||
      typeof parsed.encrypted_content !== "string"
    ) {
      return undefined;
    }
    return parsed as ReasoningItem;
  } catch {
    return undefined;
  }
}
