import { describe, it, expect, beforeAll, afterAll } from "bun:test";
import Anthropic from "@anthropic-ai/sdk";
import { app } from "../../src/server.js";

let server: ReturnType<typeof Bun.serve>;
let client: Anthropic;
let baseURL: string;

beforeAll(() => {
  server = Bun.serve({ fetch: app.fetch, port: 0 });
  baseURL = `http://localhost:${String(server.port)}`;
  client = new Anthropic({ baseURL, apiKey: "dummy" });
});

afterAll(() => {
  void server.stop();
});

describe("proxy integration", () => {
  it("streams text response", async () => {
    const stream = client.messages.stream({
      model: "claude-opus-4-5-20250929",
      max_tokens: 1000,
      messages: [{ role: "user", content: "What is 2 + 2?" }],
    });

    const events: string[] = [];
    stream.on("text", (text) => events.push(text));

    const message = await stream.finalMessage();

    expect(message.role).toBe("assistant");
    expect(message.content.length).toBeGreaterThan(0);
    const textBlock = message.content.find((c) => c.type === "text");
    expect(textBlock).toBeDefined();
    expect(events.length).toBeGreaterThan(0);
  }, 30000);

  it("non-streaming text response", async () => {
    const message = await client.messages.create({
      model: "claude-opus-4-5-20250929",
      max_tokens: 1000,
      messages: [{ role: "user", content: "What is 2 + 2?" }],
    });

    expect(message.role).toBe("assistant");
    expect(message.content.length).toBeGreaterThan(0);
    const textBlock = message.content.find((c) => c.type === "text");
    expect(textBlock).toBeDefined();
  });

  it("tool use", async () => {
    const message = await client.messages.create({
      model: "claude-opus-4-5-20250929",
      max_tokens: 1000,
      messages: [
        { role: "user", content: "What is 2 + 2? Use the calculator." },
      ],
      tools: [
        {
          name: "calculator",
          description: "A calculator that adds two numbers",
          input_schema: {
            type: "object" as const,
            properties: {
              a: { type: "number" },
              b: { type: "number" },
            },
            required: ["a", "b"],
          },
        },
      ],
    });

    expect(message.role).toBe("assistant");
    const toolUse = message.content.find((c) => c.type === "tool_use");
    expect(toolUse).toBeDefined();
    expect(toolUse?.type).toBe("tool_use");
    expect(toolUse?.name).toBe("calculator");
    expect(toolUse?.input).toHaveProperty("a");
    expect(toolUse?.input).toHaveProperty("b");
  });
});
