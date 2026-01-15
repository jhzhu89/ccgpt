import { resolveModelConfig, type ModelConfig } from "../config/index.js";
import type * as IR from "../ir/types.js";
import type {
  ResponseCreateParamsNonStreaming,
  ResponseInputContent,
  ResponseInputItem,
  ToolChoiceFunction,
} from "openai/resources/responses/responses.js";

function mapToolChoice(
  tc: IR.ToolChoice | undefined,
): "required" | ToolChoiceFunction | undefined {
  if (!tc || tc.type === "auto") return undefined;
  if (tc.type === "any") return "required";
  return { type: "function", name: tc.name };
}

function buildInput(ir: IR.Request): {
  input: ResponseInputItem[];
  tools?: ResponseCreateParamsNonStreaming["tools"];
  toolChoice?: "required" | ToolChoiceFunction;
} {
  const input: ResponseInputItem[] = [];

  for (const msg of ir.messages) {
    if (msg.role === "system") {
      input.push({
        role: "system",
        content: msg.content
          .map((c) => (c.type === "text" ? c.text : ""))
          .join(""),
      });
      continue;
    }

    if (msg.role === "user") {
      const parts: ResponseInputContent[] = [];
      for (const c of msg.content) {
        if (c.type === "text") {
          parts.push({ type: "input_text", text: c.text });
          continue;
        }
        if (c.type === "image") {
          parts.push({ type: "input_image", image_url: c.url, detail: "auto" });
          continue;
        }
        if (c.type === "tool_result") {
          input.push({
            type: "function_call_output",
            call_id: c.id,
            output: JSON.stringify(c.output),
          });
        }
      }
      if (parts.length > 0) {
        input.push({ type: "message", role: "user", content: parts });
      }
      continue;
    }

    for (const c of msg.content) {
      if (c.type === "text") {
        input.push({
          type: "message",
          role: "assistant",
          content: c.text,
        });
        continue;
      }
      if (c.type === "tool_call") {
        input.push({
          type: "function_call",
          call_id: c.id,
          name: c.name,
          arguments: JSON.stringify(c.arguments),
        });
      }
    }
  }

  const tools = ir.tools?.map((t) => ({
    type: "function" as const,
    name: t.name,
    description: t.description,
    parameters: { type: "object", ...t.inputSchema },
    strict: null,
  }));

  const toolChoice = mapToolChoice(ir.toolChoice);

  return { input, tools, toolChoice };
}

export function buildOpenAIRequest(
  ir: IR.Request,
  model: string,
  modelConfig: ModelConfig,
): ResponseCreateParamsNonStreaming {
  const { input, tools, toolChoice } = buildInput(ir);

  const reasoning = modelConfig.supportsReasoningSummaries
    ? {
        effort:
          ir.thinking?.effort ?? modelConfig.defaultReasoningEffort ?? "medium",
        summary: "auto" as const,
      }
    : undefined;

  return {
    model,
    input,
    store: true,
    parallel_tool_calls: modelConfig.supportsParallelToolCalls,
    ...(reasoning && { reasoning, include: ["reasoning.encrypted_content"] }),
    ...(tools?.length && { tools }),
    ...(toolChoice && { tool_choice: toolChoice }),
    ...(ir.maxTokens &&
      ir.maxTokens >= 16 && { max_output_tokens: ir.maxTokens }),
    ...(ir.temperature !== undefined && { temperature: ir.temperature }),
    ...(ir.topP !== undefined && { top_p: ir.topP }),
  };
}

export function toResponsesRequest(
  ir: IR.Request,
  resolved?: { model: string; config: ModelConfig },
): ResponseCreateParamsNonStreaming {
  const resolution = resolved ?? resolveModelConfig(ir.model);
  return buildOpenAIRequest(ir, resolution.model, resolution.config);
}
