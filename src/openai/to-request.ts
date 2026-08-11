import type { ModelConfig, ResolvedModel } from "../config/index.js";
import type * as IR from "../ir/types.js";
import type {
  ResponseCreateParamsNonStreaming,
  ResponseInputContent,
  ResponseInputItem,
  ToolChoiceFunction,
} from "openai/resources/responses/responses.js";

function mapToolChoice(
  tc: IR.ToolChoice | undefined,
): "none" | "required" | ToolChoiceFunction | undefined {
  if (!tc || tc.type === "auto") return undefined;
  if (tc.type === "any") return "required";
  if (tc.type === "none") return "none";
  return { type: "function", name: tc.name };
}

function isTextBlock(value: unknown): value is { type: "text"; text: string } {
  return (
    typeof value === "object" &&
    value !== null &&
    "type" in value &&
    value.type === "text" &&
    "text" in value &&
    typeof value.text === "string"
  );
}

function toolResultText(result: IR.ToolResult): string {
  let output: string;
  if (typeof result.output === "string") {
    output = result.output;
  } else if (Array.isArray(result.output) && result.output.every(isTextBlock)) {
    output = result.output.map((item) => item.text).join("\n");
  } else {
    output = JSON.stringify(result.output);
  }
  return result.isError ? `Error: ${output}` : output;
}

function buildInput(ir: IR.Request): {
  input: ResponseInputItem[];
  tools?: ResponseCreateParamsNonStreaming["tools"];
  toolChoice?: "none" | "required" | ToolChoiceFunction;
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
      let parts: ResponseInputContent[] = [];
      const flushParts = (): void => {
        if (parts.length === 0) return;
        input.push({ type: "message", role: "user", content: parts });
        parts = [];
      };

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
          flushParts();
          input.push({
            type: "function_call_output",
            call_id: c.id,
            output: toolResultText(c),
          });
        }
      }
      flushParts();
      continue;
    }

    for (const c of msg.content) {
      if (c.type === "reasoning") {
        input.push(c.item);
        continue;
      }
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
    strict: false,
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

  if (
    ir.reasoningEffort &&
    !modelConfig.reasoningEfforts.includes(ir.reasoningEffort)
  ) {
    throw new Error(
      `${model} does not support reasoning effort ${ir.reasoningEffort}`,
    );
  }

  const reasoningEffort =
    ir.reasoningEffort ??
    (modelConfig.reasoningEfforts.includes("medium") ? "medium" : undefined);
  const reasoning = reasoningEffort ? { effort: reasoningEffort } : undefined;
  const parallelDisabled =
    ir.toolChoice !== undefined &&
    ir.toolChoice.type !== "none" &&
    ir.toolChoice.disableParallelToolUse === true;
  const parallelToolCalls = tools?.length
    ? modelConfig.supportsParallelToolCalls &&
      toolChoice !== "none" &&
      !parallelDisabled
    : undefined;

  return {
    model,
    input,
    ...(parallelToolCalls !== undefined && {
      parallel_tool_calls: parallelToolCalls,
    }),
    ...(reasoning && { reasoning, include: ["reasoning.encrypted_content"] }),
    ...(tools?.length && { tools }),
    ...(toolChoice && { tool_choice: toolChoice }),
    ...(ir.maxTokens !== undefined && { max_output_tokens: ir.maxTokens }),
    ...(ir.temperature !== undefined && { temperature: ir.temperature }),
    ...(ir.topP !== undefined && { top_p: ir.topP }),
  };
}

export function toResponsesRequest(
  ir: IR.Request,
  resolved: ResolvedModel,
): ResponseCreateParamsNonStreaming {
  return buildOpenAIRequest(ir, resolved.model, resolved.config);
}
