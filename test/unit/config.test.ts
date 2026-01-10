import { describe, it, expect } from "bun:test";
import { resolveModel, config } from "../../src/config/index.js";

describe("resolveModel", () => {
  it("maps haiku to configured tier", () => {
    expect(resolveModel("claude-3-haiku-20240307")).toBe(config.tiers.haiku);
  });

  it("maps sonnet to configured tier", () => {
    expect(resolveModel("claude-3-5-sonnet-20241022")).toBe(
      config.tiers.sonnet,
    );
  });

  it("maps opus to configured tier", () => {
    expect(resolveModel("claude-opus-4-5-20250929")).toBe(config.tiers.opus);
  });

  it("returns alias unchanged for unknown models", () => {
    expect(resolveModel("unknown-model")).toBe("unknown-model");
  });
});
