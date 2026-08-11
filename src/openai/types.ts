import type {
  ResponseStreamEvent,
  ResponseOutputItem,
  ResponseOutputMessage,
  ResponseFunctionToolCall,
  ResponseOutputText,
  ResponseCreatedEvent,
  ResponseCompletedEvent,
  ResponseOutputItemAddedEvent,
  ResponseOutputItemDoneEvent,
  ResponseTextDeltaEvent,
  ResponseTextDoneEvent,
  ResponseFunctionCallArgumentsDeltaEvent,
  ResponseFunctionCallArgumentsDoneEvent,
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

export function isOutputItemAddedEvent(
  event: ResponseStreamEvent,
): event is ResponseOutputItemAddedEvent {
  return event.type === "response.output_item.added";
}

export function isOutputItemDoneEvent(
  event: ResponseStreamEvent,
): event is ResponseOutputItemDoneEvent {
  return event.type === "response.output_item.done";
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
