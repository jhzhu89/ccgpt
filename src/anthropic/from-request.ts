import type { ValidatedRequest } from "./validate.js";
import type * as IR from "../ir/types.js";
import { decodeReasoningItem } from "./reasoning.js";

type TextBlock = { type: "text"; text: string };
type ImageSource =
  | { type: "base64"; media_type: string; data: string }
  | { type: "url"; url: string };

type ImageBlock = {
  type: "image";
  source: ImageSource;
};

type ToolUseBlock = {
  type: "tool_use";
  id: string;
  name: string;
  input: unknown;
};

type ToolResultBlock = {
  type: "tool_result";
  tool_use_id: string;
  content: unknown;
  is_error?: boolean;
};

type ThinkingBlock = {
  type: "thinking";
  thinking: string;
  signature: string;
};

type MidConversationSystemBlock = {
  type: "mid_conv_system";
  content: TextBlock[];
};

type ContentBlock =
  | TextBlock
  | ImageBlock
  | ToolUseBlock
  | ToolResultBlock
  | ThinkingBlock
  | MidConversationSystemBlock;

type MessageBlock = {
  role: IR.Role;
  content: string | ContentBlock[];
};

type SystemBlock = string | TextBlock[];

type RequestBody = Omit<ValidatedRequest, "messages" | "system"> & {
  messages: MessageBlock[];
  system?: SystemBlock;
};

function isTextBlock(b: { type: string }): b is TextBlock {
  return b.type === "text";
}

function isRecord(val: unknown): val is Record<string, unknown> {
  return typeof val === "object" && val !== null;
}

function parseToolChoice(tc: unknown): IR.ToolChoice | undefined {
  if (!isRecord(tc)) return undefined;
  const disableParallelToolUse =
    typeof tc.disable_parallel_tool_use === "boolean"
      ? tc.disable_parallel_tool_use
      : undefined;
  if (tc.type === "auto") return { type: "auto", disableParallelToolUse };
  if (tc.type === "any") return { type: "any", disableParallelToolUse };
  if (tc.type === "none") return { type: "none" };
  if (tc.type === "tool" && typeof tc.name === "string") {
    return { type: "tool", name: tc.name, disableParallelToolUse };
  }
  return undefined;
}

function budgetToEffort(budget: number): IR.ReasoningEffort {
  if (budget <= 4_000) return "low";
  if (budget <= 16_000) return "medium";
  return "high";
}

function parseReasoningEffort(
  t: unknown,
  outputEffort?: "low" | "medium" | "high" | "xhigh" | "max",
): IR.ReasoningEffort | undefined {
  if (outputEffort) return outputEffort;
  if (!isRecord(t)) return undefined;
  if (t.type === "disabled") return "none";
  if (t.type === "enabled" && typeof t.budget_tokens === "number") {
    return budgetToEffort(t.budget_tokens);
  }
  return undefined;
}

function parseImageBlock(block: ImageBlock): IR.ImageContent {
  if (block.source.type === "base64") {
    return {
      type: "image",
      url: `data:${block.source.media_type};base64,${block.source.data}`,
    };
  }
  return { type: "image", url: block.source.url };
}

export function fromRequest(body: RequestBody): IR.Request {
  const messages: IR.Message[] = [];

  if (body.system) {
    const systemText =
      typeof body.system === "string"
        ? body.system
        : body.system
            .filter(isTextBlock)
            .map((b) => b.text)
            .join("\n");
    messages.push({
      role: "system",
      content: [{ type: "text", text: systemText }],
    });
  }

  for (const msg of body.messages) {
    const content: IR.Content[] = [];
    const blocks = Array.isArray(msg.content)
      ? msg.content
      : [{ type: "text" as const, text: msg.content }];

    for (const block of blocks) {
      if (block.type === "text") {
        content.push({ type: "text", text: block.text });
        continue;
      }
      if (block.type === "image") {
        content.push(parseImageBlock(block));
        continue;
      }
      if (block.type === "thinking") {
        const item = decodeReasoningItem(block.signature);
        if (item) content.push({ type: "reasoning", item });
        continue;
      }
      if (block.type === "mid_conv_system") {
        for (const item of block.content) {
          content.push({ type: "text", text: item.text });
        }
        continue;
      }
      if (block.type === "tool_use") {
        content.push({
          type: "tool_call",
          id: block.id,
          name: block.name,
          arguments: block.input,
        });
        continue;
      }
      content.push({
        type: "tool_result",
        id: block.tool_use_id,
        output: block.content,
        isError: block.is_error,
      });
    }
    messages.push({ role: msg.role, content });
  }

  const tools: IR.ToolDefinition[] = [];
  for (const t of body.tools ?? []) {
    if ("input_schema" in t && isRecord(t.input_schema)) {
      tools.push({
        name: t.name,
        description:
          "description" in t && typeof t.description === "string"
            ? t.description
            : undefined,
        inputSchema: t.input_schema,
      });
    }
  }

  return {
    model: body.model,
    messages,
    tools: tools.length > 0 ? tools : undefined,
    stream: body.stream ?? false,
    maxTokens: body.max_tokens,
    temperature: body.temperature,
    topP: body.top_p,
    stopSequences: body.stop_sequences,
    toolChoice: parseToolChoice(body.tool_choice),
    reasoningEffort: parseReasoningEffort(
      body.thinking,
      body.output_config?.effort,
    ),
  };
}
