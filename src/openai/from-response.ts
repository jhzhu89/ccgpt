import type * as IR from "../ir/types.js";
import type {
  Response,
  ResponseOutputItem,
  ResponseOutputText,
} from "openai/resources/responses/responses.js";
import { mapOpenAIStopReason } from "./stop-reason.js";

function isOutputText(c: { type: string }): c is ResponseOutputText {
  return c.type === "output_text";
}

function parseOutputItem(item: ResponseOutputItem): IR.ResponseContent[] {
  if (item.type === "message") {
    return item.content.filter(isOutputText).map((c) => ({
      type: "text",
      text: c.text,
    }));
  }
  if (item.type === "function_call") {
    return [
      {
        type: "tool_call",
        id: item.call_id,
        name: item.name,
        arguments: JSON.parse(item.arguments),
      },
    ];
  }
  return [];
}

export function fromResponse(response: Response): IR.Response {
  const content = response.output.flatMap(parseOutputItem);
  const hasToolCall = content.some((c) => c.type === "tool_call");

  return {
    content,
    stopReason: mapOpenAIStopReason(response.status, hasToolCall),
    usage: {
      inputTokens: response.usage?.input_tokens ?? 0,
      outputTokens: response.usage?.output_tokens ?? 0,
    },
  };
}
