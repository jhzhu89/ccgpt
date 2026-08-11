export type Role = "system" | "user" | "assistant";

export type TextContent = { type: "text"; text: string };
export type ImageContent = { type: "image"; url: string };

export type ReasoningItem = {
  id: string;
  type: "reasoning";
  summary: Array<{ type: "summary_text"; text: string }>;
  encrypted_content?: string | null;
  content?: Array<{ type: "reasoning_text"; text: string }>;
  status?: "in_progress" | "completed" | "incomplete";
};

export type ReasoningContent = {
  type: "reasoning";
  item: ReasoningItem;
};

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
  isError?: boolean;
};

export type Content =
  | TextContent
  | ImageContent
  | ReasoningContent
  | ToolCall
  | ToolResult;

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
  | { type: "auto"; disableParallelToolUse?: boolean }
  | { type: "any"; disableParallelToolUse?: boolean }
  | { type: "tool"; name: string; disableParallelToolUse?: boolean }
  | { type: "none" };

export type ReasoningEffort =
  | "none"
  | "low"
  | "medium"
  | "high"
  | "xhigh"
  | "max";

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
  reasoningEffort?: ReasoningEffort;
};

export type StopReason = "end_turn" | "tool_use" | "max_tokens";

export type ResponseContent = ReasoningContent | TextContent | ToolCall;

export type Response = {
  content: ResponseContent[];
  stopReason: StopReason;
  usage: { inputTokens: number; outputTokens: number };
};

export type StreamStart = { type: "stream_start" };
export type TextDelta = { type: "text_delta"; text: string };
export type TextDone = { type: "text_done" };
export type ReasoningDone = { type: "reasoning_done"; item: ReasoningItem };
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
  | ReasoningDone
  | ToolCallStart
  | ToolCallDelta
  | ToolCallDone
  | Done
  | StreamError;
