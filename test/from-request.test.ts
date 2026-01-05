import { describe, it, expect } from "vitest";
import type { ValidatedRequest } from "../src/anthropic/validate.js";
import { fromRequest } from "../src/anthropic/from-request.js";

describe("fromRequest", () => {
  it("parses basic text message", () => {
    const body: ValidatedRequest = {
      model: "claude-3",
      max_tokens: 100,
      messages: [{ role: "user", content: "hello" }],
    };
    const ir = fromRequest(body);
    expect(ir.messages).toHaveLength(1);
    expect(ir.messages[0].content[0]).toEqual({ type: "text", text: "hello" });
  });

  it("parses system message", () => {
    const body: ValidatedRequest = {
      model: "claude-3",
      max_tokens: 100,
      system: "You are helpful",
      messages: [{ role: "user", content: "hi" }],
    };
    const ir = fromRequest(body);
    expect(ir.messages[0].role).toBe("system");
    expect(ir.messages[0].content[0]).toEqual({
      type: "text",
      text: "You are helpful",
    });
  });

  it("parses stop_sequences", () => {
    const body: ValidatedRequest = {
      model: "claude-3",
      max_tokens: 100,
      messages: [{ role: "user", content: "hi" }],
      stop_sequences: ["STOP", "END"],
    };
    const ir = fromRequest(body);
    expect(ir.stopSequences).toEqual(["STOP", "END"]);
  });

  it("parses tool_choice auto", () => {
    const body: ValidatedRequest = {
      model: "claude-3",
      max_tokens: 100,
      messages: [{ role: "user", content: "hi" }],
      tool_choice: { type: "auto" },
    };
    const ir = fromRequest(body);
    expect(ir.toolChoice).toEqual({ type: "auto" });
  });

  it("parses tool_choice any", () => {
    const body: ValidatedRequest = {
      model: "claude-3",
      max_tokens: 100,
      messages: [{ role: "user", content: "hi" }],
      tool_choice: { type: "any" },
    };
    const ir = fromRequest(body);
    expect(ir.toolChoice).toEqual({ type: "any" });
  });

  it("parses tool_choice tool", () => {
    const body: ValidatedRequest = {
      model: "claude-3",
      max_tokens: 100,
      messages: [{ role: "user", content: "hi" }],
      tool_choice: { type: "tool", name: "calculator" },
    };
    const ir = fromRequest(body);
    expect(ir.toolChoice).toEqual({ type: "tool", name: "calculator" });
  });

  it("parses thinking config", () => {
    const body: ValidatedRequest = {
      model: "claude-3",
      max_tokens: 100,
      messages: [{ role: "user", content: "hi" }],
      thinking: { type: "enabled", budget_tokens: 5000 },
    };
    const ir = fromRequest(body);
    expect(ir.thinking).toEqual({ type: "enabled", budgetTokens: 5000 });
  });

  it("parses image block with base64", () => {
    const body: ValidatedRequest = {
      model: "claude-3",
      max_tokens: 100,
      messages: [
        {
          role: "user",
          content: [
            {
              type: "image",
              source: {
                type: "base64",
                media_type: "image/png",
                data: "iVBORw0KGgo=",
              },
            },
          ],
        },
      ],
    };
    const ir = fromRequest(body);
    expect(ir.messages[0].content[0]).toEqual({
      type: "image",
      url: "data:image/png;base64,iVBORw0KGgo=",
    });
  });

  it("parses tools", () => {
    const body: ValidatedRequest = {
      model: "claude-3",
      max_tokens: 100,
      messages: [{ role: "user", content: "hi" }],
      tools: [
        {
          name: "calc",
          description: "Calculator",
          input_schema: { type: "object", properties: {} },
        },
      ],
    };
    const ir = fromRequest(body);
    expect(ir.tools).toHaveLength(1);
    expect(ir.tools?.[0].name).toBe("calc");
  });
});
