import { describe, it, expect, beforeAll, afterAll } from "bun:test";
import Anthropic from "@anthropic-ai/sdk";
import type { ContentBlockParam } from "@anthropic-ai/sdk/resources/messages/messages.js";
import { createApp } from "../../src/server.js";
import { createClient } from "../../src/openai/client.js";
import { createCopilotModelRouter } from "../../src/config/index.js";

let server: ReturnType<typeof Bun.serve>;
let client: Anthropic;
let baseURL: string;

function replayableContent(
  content: Anthropic.Messages.ContentBlock[],
): ContentBlockParam[] {
  const replay: ContentBlockParam[] = [];
  for (const block of content) {
    if (block.type === "thinking") {
      replay.push({
        type: "thinking",
        thinking: block.thinking,
        signature: block.signature,
      });
    } else if (block.type === "text") {
      replay.push({ type: "text", text: block.text });
    } else if (block.type === "tool_use") {
      replay.push({
        type: "tool_use",
        id: block.id,
        name: block.name,
        input: block.input,
      });
    }
  }
  return replay;
}

beforeAll(async () => {
  const openai = await createClient({ kind: "copilot" });
  const models = await openai.models.list();
  const router = createCopilotModelRouter(models.data);
  const app = createApp(openai, (requested) => router.resolve(requested));
  server = Bun.serve({ fetch: app.fetch, port: 0 });
  baseURL = `http://localhost:${String(server.port)}`;
  client = new Anthropic({ baseURL, apiKey: "dummy" });
});

afterAll(async () => {
  await server.stop();
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
  }, 30000);

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

  it("replays streamed reasoning through a tool round trip", async () => {
    const prompt =
      "Three warehouses hold 17, 23, and 29 crates. Each crate contains 13 boxes, and each box weighs 7 kg. Determine the total weight. Use the multiply tool exactly once after working out the two factors.";
    const tools = [
      {
        name: "multiply",
        description: "Multiply two numbers",
        input_schema: {
          type: "object" as const,
          properties: {
            a: { type: "number" },
            b: { type: "number" },
          },
          required: ["a", "b"],
        },
      },
    ];

    const first = await client.messages
      .stream({
        model: "opus",
        max_tokens: 2000,
        output_config: { effort: "high" },
        messages: [{ role: "user", content: prompt }],
        tools,
      })
      .finalMessage();

    const thinking = first.content.find((block) => block.type === "thinking");
    const toolUse = first.content.find((block) => block.type === "tool_use");
    expect(thinking?.type).toBe("thinking");
    expect(thinking?.signature).toStartWith("ccgpt:");
    expect(toolUse?.type).toBe("tool_use");
    if (toolUse?.type !== "tool_use") throw new Error("Tool call missing");
    expect(toolUse.input).toEqual({ a: 69, b: 91 });

    const second = await client.messages
      .stream({
        model: "opus",
        max_tokens: 2000,
        output_config: { effort: "high" },
        messages: [
          { role: "user", content: prompt },
          { role: "assistant", content: replayableContent(first.content) },
          {
            role: "user",
            content: [
              {
                type: "tool_result",
                tool_use_id: toolUse.id,
                content: "6279",
              },
            ],
          },
        ],
        tools,
      })
      .finalMessage();

    const answer = second.content.find((block) => block.type === "text");
    expect(answer?.type).toBe("text");
    expect(answer?.text).toMatch(/total weight/i);
  }, 60000);
});
