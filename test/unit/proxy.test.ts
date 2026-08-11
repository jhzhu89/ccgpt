import { describe, it, expect, beforeAll, afterAll } from "bun:test";
import { createApp } from "../../src/server.js";
import type { OpenAIClient } from "../../src/openai/client.js";

let server: ReturnType<typeof Bun.serve>;
let baseURL: string;

beforeAll(() => {
  const app = createApp(null as unknown as OpenAIClient, () => {
    throw new Error("model resolution is not expected in this test");
  });
  server = Bun.serve({ fetch: app.fetch, port: 0 });
  baseURL = `http://localhost:${String(server.port)}`;
});

afterAll(async () => {
  await server.stop();
});

describe("proxy", () => {
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
