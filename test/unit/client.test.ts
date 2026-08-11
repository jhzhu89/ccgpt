import { describe, expect, it } from "bun:test";
import { createClient } from "../../src/openai/client.js";

describe("OpenAI client", () => {
  it("creates an API-key client without Copilot authentication", async () => {
    const client = await createClient({
      kind: "openai",
      apiKey: "test-key",
      baseURL: "https://example.com/v1",
      targets: {
        high: "gpt-high",
        balanced: "gpt-balanced",
        fast: "gpt-fast",
      },
    });

    expect(client.apiKey).toBe("test-key");
    expect(client.baseURL).toBe("https://example.com/v1");
  });

  it("sends Responses requests to the configured API-key backend", async () => {
    let url = "";
    let authorization = "";
    let body: Record<string, unknown> = {};
    const server = Bun.serve({
      port: 0,
      async fetch(request) {
        url = request.url;
        authorization = request.headers.get("authorization") ?? "";
        body = (await request.json()) as Record<string, unknown>;
        return Response.json({
          id: "resp_test",
          object: "response",
          output: [],
        });
      },
    });

    try {
      const client = await createClient({
        kind: "openai",
        apiKey: "test-key",
        baseURL: `http://127.0.0.1:${String(server.port)}/v1`,
        targets: {
          high: "gpt-high",
          balanced: "gpt-balanced",
          fast: "gpt-fast",
        },
      });

      await client.responses.create({ model: "gpt-exact", input: "hello" });

      expect(url).toEndWith("/v1/responses");
      expect(authorization).toBe("Bearer test-key");
      expect(body.model).toBe("gpt-exact");
    } finally {
      await server.stop();
    }
  });
});
