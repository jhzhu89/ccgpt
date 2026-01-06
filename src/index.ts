#!/usr/bin/env bun
import { createApp } from "./server.js";
import { createClient } from "./openai/client.js";
import { config } from "./config/index.js";
import { logger } from "./logger.js";

const app = createApp(createClient());

const server = Bun.serve({
  fetch: app.fetch,
  port: config.port,
});

logger.info({ port: server.port }, "proxy server started");
