import { describe, it, expect } from "vitest";
import { countMessageTokens, countToolTokens } from "../src/tokenizer/index.js";
import type * as IR from "../src/ir/types.js";

describe("countMessageTokens", () => {
  it("counts simple text message", () => {
    const messages: IR.Message[] = [
      { role: "user", content: [{ type: "text", text: "Hello world" }] },
    ];
    const count = countMessageTokens(messages);
    expect(count).toBeGreaterThan(0);
  });

  it("counts multiple messages", () => {
    const single: IR.Message[] = [
      { role: "user", content: [{ type: "text", text: "Hi" }] },
    ];
    const double: IR.Message[] = [
      { role: "user", content: [{ type: "text", text: "Hi" }] },
      { role: "assistant", content: [{ type: "text", text: "Hello" }] },
    ];
    expect(countMessageTokens(double)).toBeGreaterThan(
      countMessageTokens(single),
    );
  });

  it("counts tool call content", () => {
    const messages: IR.Message[] = [
      {
        role: "assistant",
        content: [
          {
            type: "tool_call",
            id: "call_1",
            name: "calculator",
            arguments: { a: 1, b: 2 },
          },
        ],
      },
    ];
    const count = countMessageTokens(messages);
    expect(count).toBeGreaterThan(0);
  });

  it("counts tool result content", () => {
    const messages: IR.Message[] = [
      {
        role: "user",
        content: [{ type: "tool_result", id: "call_1", output: { result: 3 } }],
      },
    ];
    const count = countMessageTokens(messages);
    expect(count).toBeGreaterThan(0);
  });

  it("counts image as 85 tokens", () => {
    const withImage: IR.Message[] = [
      {
        role: "user",
        content: [{ type: "image", url: "data:image/png;base64,abc" }],
      },
    ];
    const withoutImage: IR.Message[] = [
      { role: "user", content: [{ type: "text", text: "" }] },
    ];
    const diff =
      countMessageTokens(withImage) - countMessageTokens(withoutImage);
    expect(diff).toBe(85);
  });
});

describe("countToolTokens", () => {
  it("counts tool definitions", () => {
    const tools: IR.ToolDefinition[] = [
      {
        name: "calculator",
        description: "A simple calculator",
        inputSchema: {
          type: "object",
          properties: { a: { type: "number" }, b: { type: "number" } },
        },
      },
    ];
    const count = countToolTokens(tools);
    expect(count).toBeGreaterThan(0);
  });

  it("increases with more tools", () => {
    const one: IR.ToolDefinition[] = [{ name: "a", inputSchema: {} }];
    const two: IR.ToolDefinition[] = [
      { name: "a", inputSchema: {} },
      { name: "b", inputSchema: {} },
    ];
    expect(countToolTokens(two)).toBeGreaterThan(countToolTokens(one));
  });
});
