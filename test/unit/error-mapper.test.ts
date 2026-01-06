import { describe, it, expect } from "bun:test";
import { Hono } from "hono";
import type { Context } from "hono";
import { handleError, streamError } from "../../src/anthropic/error-mapper.js";
import { APIError } from "openai";

function createAPIError(status: number, message: string): APIError {
  return new APIError(status, { message }, message, new Headers());
}

async function parseResponse(
  res: Response,
): Promise<{ body: unknown; status: number }> {
  const body: unknown = await res.json();
  return { body, status: res.status };
}

describe("error-mapper", () => {
  const app = new Hono();
  app.get("/test", (c: Context) => c.text("ok"));

  describe("handleError", () => {
    it("maps 401 to authentication_error", async () => {
      const c = await app.request("/test");
      const error = createAPIError(401, "Invalid API key");
      const res = handleError(c as unknown as Context, error, {
        reqId: "req_123",
      });
      const { body, status } = await parseResponse(res);

      expect(status).toBe(401);
      expect(body).toEqual({
        type: "error",
        error: { type: "authentication_error", message: "401 Invalid API key" },
      });
    });

    it("maps 429 to rate_limit_error", async () => {
      const c = await app.request("/test");
      const error = createAPIError(429, "Rate limit exceeded");
      const res = handleError(c as unknown as Context, error, {
        reqId: "req_123",
      });
      const { body, status } = await parseResponse(res);

      expect(status).toBe(429);
      expect(body).toEqual({
        type: "error",
        error: { type: "rate_limit_error", message: "429 Rate limit exceeded" },
      });
    });

    it("maps 500 to api_error with 502 status", async () => {
      const c = await app.request("/test");
      const error = createAPIError(500, "Internal server error");
      const res = handleError(c as unknown as Context, error, {
        reqId: "req_123",
      });
      const { body, status } = await parseResponse(res);

      expect(status).toBe(502);
      expect(body).toEqual({
        type: "error",
        error: { type: "api_error", message: "500 Internal server error" },
      });
    });

    it("maps 400 to invalid_request_error", async () => {
      const c = await app.request("/test");
      const error = createAPIError(400, "Bad request");
      const res = handleError(c as unknown as Context, error, {
        reqId: "req_123",
      });
      const { body, status } = await parseResponse(res);

      expect(status).toBe(400);
      expect(body).toEqual({
        type: "error",
        error: { type: "invalid_request_error", message: "400 Bad request" },
      });
    });

    it("maps 403 to permission_error", async () => {
      const c = await app.request("/test");
      const error = createAPIError(403, "Forbidden");
      const res = handleError(c as unknown as Context, error, {
        reqId: "req_123",
      });
      const { body, status } = await parseResponse(res);

      expect(status).toBe(403);
      expect(body).toEqual({
        type: "error",
        error: { type: "permission_error", message: "403 Forbidden" },
      });
    });

    it("maps 404 to not_found_error", async () => {
      const c = await app.request("/test");
      const error = createAPIError(404, "Not found");
      const res = handleError(c as unknown as Context, error, {
        reqId: "req_123",
      });
      const { body, status } = await parseResponse(res);

      expect(status).toBe(404);
      expect(body).toEqual({
        type: "error",
        error: { type: "not_found_error", message: "404 Not found" },
      });
    });

    it("handles generic Error", async () => {
      const c = await app.request("/test");
      const error = new Error("Something went wrong");
      const res = handleError(c as unknown as Context, error, {
        reqId: "req_123",
      });
      const { body, status } = await parseResponse(res);

      expect(status).toBe(502);
      expect(body).toEqual({
        type: "error",
        error: { type: "api_error", message: "Something went wrong" },
      });
    });

    it("maps 529 to overloaded_error", async () => {
      const c = await app.request("/test");
      const error = createAPIError(529, "Overloaded");
      const res = handleError(c as unknown as Context, error, {
        reqId: "req_123",
      });
      const { body, status } = await parseResponse(res);

      expect(status).toBe(529);
      expect(body).toEqual({
        type: "error",
        error: { type: "overloaded_error", message: "529 Overloaded" },
      });
    });
  });

  describe("streamError", () => {
    it("yields SSE error event for 429", () => {
      const error = createAPIError(429, "Rate limit exceeded");
      const events = [...streamError(error, { reqId: "req_123" })];

      expect(events.length).toBe(1);
      expect(events[0]).toContain("event: error");
      expect(events[0]).toContain("rate_limit_error");
    });

    it("yields SSE error event for 500", () => {
      const error = createAPIError(500, "Internal server error");
      const events = [...streamError(error, { reqId: "req_123" })];

      expect(events.length).toBe(1);
      expect(events[0]).toContain("event: error");
      expect(events[0]).toContain("api_error");
    });

    it("yields SSE error event for generic Error", () => {
      const error = new Error("Unexpected failure");
      const events = [...streamError(error, { reqId: "req_123" })];

      expect(events.length).toBe(1);
      expect(events[0]).toContain("event: error");
      expect(events[0]).toContain("api_error");
      expect(events[0]).toContain("Unexpected failure");
    });
  });
});
