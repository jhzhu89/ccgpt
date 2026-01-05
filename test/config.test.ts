import { describe, it, expect, beforeEach, vi } from "vitest";

describe("resolveModel", () => {
  beforeEach(() => {
    vi.resetModules();
    delete process.env.MODEL_MAP;
    delete process.env.TIER_HAIKU;
    delete process.env.TIER_SONNET;
    delete process.env.TIER_OPUS;
  });

  it("returns exact match from MODEL_MAP", async () => {
    process.env.MODEL_MAP = JSON.stringify({ "claude-exact": "mapped-model" });
    const { resolveModel } = await import("../src/config/index.js");
    expect(resolveModel("claude-exact")).toBe("mapped-model");
  });

  it("maps haiku to default tier", async () => {
    const { resolveModel } = await import("../src/config/index.js");
    expect(resolveModel("claude-3-haiku-20240307")).toBe("gpt-5-mini");
  });

  it("maps sonnet to default tier", async () => {
    const { resolveModel } = await import("../src/config/index.js");
    expect(resolveModel("claude-3-5-sonnet-20241022")).toBe("gpt-5.2");
  });

  it("maps opus to default tier", async () => {
    const { resolveModel } = await import("../src/config/index.js");
    expect(resolveModel("claude-opus-4-5-20250929")).toBe("gpt-5.1-codex-max");
  });

  it("respects custom TIER_HAIKU", async () => {
    process.env.TIER_HAIKU = "gpt-5.1-nano";
    const { resolveModel } = await import("../src/config/index.js");
    expect(resolveModel("claude-haiku-3")).toBe("gpt-5.1-nano");
  });

  it("respects custom TIER_SONNET", async () => {
    process.env.TIER_SONNET = "gpt-5.1";
    const { resolveModel } = await import("../src/config/index.js");
    expect(resolveModel("claude-sonnet-4")).toBe("gpt-5.1");
  });

  it("respects custom TIER_OPUS", async () => {
    process.env.TIER_OPUS = "gpt-5.1-codex";
    const { resolveModel } = await import("../src/config/index.js");
    expect(resolveModel("claude-opus-4")).toBe("gpt-5.1-codex");
  });

  it("returns alias unchanged for unknown models", async () => {
    const { resolveModel } = await import("../src/config/index.js");
    expect(resolveModel("unknown-model")).toBe("unknown-model");
  });

  it("prefers exact match over tier", async () => {
    process.env.MODEL_MAP = JSON.stringify({ "claude-sonnet-exact": "exact" });
    const { resolveModel } = await import("../src/config/index.js");
    expect(resolveModel("claude-sonnet-exact")).toBe("exact");
  });
});
