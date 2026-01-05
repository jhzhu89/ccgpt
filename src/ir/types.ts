export type Role = "system" | "user" | "assistant";

export type TextContent = { type: "text"; text: string };
export type ImageContent = { type: "image"; url: string };

export type ToolCall = {
  type: "tool_call";
  id: string;
  name: string;
  arguments: unknown;
};

export type ToolResult = {
  type: "tool_result";
  id: string;
  output: unknown;
};

export type Content = TextContent | ImageContent | ToolCall | ToolResult;

export type Message = {
  role: Role;
  content: Content[];
};

export interface ToolDefinition {
  name: string;
  description?: string;
  inputSchema: Record<string, unknown>;
}

export type ToolChoice =
  | { type: "auto" }
  | { type: "any" }
  | { type: "tool"; name: string };

export type ThinkingConfig = {
  type: "enabled";
  budgetTokens: number;
};

export type Request = {
  model: string;
  messages: Message[];
  tools?: ToolDefinition[];
  stream: boolean;
  maxTokens?: number;
  temperature?: number;
  topP?: number;
  stopSequences?: string[];
  toolChoice?: ToolChoice;
  thinking?: ThinkingConfig;
};

export type StopReason = "end_turn" | "tool_use" | "max_tokens";

export type ResponseContent = TextContent | ToolCall;

export type Response = {
  content: ResponseContent[];
  stopReason: StopReason;
  usage: { inputTokens: number; outputTokens: number };
};

export type StreamStart = { type: "stream_start" };
export type TextDelta = { type: "text_delta"; text: string };
export type TextDone = { type: "text_done" };
export type ThinkingDelta = { type: "thinking_delta"; thinking: string };
export type ThinkingDone = { type: "thinking_done" };
export type ToolCallStart = {
  type: "tool_call_start";
  id: string;
  name: string;
};
export type ToolCallDelta = {
  type: "tool_call_delta";
  id: string;
  args: string;
};
export type ToolCallDone = { type: "tool_call_done"; id: string };
export type Done = {
  type: "done";
  stopReason: StopReason;
  usage: { inputTokens: number; outputTokens: number };
};
export type StreamError = { type: "error"; message: string };

export type StreamEvent =
  | StreamStart
  | TextDelta
  | TextDone
  | ThinkingDelta
  | ThinkingDone
  | ToolCallStart
  | ToolCallDelta
  | ToolCallDone
  | Done
  | StreamError;
