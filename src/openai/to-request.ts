import type * as IR from "../ir/types.js";
import { resolveModel } from "../config/index.js";
import type {
  ResponseCreateParamsNonStreaming,
  ResponseInputItem,
  ResponseInputContent,
  ToolChoiceFunction,
} from "openai/resources/responses/responses.js";

function isDictionaryType(schema: Record<string, unknown>): boolean {
  if (schema.type !== "object") return false;
  const additionalProps = schema.additionalProperties;
  if (typeof additionalProps === "object" && additionalProps !== null) {
    const props = schema.properties;
    if (
      !props ||
      (typeof props === "object" && Object.keys(props).length === 0)
    ) {
      return true;
    }
  }
  return false;
}

function toStrictSchema(
  schema: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const [key, value] of Object.entries(schema)) {
    if (key === "format" || key === "required") continue;
    if (key === "additionalProperties") continue;

    if (key === "properties" && typeof value === "object" && value !== null) {
      const props: Record<string, unknown> = {};
      for (const [propKey, propVal] of Object.entries(value)) {
        if (typeof propVal === "object" && propVal !== null) {
          if (isDictionaryType(propVal as Record<string, unknown>)) {
            continue;
          }
          props[propKey] = toStrictSchema(propVal as Record<string, unknown>);
        } else {
          props[propKey] = propVal;
        }
      }
      result.properties = props;
    } else if (key === "items" && typeof value === "object" && value !== null) {
      result.items = toStrictSchema(value as Record<string, unknown>);
    } else if (
      (key === "anyOf" || key === "oneOf" || key === "allOf") &&
      Array.isArray(value)
    ) {
      result[key] = (value as unknown[]).map((v): unknown =>
        typeof v === "object" && v !== null
          ? toStrictSchema(v as Record<string, unknown>)
          : v,
      );
    } else {
      result[key] = value;
    }
  }

  if (result.type === "object") {
    result.additionalProperties = false;
    const props = result.properties;
    result.required =
      typeof props === "object" && props !== null ? Object.keys(props) : [];
  }

  return result;
}

function mapToolChoice(
  tc: IR.ToolChoice | undefined,
): "required" | ToolChoiceFunction | undefined {
  if (!tc || tc.type === "auto") return undefined;
  if (tc.type === "any") return "required";
  return { type: "function", name: tc.name };
}

export function toResponsesRequest(
  ir: IR.Request,
): ResponseCreateParamsNonStreaming {
  const input: ResponseInputItem[] = [];

  for (const msg of ir.messages) {
    if (msg.role === "system") {
      input.push({
        role: "system",
        content: msg.content
          .map((c) => (c.type === "text" ? c.text : ""))
          .join(""),
      });
    } else if (msg.role === "user") {
      const parts: ResponseInputContent[] = [];
      for (const c of msg.content) {
        if (c.type === "text") {
          parts.push({ type: "input_text", text: c.text });
        } else if (c.type === "image") {
          parts.push({ type: "input_image", image_url: c.url, detail: "auto" });
        } else if (c.type === "tool_result") {
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
    } else {
      for (const c of msg.content) {
        if (c.type === "text") {
          input.push({
            type: "message",
            role: "assistant",
            content: c.text,
          });
        } else if (c.type === "tool_call") {
          input.push({
            type: "function_call",
            call_id: c.id,
            name: c.name,
            arguments: JSON.stringify(c.arguments),
          });
        }
      }
    }
  }

  const tools = ir.tools?.map((t) => {
    const params = toStrictSchema({ type: "object", ...t.inputSchema });
    return {
      type: "function" as const,
      name: t.name,
      description: t.description,
      parameters: params,
      strict: true,
    };
  });

  const reasoning =
    ir.thinking?.type === "enabled"
      ? { effort: "high" as const, summary: "auto" as const }
      : { effort: "medium" as const, summary: "auto" as const };

  const toolChoice = mapToolChoice(ir.toolChoice);

  return {
    model: resolveModel(ir.model),
    input,
    store: true,
    truncation: "auto",
    parallel_tool_calls: true,
    reasoning,
    ...(tools?.length && { tools }),
    ...(toolChoice && { tool_choice: toolChoice }),
    ...(ir.maxTokens && { max_output_tokens: ir.maxTokens }),
    ...(ir.temperature !== undefined && { temperature: ir.temperature }),
    ...(ir.topP !== undefined && { top_p: ir.topP }),
  };
}
