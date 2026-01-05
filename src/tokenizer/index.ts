import { encode } from "gpt-tokenizer";
import type * as IR from "../ir/types.js";

const TOKENS_PER_MESSAGE = 3;
const BASE_TOKENS = 3;

export function countMessageTokens(messages: IR.Message[]): number {
  let total = BASE_TOKENS;

  for (const msg of messages) {
    total += TOKENS_PER_MESSAGE;
    total += encode(msg.role).length;

    for (const c of msg.content) {
      switch (c.type) {
        case "text":
          total += encode(c.text).length;
          break;
        case "tool_call":
          total += encode(c.name).length;
          total += encode(JSON.stringify(c.arguments)).length;
          break;
        case "tool_result":
          total += encode(JSON.stringify(c.output)).length;
          break;
        case "image":
          total += 85;
          break;
      }
    }
  }

  return total;
}

export function countToolTokens(tools: IR.ToolDefinition[]): number {
  let total = 0;
  for (const t of tools) {
    total += encode(t.name).length;
    total += encode(t.description || "").length;
    total += encode(JSON.stringify(t.inputSchema)).length;
    total += 10;
  }
  return total;
}
