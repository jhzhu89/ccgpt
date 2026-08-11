#!/usr/bin/env bun
import { config as loadEnv } from "dotenv";
import { homedir } from "node:os";
import { join } from "node:path";

loadEnv({ path: join(homedir(), ".ccgptrc"), quiet: true });

const { createApp } = await import("./server.js");
const { createClient } = await import("./openai/client.js");
const { config, createCopilotModelRouter, createDirectModelRouter } =
  await import("./config/index.js");
const { logger } = await import("./logger.js");

if (process.argv[2] === "auth") {
  const { CopilotAuth, githubTokenPath } = await import("./copilot/auth.js");
  await new CopilotAuth().login();
  logger.info({ path: githubTokenPath }, "GitHub authentication complete");
  process.exit(0);
}

const client = await createClient(config.backend);
const modelRouter =
  config.backend.kind === "copilot"
    ? createCopilotModelRouter((await client.models.list()).data)
    : createDirectModelRouter(config.backend.targets);
logger.info({ models: modelRouter.targets }, "model routing configured");
const app = createApp(client, (requested) => modelRouter.resolve(requested));
const launchClaude = process.argv[2] === "run";

const server = Bun.serve({
  fetch: app.fetch,
  port: launchClaude ? 0 : config.port,
  ...(launchClaude && { hostname: "127.0.0.1" }),
  idleTimeout: 255,
});

logger.info(
  { port: server.port, backend: config.backend.kind },
  "gateway started",
);

if (launchClaude) {
  let exitCode = 1;
  try {
    const claude = Bun.spawn(["claude", ...process.argv.slice(3)], {
      env: {
        ...process.env,
        ANTHROPIC_BASE_URL: `http://127.0.0.1:${String(server.port)}`,
        ANTHROPIC_AUTH_TOKEN: "ccgpt",
      },
      stdin: "inherit",
      stdout: "inherit",
      stderr: "inherit",
    });
    exitCode = await claude.exited;
  } finally {
    await server.stop();
  }
  process.exit(exitCode);
}
