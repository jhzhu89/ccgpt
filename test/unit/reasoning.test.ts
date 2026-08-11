import { describe, expect, it } from "bun:test";
import type { ResponseOutputItemDoneEvent } from "openai/resources/responses/responses.js";
import {
  decodeReasoningItem,
  encodeReasoningItem,
} from "../../src/anthropic/reasoning.js";
import { toResponse } from "../../src/anthropic/to-response.js";
import { StreamTranslator } from "../../src/anthropic/to-stream.js";
import type { ReasoningItem } from "../../src/ir/types.js";
import { fromStreamEvent } from "../../src/openai/from-stream.js";

const item: ReasoningItem = {
  id: "reasoning_1",
  type: "reasoning",
  summary: [],
  encrypted_content: "encrypted",
};

describe("reasoning continuity", () => {
  it("round-trips a reasoning item through the Anthropic signature", () => {
    expect(decodeReasoningItem(encodeReasoningItem(item))).toEqual(item);
  });

  it("returns non-streaming reasoning as an opaque thinking block", () => {
    const response = toResponse(
      {
        content: [{ type: "reasoning", item }],
        stopReason: "end_turn",
        usage: { inputTokens: 1, outputTokens: 1 },
      },
      "claude-opus-5",
    );
    const block = response.content[0];
    expect(block.type).toBe("thinking");
    if (block.type !== "thinking") throw new Error("Expected thinking block");
    expect(block.thinking).toBe("");
    expect(decodeReasoningItem(block.signature)).toEqual(item);
  });

  it("maps completed reasoning stream items to signature deltas", () => {
    const event: ResponseOutputItemDoneEvent = {
      type: "response.output_item.done",
      output_index: 0,
      sequence_number: 1,
      item,
    };
    const irEvents = [...fromStreamEvent(event)];
    expect(irEvents).toEqual([{ type: "reasoning_done", item }]);

    const translator = new StreamTranslator({ model: "claude-opus-5" });
    const sse = [...translator.process(irEvents[0])].join("");
    expect(sse).toContain("signature_delta");
    expect(sse).toContain(encodeReasoningItem(item));
  });
});
