import { serve } from "@hono/node-server";
import { app } from "./server.js";
import { config } from "./config/index.js";
import { logger } from "./logger.js";

serve({ fetch: app.fetch, port: config.port }, (info) => {
  logger.info({ port: info.port }, "proxy server started");
});
