#!/usr/bin/env bun
import { config as loadEnv } from "dotenv";
import { homedir } from "node:os";
import { join } from "node:path";

loadEnv({ path: join(homedir(), ".m2rrc") });

const { createApp } = await import("./server.js");
const { createClient } = await import("./openai/client.js");
const { config } = await import("./config/index.js");
const { logger } = await import("./logger.js");

const app = createApp(createClient());

const server = Bun.serve({
  fetch: app.fetch,
  port: config.port,
});

logger.info({ port: server.port }, "proxy server started");
