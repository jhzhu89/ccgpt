import type * as IR from "../ir/types.js";
import { sseEvent } from "./sse.js";
import { generateId } from "../ir/normalize.js";
import { encodeReasoningItem } from "./reasoning.js";

export interface StreamContext {
  model: string;
}

export class StreamTranslator {
  private messageId = generateId("msg");
  private blockIndex = 0;
  private blockOpen = false;
  private model: string;

  constructor(ctx: StreamContext) {
    this.model = ctx.model;
  }

  *process(event: IR.StreamEvent): Generator<string> {
    switch (event.type) {
      case "stream_start":
        yield* this.handleStreamStart();
        break;
      case "reasoning_done":
        yield* this.handleReasoningDone(event);
        break;
      case "text_delta":
        yield* this.handleTextDelta(event);
        break;
      case "text_done":
        yield* this.handleTextDone();
        break;
      case "tool_call_start":
        yield* this.handleToolCallStart(event);
        break;
      case "tool_call_delta":
        yield* this.handleToolCallDelta(event);
        break;
      case "tool_call_done":
        yield* this.handleToolCallDone();
        break;
      case "error":
        yield* this.handleError(event);
        break;
      case "done":
        yield* this.handleDone(event);
        break;
    }
  }

  private *handleStreamStart(): Generator<string> {
    yield sseEvent("message_start", {
      type: "message_start",
      message: {
        id: this.messageId,
        type: "message",
        role: "assistant",
        content: [],
        model: this.model,
        stop_reason: null,
        usage: { input_tokens: 0, output_tokens: 0 },
      },
    });
  }

  private *closeBlock(): Generator<string> {
    if (!this.blockOpen) return;
    yield sseEvent("content_block_stop", {
      type: "content_block_stop",
      index: this.blockIndex,
    });
    this.blockIndex++;
    this.blockOpen = false;
  }

  private *handleReasoningDone(event: IR.ReasoningDone): Generator<string> {
    yield* this.closeBlock();
    yield sseEvent("content_block_start", {
      type: "content_block_start",
      index: this.blockIndex,
      content_block: { type: "thinking", thinking: "", signature: "" },
    });
    yield sseEvent("content_block_delta", {
      type: "content_block_delta",
      index: this.blockIndex,
      delta: {
        type: "signature_delta",
        signature: encodeReasoningItem(event.item),
      },
    });
    yield sseEvent("content_block_stop", {
      type: "content_block_stop",
      index: this.blockIndex,
    });
    this.blockIndex++;
  }

  private *handleTextDelta(event: IR.TextDelta): Generator<string> {
    if (!this.blockOpen) {
      yield sseEvent("content_block_start", {
        type: "content_block_start",
        index: this.blockIndex,
        content_block: { type: "text", text: "" },
      });
      this.blockOpen = true;
    }
    yield sseEvent("content_block_delta", {
      type: "content_block_delta",
      index: this.blockIndex,
      delta: { type: "text_delta", text: event.text },
    });
  }

  private *handleTextDone(): Generator<string> {
    yield* this.closeBlock();
  }

  private *handleToolCallStart(event: IR.ToolCallStart): Generator<string> {
    yield sseEvent("content_block_start", {
      type: "content_block_start",
      index: this.blockIndex,
      content_block: {
        type: "tool_use",
        id: event.id,
        name: event.name,
        input: {},
      },
    });
    this.blockOpen = true;
  }

  private *handleToolCallDelta(event: IR.ToolCallDelta): Generator<string> {
    yield sseEvent("content_block_delta", {
      type: "content_block_delta",
      index: this.blockIndex,
      delta: { type: "input_json_delta", partial_json: event.args },
    });
  }

  private *handleToolCallDone(): Generator<string> {
    yield* this.closeBlock();
  }

  private *handleError(event: IR.StreamError): Generator<string> {
    yield sseEvent("error", {
      type: "error",
      error: { type: "api_error", message: event.message },
    });
  }

  private *handleDone(event: IR.Done): Generator<string> {
    yield sseEvent("message_delta", {
      type: "message_delta",
      delta: { stop_reason: event.stopReason },
      usage: { output_tokens: event.usage.outputTokens },
    });
    yield sseEvent("message_stop", { type: "message_stop" });
  }
}
