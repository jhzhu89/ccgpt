import { describe, it, expect, mock, beforeEach } from "bun:test";

void mock.module("../../src/config/index.js", () => ({
  resolveModel: (m: string): string => m,
}));

void mock.module(
  "../../src/logger.js",
  (): { logger: { info: () => void; warn: () => void } } => ({
    logger: { info: (): void => {}, warn: (): void => {} },
  }),
);

import { toResponsesRequest } from "../../src/openai/to-request.js";
import type * as IR from "../../src/ir/types.js";

function baseRequest(overrides: Partial<IR.Request> = {}): IR.Request {
  return {
    model: "claude-3",
    messages: [{ role: "user", content: [{ type: "text", text: "hello" }] }],
    stream: false,
    ...overrides,
  };
}

describe("toResponsesRequest", () => {
  beforeEach(() => {});

  it("passes max_tokens through", () => {
    const ir = baseRequest({ maxTokens: 100000 });
    const result = toResponsesRequest(ir);
    expect(result.max_output_tokens).toBe(100000);
  });

  it("preserves max_tokens under limit", () => {
    const ir = baseRequest({ maxTokens: 1000 });
    const result = toResponsesRequest(ir);
    expect(result.max_output_tokens).toBe(1000);
  });

  it("maps tool_choice any to required", () => {
    const ir = baseRequest({
      toolChoice: { type: "any" },
      tools: [{ name: "t", inputSchema: {} }],
    });
    const result = toResponsesRequest(ir);
    expect(result.tool_choice).toBe("required");
  });

  it("maps tool_choice tool to function", () => {
    const ir = baseRequest({
      toolChoice: { type: "tool", name: "calc" },
      tools: [{ name: "calc", inputSchema: {} }],
    });
    const result = toResponsesRequest(ir);
    expect(result.tool_choice).toEqual({
      type: "function",
      name: "calc",
    });
  });

  it("omits tool_choice for auto", () => {
    const ir = baseRequest({ toolChoice: { type: "auto" } });
    const result = toResponsesRequest(ir);
    expect(result.tool_choice).toBeUndefined();
  });

  it("sets reasoning when model supports it", () => {
    const ir = baseRequest({
      model: "gpt-5.1-codex-max",
      thinking: { type: "enabled", budgetTokens: 5000, effort: "high" },
    });
    const result = toResponsesRequest(ir);
    expect(result.reasoning).toEqual({ effort: "high", summary: "auto" });
  });

  it("omits reasoning for models that dont support summaries", () => {
    const ir = baseRequest({ model: "gpt-4o" });
    const result = toResponsesRequest(ir);
    expect(result.reasoning).toBeUndefined();
  });

  it("converts image content to input_image", () => {
    const ir = baseRequest({
      messages: [
        {
          role: "user",
          content: [{ type: "image", url: "data:image/png;base64,abc" }],
        },
      ],
    });
    const result = toResponsesRequest(ir);
    expect(result.input).toBeDefined();
    const input = result.input as Array<{ content: unknown[] }>;
    expect(input[0].content[0]).toEqual({
      type: "input_image",
      image_url: "data:image/png;base64,abc",
      detail: "auto",
    });
  });
});
