export const copilotBaseUrl = "https://api.githubcopilot.com";

const vscodeVersion = "1.131.0";
const copilotChatVersion = "0.26.7";
const userAgent = `GitHubCopilotChat/${copilotChatVersion}`;

export function githubHeaders(token?: string): Record<string, string> {
  return {
    accept: "application/json",
    "content-type": "application/json",
    ...(token && { authorization: `token ${token}` }),
    "editor-version": `vscode/${vscodeVersion}`,
    "editor-plugin-version": `copilot-chat/${copilotChatVersion}`,
    "user-agent": userAgent,
    "x-github-api-version": "2025-04-01",
    "x-vscode-user-agent-library-version": "electron-fetch",
  };
}

export function copilotHeaders(): Record<string, string> {
  return {
    "copilot-integration-id": "vscode-chat",
    "editor-version": `vscode/${vscodeVersion}`,
    "editor-plugin-version": `copilot-chat/${copilotChatVersion}`,
    "openai-intent": "conversation-panel",
    "user-agent": userAgent,
    "x-github-api-version": "2025-04-01",
    "x-initiator": "agent",
    "x-vscode-user-agent-library-version": "electron-fetch",
  };
}
