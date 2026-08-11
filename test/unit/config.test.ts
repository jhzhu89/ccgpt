import { describe, expect, it } from "bun:test";
import {
  createCopilotModelRouter,
  createDirectModelRouter,
  readConfig,
  type CopilotModel,
} from "../../src/config/index.js";

function model(id: string): CopilotModel {
  return {
    id,
    supported_endpoints: ["/responses"],
    capabilities: {
      supports: {
        parallel_tool_calls: true,
        reasoning_effort: ["none", "low", "medium", "high", "xhigh", "max"],
      },
    },
  };
}

describe("model routing", () => {
  const router = createCopilotModelRouter([
    model("gpt-5.6-sol"),
    model("gpt-5.6-terra"),
    model("gpt-5.6-luna"),
    model("gpt-5.7-sol"),
    model("gpt-5.7-terra"),
    model("gpt-5.7-luna"),
  ]);

  it("maps Claude model families to the latest matching tier", () => {
    expect(router.resolve("claude-fable-5").model).toBe("gpt-5.7-sol");
    expect(router.resolve("claude-opus-5").model).toBe("gpt-5.7-sol");
    expect(router.resolve("claude-sonnet-5").model).toBe("gpt-5.7-terra");
    expect(router.resolve("claude-haiku-4-5").model).toBe("gpt-5.7-luna");
  });

  it("routes unknown Claude families to the high tier", () => {
    expect(router.resolve("claude-future-6").model).toBe("gpt-5.7-sol");
  });

  it("passes an available Copilot model through exactly", () => {
    expect(router.resolve("gpt-5.6-terra").model).toBe("gpt-5.6-terra");
  });

  it("rejects an unavailable explicit Copilot model", () => {
    expect(() => router.resolve("gpt-9-sol")).toThrow("not available");
  });
});

describe("direct model routing", () => {
  const targets = {
    high: "gpt-5.6-sol",
    balanced: "gpt-5.6-terra",
    fast: "gpt-5.6-luna",
  };
  const router = createDirectModelRouter(targets);

  it("maps Claude tiers to configured models", () => {
    expect(router.resolve("opus").model).toBe(targets.high);
    expect(router.resolve("sonnet").model).toBe(targets.balanced);
    expect(router.resolve("haiku").model).toBe(targets.fast);
  });

  it("passes backend model IDs through", () => {
    expect(router.resolve("gpt-custom").model).toBe("gpt-custom");
    expect(router.resolve("o4-mini").model).toBe("o4-mini");
  });
});

describe("configuration", () => {
  it("uses Copilot by default", () => {
    expect(readConfig({}).backend).toEqual({ kind: "copilot" });
    expect(readConfig({ OPENAI_API_KEY: "generic-key" }).backend).toEqual({
      kind: "copilot",
    });
  });

  it("uses the API backend only with an explicit ccgpt key", () => {
    expect(
      readConfig({
        CCGPT_API_KEY: "key",
        CCGPT_BASE_URL: "https://example.com/v1",
        CCGPT_MODEL_HIGH: "gpt-high",
      }).backend,
    ).toEqual({
      kind: "openai",
      apiKey: "key",
      baseURL: "https://example.com/v1",
      targets: {
        high: "gpt-high",
        balanced: "gpt-5.6-terra",
        fast: "gpt-5.6-luna",
      },
    });
  });

  it("rejects a base URL without an API key", () => {
    expect(() =>
      readConfig({ CCGPT_BASE_URL: "https://example.com/v1" }),
    ).toThrow("requires CCGPT_API_KEY");
  });
});
