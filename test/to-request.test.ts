import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../src/config/index.js", () => ({
  resolveModel: (m: string): string => m,
}));

vi.mock("../src/logger.js", () => ({
  logger: { info: vi.fn(), warn: vi.fn() },
}));

import { toResponsesRequest } from "../src/openai/to-request.js";
import type * as IR from "../src/ir/types.js";

function baseRequest(overrides: Partial<IR.Request> = {}): IR.Request {
  return {
    model: "claude-3",
    messages: [{ role: "user", content: [{ type: "text", text: "hello" }] }],
    stream: false,
    ...overrides,
  };
}

describe("toResponsesRequest", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("caps max_tokens to 16384", () => {
    const ir = baseRequest({ maxTokens: 100000 });
    const result = toResponsesRequest(ir);
    expect(result.max_output_tokens).toBe(16384);
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

  it("sets reasoning effort high when thinking enabled", () => {
    const ir = baseRequest({
      thinking: { type: "enabled", budgetTokens: 5000 },
    });
    const result = toResponsesRequest(ir);
    expect(result.reasoning).toEqual({ effort: "high", summary: "auto" });
  });

  it("sets reasoning effort medium without thinking", () => {
    const ir = baseRequest();
    const result = toResponsesRequest(ir);
    expect(result.reasoning).toEqual({ effort: "medium", summary: "auto" });
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
