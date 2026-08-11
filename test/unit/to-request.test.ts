import { describe, expect, it } from "bun:test";
import type { ResolvedModel } from "../../src/config/index.js";
import type * as IR from "../../src/ir/types.js";
import { toResponsesRequest } from "../../src/openai/to-request.js";
import type { ResponseCreateParamsNonStreaming } from "openai/resources/responses/responses.js";

const resolved: ResolvedModel = {
  model: "gpt-5.6-sol",
  config: {
    supportsParallelToolCalls: true,
    reasoningEfforts: ["none", "low", "medium", "high", "xhigh", "max"],
  },
};

function baseRequest(overrides: Partial<IR.Request> = {}): IR.Request {
  return {
    model: "claude-3",
    messages: [{ role: "user", content: [{ type: "text", text: "hello" }] }],
    stream: false,
    ...overrides,
  };
}

function translate(
  overrides: Partial<IR.Request> = {},
): ResponseCreateParamsNonStreaming {
  return toResponsesRequest(baseRequest(overrides), resolved);
}

describe("toResponsesRequest", () => {
  it("passes max_tokens through", () => {
    expect(translate({ maxTokens: 100_000 }).max_output_tokens).toBe(100_000);
    expect(translate({ maxTokens: 8 }).max_output_tokens).toBe(8);
  });

  it("maps Anthropic tool choices", () => {
    expect(
      translate({
        toolChoice: { type: "any" },
        tools: [{ name: "t", inputSchema: {} }],
      }).tool_choice,
    ).toBe("required");
    expect(
      translate({
        toolChoice: { type: "tool", name: "calc" },
        tools: [{ name: "calc", inputSchema: {} }],
      }).tool_choice,
    ).toEqual({ type: "function", name: "calc" });
    expect(
      translate({ toolChoice: { type: "auto" } }).tool_choice,
    ).toBeUndefined();
    expect(translate({ toolChoice: { type: "none" } }).tool_choice).toBe(
      "none",
    );
  });

  it("preserves mid-conversation system messages", () => {
    const result = translate({
      messages: [
        {
          role: "user",
          content: [{ type: "text", text: "first" }],
        },
        {
          role: "system",
          content: [{ type: "text", text: "updated constraint" }],
        },
        {
          role: "user",
          content: [{ type: "text", text: "continue" }],
        },
      ],
    });
    expect((result.input as Array<{ role?: string }>)[1].role).toBe("system");
  });

  it("preserves max reasoning effort", () => {
    const result = translate({ reasoningEffort: "max" });
    expect(result.reasoning).toEqual({ effort: "max" });
    expect(result.include).toEqual(["reasoning.encrypted_content"]);
  });

  it("uses medium reasoning by default", () => {
    expect(translate().reasoning).toEqual({ effort: "medium" });
  });

  it("honors disable_parallel_tool_use", () => {
    const result = translate({
      toolChoice: { type: "auto", disableParallelToolUse: true },
      tools: [{ name: "read", inputSchema: {} }],
    });
    expect(result.parallel_tool_calls).toBeFalse();
  });

  it("enables parallel tools only when usable tools are present", () => {
    expect(translate().parallel_tool_calls).toBeUndefined();
    expect(
      translate({ tools: [{ name: "read", inputSchema: {} }] })
        .parallel_tool_calls,
    ).toBeTrue();
    expect(
      translate({
        tools: [{ name: "read", inputSchema: {} }],
        toolChoice: { type: "none" },
      }).parallel_tool_calls,
    ).toBeFalse();
  });

  it("does not enable parallel tools for unsupported models", () => {
    const result = toResponsesRequest(
      baseRequest({ tools: [{ name: "read", inputSchema: {} }] }),
      {
        ...resolved,
        config: { ...resolved.config, supportsParallelToolCalls: false },
      },
    );
    expect(result.parallel_tool_calls).toBeFalse();
  });

  it("converts image content to input_image", () => {
    const result = translate({
      messages: [
        {
          role: "user",
          content: [{ type: "image", url: "data:image/png;base64,abc" }],
        },
      ],
    });
    const input = result.input as Array<{ content: unknown[] }>;
    expect(input[0].content[0]).toEqual({
      type: "input_image",
      image_url: "data:image/png;base64,abc",
      detail: "auto",
    });
  });

  it("keeps tool output text unquoted and preserves item order", () => {
    const result = translate({
      messages: [
        {
          role: "user",
          content: [
            { type: "text", text: "before" },
            { type: "tool_result", id: "call_1", output: "result" },
            { type: "text", text: "after" },
          ],
        },
      ],
    });
    const input = result.input as Array<Record<string, unknown>>;
    expect(input.map((item) => item.type)).toEqual([
      "message",
      "function_call_output",
      "message",
    ]);
    expect(input[1].output).toBe("result");
  });

  it("replays opaque reasoning items", () => {
    const item: IR.ReasoningItem = {
      id: "reasoning_1",
      type: "reasoning",
      summary: [],
      encrypted_content: "encrypted",
    };
    const result = translate({
      messages: [
        {
          role: "assistant",
          content: [{ type: "reasoning", item }],
        },
      ],
    });
    expect((result.input as unknown[])[0]).toEqual(item);
  });
});
