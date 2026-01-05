import { AzureOpenAI } from "openai";
import {
  DefaultAzureCredential,
  getBearerTokenProvider,
} from "@azure/identity";
import { config } from "../config/index.js";

const credential = new DefaultAzureCredential();
const scope = "https://cognitiveservices.azure.com/.default";
const azureADTokenProvider = getBearerTokenProvider(credential, scope);

export const openai = new AzureOpenAI({
  azureADTokenProvider,
  endpoint: config.azure.endpoint,
  apiVersion: config.azure.apiVersion,
});
