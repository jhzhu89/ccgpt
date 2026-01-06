/**
 * Collects all SSE events from a streaming request for debugging.
 * Run with: bun test/collect-events.ts
 */

import { createApp } from "../src/server.js";
import { createClient } from "../src/openai/client.js";
import { writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));

interface CollectedEvent {
  event: string;
  data: unknown;
  timestamp: number;
}

async function collectEvents(): Promise<void> {
  const app = createApp(createClient());
  const server = Bun.serve({ fetch: app.fetch, port: 0 });
  const baseURL = `http://localhost:${String(server.port)}`;

  console.log(`Server started on ${baseURL}`);

  const events: CollectedEvent[] = [];
  const startTime = Date.now();

  try {
    const response = await fetch(`${baseURL}/v1/messages`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "x-api-key": "dummy",
        "anthropic-version": "2023-06-01",
      },
      body: JSON.stringify({
        model: "claude-opus-4-5-20250929",
        max_tokens: 500,
        stream: true,
        messages: [
          {
            role: "user",
            content: "What is 15 + 27? Use the calculator tool.",
          },
        ],
        tools: [
          {
            name: "calculator",
            description: "A calculator that adds two numbers",
            input_schema: {
              type: "object",
              properties: {
                a: { type: "number", description: "First number" },
                b: { type: "number", description: "Second number" },
              },
              required: ["a", "b"],
            },
          },
        ],
      }),
    });

    if (!response.ok) {
      throw new Error(
        `HTTP ${String(response.status)}: ${await response.text()}`,
      );
    }

    const reader = response.body?.getReader();
    if (!reader) throw new Error("No response body");

    const decoder = new TextDecoder();
    let buffer = "";

    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split("\n");
      buffer = lines.pop() ?? "";

      let currentEvent = "message";

      for (const line of lines) {
        if (line.startsWith("event: ")) {
          currentEvent = line.slice(7).trim();
        } else if (line.startsWith("data: ")) {
          const dataStr = line.slice(6);
          try {
            const data: unknown = JSON.parse(dataStr);
            events.push({
              event: currentEvent,
              data,
              timestamp: Date.now() - startTime,
            });
            console.log(
              `[${String(events.length)}] ${currentEvent}:`,
              summarize(data),
            );
          } catch {
            events.push({
              event: currentEvent,
              data: dataStr,
              timestamp: Date.now() - startTime,
            });
          }
        }
      }
    }

    const outputPath = join(__dirname, "collected-events.json");
    writeFileSync(outputPath, JSON.stringify(events, null, 2));
    console.log(
      `\nCollected ${String(events.length)} events, saved to ${outputPath}`,
    );
  } finally {
    await server.stop();
  }
}

function summarize(data: unknown): string {
  if (typeof data !== "object" || data === null) return String(data);

  const obj = data as Record<string, unknown>;
  const type = obj.type;

  if (type === "content_block_delta") {
    const delta = obj.delta as Record<string, unknown> | undefined;
    if (delta?.type === "text_delta" && typeof delta.text === "string") {
      const text = delta.text;
      return `text_delta: "${text.slice(0, 50)}${text.length > 50 ? "..." : ""}"`;
    }
    if (
      delta?.type === "thinking_delta" &&
      typeof delta.thinking === "string"
    ) {
      const thinking = delta.thinking;
      return `thinking_delta: "${thinking.slice(0, 50)}${thinking.length > 50 ? "..." : ""}"`;
    }
    return `delta: ${JSON.stringify(delta)}`;
  }

  if (type === "content_block_start") {
    const block = obj.content_block as Record<string, unknown> | undefined;
    return `block_start: ${String(block?.type)}`;
  }

  return String(type);
}

void collectEvents().catch(console.error);
