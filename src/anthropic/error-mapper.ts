import { APIError } from "openai";
import type { Context } from "hono";
import { logger } from "../logger.js";
import { sseEvent } from "./sse.js";

type AnthropicErrorType =
  | "invalid_request_error"
  | "authentication_error"
  | "permission_error"
  | "not_found_error"
  | "rate_limit_error"
  | "api_error"
  | "overloaded_error";

interface ErrorContext {
  reqId: string;
  model?: string;
}

function mapStatusToErrorType(status: number): AnthropicErrorType {
  switch (status) {
    case 400:
      return "invalid_request_error";
    case 401:
      return "authentication_error";
    case 403:
      return "permission_error";
    case 404:
      return "not_found_error";
    case 429:
      return "rate_limit_error";
    case 529:
      return "overloaded_error";
    default:
      return "api_error";
  }
}

function mapStatusToHttpCode(status: number): number {
  if (status === 529) return 529;
  if (status >= 500) return 502;
  return status;
}

function extractMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return "An unexpected error occurred";
}

function extractStatus(error: unknown): number {
  if (error instanceof APIError && typeof error.status === "number") {
    return error.status;
  }
  return 500;
}

function extractUpstreamRequestId(error: unknown): string | null {
  if (error instanceof APIError) {
    const headers = error.headers as Headers | undefined;
    return headers?.get("x-request-id") ?? null;
  }
  return null;
}

export function handleError(
  c: Context,
  error: unknown,
  ctx: ErrorContext,
): Response {
  const status = extractStatus(error);
  const message = extractMessage(error);
  const errorType = mapStatusToErrorType(status);
  const httpCode = mapStatusToHttpCode(status);

  logger.error(
    {
      reqId: ctx.reqId,
      model: ctx.model,
      status,
      errorType,
      message,
      upstreamRequestId: extractUpstreamRequestId(error),
    },
    "upstream error",
  );

  return new Response(
    JSON.stringify({
      type: "error",
      error: { type: errorType, message },
    }),
    {
      status: httpCode,
      headers: { "Content-Type": "application/json" },
    },
  );
}

export function* streamError(
  error: unknown,
  ctx: ErrorContext,
): Generator<string> {
  const status = extractStatus(error);
  const message = extractMessage(error);
  const errorType = mapStatusToErrorType(status);

  logger.error(
    {
      reqId: ctx.reqId,
      model: ctx.model,
      status,
      errorType,
      message,
      upstreamRequestId: extractUpstreamRequestId(error),
    },
    "upstream streaming error",
  );

  yield sseEvent("error", {
    type: "error",
    error: { type: errorType, message },
  });
}
