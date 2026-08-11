import type * as IR from "../ir/types.js";
import { generateId } from "../ir/normalize.js";
import { encodeReasoningItem } from "./reasoning.js";

interface AnthropicThinkingBlock {
  type: "thinking";
  thinking: "";
  signature: string;
}

interface AnthropicTextBlock {
  type: "text";
  text: string;
}

interface AnthropicToolUseBlock {
  type: "tool_use";
  id: string;
  name: string;
  input: unknown;
}

type AnthropicContentBlock =
  | AnthropicThinkingBlock
  | AnthropicTextBlock
  | AnthropicToolUseBlock;

interface AnthropicResponse {
  id: string;
  type: "message";
  role: "assistant";
  content: AnthropicContentBlock[];
  model: string;
  stop_reason: "end_turn" | "tool_use" | "max_tokens";
  usage: { input_tokens: number; output_tokens: number };
}

function toContentBlock(c: IR.ResponseContent): AnthropicContentBlock {
  if (c.type === "reasoning") {
    return {
      type: "thinking",
      thinking: "",
      signature: encodeReasoningItem(c.item),
    };
  }
  if (c.type === "text") {
    return { type: "text", text: c.text };
  }
  return { type: "tool_use", id: c.id, name: c.name, input: c.arguments };
}

export function toResponse(ir: IR.Response, model: string): AnthropicResponse {
  return {
    id: generateId("msg"),
    type: "message",
    role: "assistant",
    content: ir.content.map(toContentBlock),
    model,
    stop_reason: ir.stopReason,
    usage: {
      input_tokens: ir.usage.inputTokens,
      output_tokens: ir.usage.outputTokens,
    },
  };
}
