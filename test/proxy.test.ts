import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { serve, type ServerType } from "@hono/node-server";
import Anthropic from "@anthropic-ai/sdk";
import { app } from "../src/server.js";

let server: ServerType;
let client: Anthropic;
let baseURL: string;

beforeAll(async () => {
  server = serve({ fetch: app.fetch, port: 0 });
  await new Promise<void>((resolve) => server.on("listening", resolve));
  const addr = server.address();
  const port = typeof addr === "object" && addr ? addr.port : 8000;
  baseURL = `http://localhost:${String(port)}`;
  client = new Anthropic({ baseURL, apiKey: "dummy" });
});

afterAll(() => {
  server.close();
});

describe("proxy", () => {
  it("health check", async () => {
    const res = await fetch(`${baseURL}/health`);
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ status: "ok" });
  });

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
  });

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

  it("count_tokens returns token count", async () => {
    const res = await fetch(`${baseURL}/v1/messages/count_tokens`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        model: "claude-3",
        messages: [{ role: "user", content: "Hello world" }],
      }),
    });

    expect(res.status).toBe(200);
    const data = (await res.json()) as { input_tokens: number };
    expect(data.input_tokens).toBeGreaterThan(0);
  });

  it("count_tokens with tools", async () => {
    const res = await fetch(`${baseURL}/v1/messages/count_tokens`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        model: "claude-3",
        messages: [{ role: "user", content: "Hello" }],
        tools: [
          {
            name: "calc",
            description: "Calculator",
            input_schema: { type: "object", properties: {} },
          },
        ],
      }),
    });

    expect(res.status).toBe(200);
    const data = (await res.json()) as { input_tokens: number };
    expect(data.input_tokens).toBeGreaterThan(0);
  });

  it("count_tokens validates request", async () => {
    const res = await fetch(`${baseURL}/v1/messages/count_tokens`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ invalid: true }),
    });

    expect(res.status).toBe(400);
    const data = (await res.json()) as { type: string };
    expect(data.type).toBe("error");
  });

  it("messages validates request", async () => {
    const res = await fetch(`${baseURL}/v1/messages`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ invalid: true }),
    });

    expect(res.status).toBe(400);
    const data = (await res.json()) as { type: string };
    expect(data.type).toBe("error");
  });
});
