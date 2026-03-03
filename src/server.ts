import { Hono } from "hono";
import { stream } from "hono/streaming";
import { validate, validateCountTokens } from "./anthropic/validate.js";
import { fromRequest } from "./anthropic/from-request.js";
import { toResponse } from "./anthropic/to-response.js";
import { StreamTranslator } from "./anthropic/to-stream.js";
import { handleError, streamError } from "./anthropic/error-mapper.js";
import { toResponsesRequest } from "./openai/to-request.js";
import { fromResponse } from "./openai/from-response.js";
import { fromStreamEvent } from "./openai/from-stream.js";
import type { OpenAIClient } from "./openai/client.js";
import { generateId } from "./ir/normalize.js";
import { logger } from "./logger.js";
import { countMessageTokens, countToolTokens } from "./tokenizer/index.js";
import { resolveModelConfig } from "./config/index.js";

export function createApp(client: OpenAIClient): Hono {
  const app = new Hono();

  app.post("/v1/messages/count_tokens", async (c) => {
    const body: unknown = await c.req.json();
    const validation = validateCountTokens(body);

    if (!validation.ok) {
      return c.json(
        {
          type: "error",
          error: { type: "invalid_request_error", message: validation.error },
        },
        400,
      );
    }

    const ir = fromRequest(validation.data);
    let inputTokens = countMessageTokens(ir.messages);

    if (ir.tools) {
      inputTokens += countToolTokens(ir.tools);
    }

    return c.json({ input_tokens: inputTokens });
  });

  app.post("/v1/messages", async (c) => {
    const body: unknown = await c.req.json();

    logger.debug({ body }, "incoming request body");

    const validation = validate(body);

    if (!validation.ok) {
      logger.warn({ error: validation.error }, "invalid request");
      return c.json(
        {
          type: "error",
          error: { type: "invalid_request_error", message: validation.error },
        },
        400,
      );
    }

    const ir = fromRequest(validation.data);
    const reqId = generateId("req");
    const resolved = resolveModelConfig(ir.model);
    c.header("x-request-id", reqId);

    logger.info(
      {
        reqId,
        model: ir.model,
        resolvedModel: resolved.model,
        stream: ir.stream,
        maxTokens: ir.maxTokens,
        tools: ir.tools?.length ?? 0,
        thinking: ir.thinking?.type === "enabled",
      },
      "request received",
    );

    if (!ir.stream) {
      const openaiReq = toResponsesRequest(ir, resolved);
      try {
        const openaiRes = await client.responses.create(openaiReq);
        const irRes = fromResponse(openaiRes);
        const anthropicRes = toResponse(irRes, ir.model);

        logger.info(
          {
            reqId,
            stopReason: irRes.stopReason,
            inputTokens: irRes.usage.inputTokens,
            outputTokens: irRes.usage.outputTokens,
          },
          "non-streaming response complete",
        );
        return c.json(anthropicRes);
      } catch (error) {
        return handleError(error, { reqId, model: resolved.model });
      }
    }

    c.header("Content-Type", "text/event-stream");
    c.header("Cache-Control", "no-cache");
    c.header("Connection", "keep-alive");

    return stream(c, async (s) => {
      const openaiReq = toResponsesRequest(ir, resolved);
      try {
        const openaiStream = await client.responses.create({
          ...openaiReq,
          stream: true,
        });

        const translator = new StreamTranslator({ model: ir.model });

        for await (const event of openaiStream) {
          for (const irEvent of fromStreamEvent(event)) {
            for (const sse of translator.process(irEvent)) {
              await s.write(sse);
            }
          }
        }

        logger.info({ reqId }, "streaming response complete");
      } catch (error) {
        for (const sse of streamError(error, {
          reqId,
          model: resolved.model,
        })) {
          await s.write(sse);
        }
      }
    });
  });

  app.get("/health", (c) => c.json({ status: "ok" }));

  return app;
}
