import type * as IR from "../ir/types.js";
import type {
  ResponseStreamEvent,
  ResponseOutputItemAddedEvent,
  ResponseReasoningItem,
} from "openai/resources/responses/responses.js";
import {
  isCreatedEvent,
  isTextDeltaEvent,
  isTextDoneEvent,
  isFunctionCallArgumentsDeltaEvent,
  isFunctionCallArgumentsDoneEvent,
  isOutputItemAddedEvent,
  isOutputItemDoneEvent,
  isCompletedEvent,
  isFailedEvent,
  isIncompleteEvent,
} from "./types.js";
import { mapOpenAIStopReason } from "./stop-reason.js";

function isFunctionCallItem(
  event: ResponseOutputItemAddedEvent,
): event is ResponseOutputItemAddedEvent & {
  item: { type: "function_call"; call_id: string; name: string };
} {
  return event.item.type === "function_call";
}

function isReasoningItem(item: {
  type: string;
}): item is ResponseReasoningItem {
  return item.type === "reasoning";
}

export function* fromStreamEvent(
  event: ResponseStreamEvent,
): Generator<IR.StreamEvent> {
  if (isCreatedEvent(event)) {
    yield { type: "stream_start" };
  } else if (isTextDeltaEvent(event)) {
    yield { type: "text_delta", text: event.delta };
  } else if (isTextDoneEvent(event)) {
    yield { type: "text_done" };
  } else if (isFunctionCallArgumentsDeltaEvent(event)) {
    yield { type: "tool_call_delta", id: event.item_id, args: event.delta };
  } else if (isFunctionCallArgumentsDoneEvent(event)) {
    yield { type: "tool_call_done", id: event.item_id };
  } else if (isOutputItemAddedEvent(event) && isFunctionCallItem(event)) {
    yield {
      type: "tool_call_start",
      id: event.item.call_id,
      name: event.item.name,
    };
  } else if (
    isOutputItemDoneEvent(event) &&
    isReasoningItem(event.item) &&
    typeof event.item.encrypted_content === "string"
  ) {
    yield { type: "reasoning_done", item: event.item };
  } else if (isFailedEvent(event)) {
    yield {
      type: "error",
      message: event.response.error?.message ?? "Unknown error",
    };
  } else if (isIncompleteEvent(event)) {
    yield {
      type: "done",
      stopReason: "max_tokens" as const,
      usage: {
        inputTokens: event.response.usage?.input_tokens ?? 0,
        outputTokens: event.response.usage?.output_tokens ?? 0,
      },
    };
  } else if (isCompletedEvent(event)) {
    const hasToolCall = event.response.output.some(
      (item) => item.type === "function_call",
    );
    yield {
      type: "done",
      stopReason: mapOpenAIStopReason(event.response.status, hasToolCall),
      usage: {
        inputTokens: event.response.usage?.input_tokens ?? 0,
        outputTokens: event.response.usage?.output_tokens ?? 0,
      },
    };
  }
}
