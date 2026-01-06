#!/usr/bin/env bun
import { app } from "./server.js";
import { config } from "./config/index.js";
import { logger } from "./logger.js";

const server = Bun.serve({
  fetch: app.fetch,
  port: config.port,
});

logger.info({ port: server.port }, "proxy server started");
