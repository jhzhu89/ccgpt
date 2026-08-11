import { randomUUID } from "node:crypto";
import OpenAI, { type ClientOptions } from "openai";
import { CopilotAuth } from "../copilot/auth.js";
import { copilotBaseUrl, copilotHeaders } from "../copilot/headers.js";
import type { BackendConfig } from "../config/index.js";

export type OpenAIClient = OpenAI;

function createFetch(auth: CopilotAuth): NonNullable<ClientOptions["fetch"]> {
  return async (input, init): Promise<Response> => {
    const request = new Request(input, init);
    const headers = new Headers(request.headers);
    headers.set("x-request-id", randomUUID());
    const firstRequest = new Request(request, { headers });
    const retryRequest = firstRequest.clone();
    const response = await fetch(firstRequest);
    if (response.status !== 401) return response;

    const token = await auth.getCopilotToken(true);
    const retryHeaders = new Headers(retryRequest.headers);
    retryHeaders.set("authorization", `Bearer ${token}`);
    retryHeaders.set("x-request-id", randomUUID());
    return fetch(new Request(retryRequest, { headers: retryHeaders }));
  };
}

export async function createClient(
  backend: BackendConfig,
): Promise<OpenAIClient> {
  if (backend.kind === "openai") {
    return new OpenAI({
      apiKey: backend.apiKey,
      baseURL: backend.baseURL,
      timeout: 300_000,
    });
  }

  const auth = new CopilotAuth();
  await auth.initialize();

  return new OpenAI({
    apiKey: () => auth.getCopilotToken(),
    baseURL: copilotBaseUrl,
    defaultHeaders: copilotHeaders(),
    fetch: createFetch(auth),
    timeout: 300_000,
  });
}
