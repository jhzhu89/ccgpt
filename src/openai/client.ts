import { AzureOpenAI } from "openai";
import {
  DefaultAzureCredential,
  getBearerTokenProvider,
} from "@azure/identity";
import { config } from "../config/index.js";

export type OpenAIClient = AzureOpenAI;

export function createClient(): OpenAIClient {
  const credential = new DefaultAzureCredential();
  const scope = "https://cognitiveservices.azure.com/.default";
  const azureADTokenProvider = getBearerTokenProvider(credential, scope);

  return new AzureOpenAI({
    azureADTokenProvider,
    endpoint: config.azure.endpoint,
    apiVersion: config.azure.apiVersion,
    timeout: 300_000, // 5 minutes for long reasoning requests
  });
}
