import { chmod, mkdir, readFile, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { githubHeaders } from "./headers.js";

const githubBaseUrl = "https://github.com";
const githubApiBaseUrl = "https://api.github.com";
const githubClientId = "Iv1.b507a08c87ecfe98";

export const githubTokenPath = join(
  homedir(),
  ".local",
  "share",
  "ccgpt",
  "github_token",
);

interface DeviceCode {
  device_code: string;
  user_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
}

interface AccessTokenResult {
  access_token?: string;
  error?: string;
}

interface CopilotToken {
  token: string;
  expires_at: number;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function parseDeviceCode(value: unknown): DeviceCode {
  if (
    !isRecord(value) ||
    typeof value.device_code !== "string" ||
    typeof value.user_code !== "string" ||
    typeof value.verification_uri !== "string" ||
    typeof value.expires_in !== "number" ||
    typeof value.interval !== "number"
  ) {
    throw new Error("GitHub returned an invalid device code response");
  }
  return {
    device_code: value.device_code,
    user_code: value.user_code,
    verification_uri: value.verification_uri,
    expires_in: value.expires_in,
    interval: value.interval,
  };
}

function parseAccessToken(value: unknown): AccessTokenResult {
  if (!isRecord(value)) {
    throw new Error("GitHub returned an invalid access token response");
  }
  return {
    access_token:
      typeof value.access_token === "string" ? value.access_token : undefined,
    error: typeof value.error === "string" ? value.error : undefined,
  };
}

function parseCopilotToken(value: unknown): CopilotToken {
  if (
    !isRecord(value) ||
    typeof value.token !== "string" ||
    typeof value.expires_at !== "number"
  ) {
    throw new Error("GitHub returned an invalid Copilot token response");
  }
  return { token: value.token, expires_at: value.expires_at };
}

async function responseError(response: Response): Promise<Error> {
  const detail = (await response.text()).slice(0, 500);
  return new Error(
    `GitHub request failed (${String(response.status)}): ${detail}`,
  );
}

async function sleep(milliseconds: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, milliseconds));
}

export class CopilotAuth {
  private githubToken?: string;
  private token?: CopilotToken;
  private refresh?: Promise<string>;

  async initialize(): Promise<void> {
    try {
      this.githubToken = (await readFile(githubTokenPath, "utf8")).trim();
    } catch {
      throw new Error(
        "GitHub authentication required. Run `ccgpt auth` first.",
      );
    }
    if (!this.githubToken) {
      throw new Error(
        "GitHub authentication required. Run `ccgpt auth` first.",
      );
    }
    await this.getCopilotToken();
  }

  async login(): Promise<void> {
    const response = await fetch(`${githubBaseUrl}/login/device/code`, {
      method: "POST",
      headers: githubHeaders(),
      body: JSON.stringify({ client_id: githubClientId, scope: "read:user" }),
    });
    if (!response.ok) throw await responseError(response);

    const device = parseDeviceCode(await response.json());
    process.stdout.write(
      `Open ${device.verification_uri} and enter code ${device.user_code}\n`,
    );

    const token = await this.pollAccessToken(device);
    await mkdir(dirname(githubTokenPath), { recursive: true });
    await writeFile(githubTokenPath, token, { mode: 0o600 });
    await chmod(githubTokenPath, 0o600);
    this.githubToken = token;
    this.token = undefined;
    await this.getCopilotToken();
  }

  async getCopilotToken(force = false): Promise<string> {
    if (!this.githubToken) {
      throw new Error(
        "GitHub authentication required. Run `ccgpt auth` first.",
      );
    }
    if (
      !force &&
      this.token &&
      this.token.expires_at * 1000 > Date.now() + 60_000
    ) {
      return this.token.token;
    }
    if (this.refresh) return this.refresh;

    this.refresh = this.fetchCopilotToken().finally(() => {
      this.refresh = undefined;
    });
    return this.refresh;
  }

  private async fetchCopilotToken(): Promise<string> {
    const response = await fetch(
      `${githubApiBaseUrl}/copilot_internal/v2/token`,
      { headers: githubHeaders(this.githubToken) },
    );
    if (!response.ok) throw await responseError(response);
    this.token = parseCopilotToken(await response.json());
    return this.token.token;
  }

  private async pollAccessToken(device: DeviceCode): Promise<string> {
    const deadline = Date.now() + device.expires_in * 1000;
    let interval = device.interval * 1000;

    while (Date.now() < deadline) {
      await sleep(interval);
      const response = await fetch(
        `${githubBaseUrl}/login/oauth/access_token`,
        {
          method: "POST",
          headers: githubHeaders(),
          body: JSON.stringify({
            client_id: githubClientId,
            device_code: device.device_code,
            grant_type: "urn:ietf:params:oauth:grant-type:device_code",
          }),
        },
      );
      if (!response.ok) throw await responseError(response);

      const result = parseAccessToken(await response.json());
      if (result.access_token) return result.access_token;
      if (result.error === "slow_down") {
        interval += 5_000;
        continue;
      }
      if (result.error && result.error !== "authorization_pending") {
        throw new Error(`GitHub authentication failed: ${result.error}`);
      }
    }
    throw new Error("GitHub authentication timed out");
  }
}
