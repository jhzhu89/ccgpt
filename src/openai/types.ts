import type {
  ResponseStreamEvent,
  ResponseOutputItem,
  ResponseOutputMessage,
  ResponseFunctionToolCall,
  ResponseOutputText,
  ResponseCreatedEvent,
  ResponseCompletedEvent,
  ResponseOutputItemAddedEvent,
  ResponseTextDeltaEvent,
  ResponseTextDoneEvent,
  ResponseFunctionCallArgumentsDeltaEvent,
  ResponseFunctionCallArgumentsDoneEvent,
  ResponseReasoningSummaryTextDeltaEvent,
  ResponseReasoningSummaryTextDoneEvent,
  ResponseFailedEvent,
  ResponseIncompleteEvent,
} from "openai/resources/responses/responses.js";

export function isCreatedEvent(
  event: ResponseStreamEvent,
): event is ResponseCreatedEvent {
  return event.type === "response.created";
}

export function isOutputMessage(
  item: ResponseOutputItem,
): item is ResponseOutputMessage {
  return item.type === "message";
}

export function isFunctionToolCall(
  item: ResponseOutputItem,
): item is ResponseFunctionToolCall {
  return item.type === "function_call";
}

export function isOutputText(
  content: ResponseOutputMessage["content"][number],
): content is ResponseOutputText {
  return content.type === "output_text";
}

export function isTextDeltaEvent(
  event: ResponseStreamEvent,
): event is ResponseTextDeltaEvent {
  return event.type === "response.output_text.delta";
}

export function isTextDoneEvent(
  event: ResponseStreamEvent,
): event is ResponseTextDoneEvent {
  return event.type === "response.output_text.done";
}

export function isFunctionCallArgumentsDeltaEvent(
  event: ResponseStreamEvent,
): event is ResponseFunctionCallArgumentsDeltaEvent {
  return event.type === "response.function_call_arguments.delta";
}

export function isFunctionCallArgumentsDoneEvent(
  event: ResponseStreamEvent,
): event is ResponseFunctionCallArgumentsDoneEvent {
  return event.type === "response.function_call_arguments.done";
}

export function isReasoningTextDeltaEvent(
  event: ResponseStreamEvent,
): event is ResponseReasoningSummaryTextDeltaEvent {
  return event.type === "response.reasoning_summary_text.delta";
}

export function isReasoningTextDoneEvent(
  event: ResponseStreamEvent,
): event is ResponseReasoningSummaryTextDoneEvent {
  return event.type === "response.reasoning_summary_text.done";
}

export function isOutputItemAddedEvent(
  event: ResponseStreamEvent,
): event is ResponseOutputItemAddedEvent {
  return event.type === "response.output_item.added";
}

export function isCompletedEvent(
  event: ResponseStreamEvent,
): event is ResponseCompletedEvent {
  return event.type === "response.completed";
}

export function isFailedEvent(
  event: ResponseStreamEvent,
): event is ResponseFailedEvent {
  return event.type === "response.failed";
}

export function isIncompleteEvent(
  event: ResponseStreamEvent,
): event is ResponseIncompleteEvent {
  return event.type === "response.incomplete";
}
