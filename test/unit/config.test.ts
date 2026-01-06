import { describe, it, expect } from "bun:test";
import { resolveModel } from "../../src/config/index.js";

describe("resolveModel", () => {
  it("maps haiku to default tier", () => {
    expect(resolveModel("claude-3-haiku-20240307")).toBe("gpt-5-mini");
  });

  it("maps sonnet to default tier", () => {
    expect(resolveModel("claude-3-5-sonnet-20241022")).toBe("gpt-5.2");
  });

  it("maps opus to default tier", () => {
    expect(resolveModel("claude-opus-4-5-20250929")).toBe("gpt-5.1-codex-max");
  });

  it("returns alias unchanged for unknown models", () => {
    expect(resolveModel("unknown-model")).toBe("unknown-model");
  });
});
