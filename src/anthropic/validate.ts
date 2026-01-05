import { z } from "zod";

const textBlock = z.object({ type: z.literal("text"), text: z.string() });
const toolUseBlock = z.object({
  type: z.literal("tool_use"),
  id: z.string(),
  name: z.string(),
  input: z.unknown(),
});
const toolResultBlock = z.object({
  type: z.literal("tool_result"),
  tool_use_id: z.string(),
  content: z.unknown(),
});

const imageSource = z.union([
  z.object({
    type: z.literal("base64"),
    media_type: z.enum(["image/jpeg", "image/png", "image/gif", "image/webp"]),
    data: z.string(),
  }),
  z.object({ type: z.literal("url"), url: z.string() }),
]);

const imageBlock = z.object({ type: z.literal("image"), source: imageSource });

const thinkingBlock = z.object({
  type: z.literal("thinking"),
  thinking: z.string(),
});

const contentBlock = z.union([
  textBlock,
  toolUseBlock,
  toolResultBlock,
  imageBlock,
  thinkingBlock,
]);
const content = z.union([z.string(), z.array(contentBlock)]);

const message = z.object({
  role: z.enum(["user", "assistant"]),
  content,
});

const systemBlock = z.union([
  z.string(),
  z.array(z.object({ type: z.literal("text"), text: z.string() })),
]);

const tool = z.object({
  name: z.string(),
  description: z.string().optional(),
  input_schema: z.record(z.string(), z.unknown()),
});

const toolChoice = z.union([
  z.object({ type: z.literal("auto") }),
  z.object({ type: z.literal("any") }),
  z.object({ type: z.literal("tool"), name: z.string() }),
]);

const thinking = z.object({
  type: z.literal("enabled"),
  budget_tokens: z.number(),
});

const schema = z.object({
  model: z.string(),
  messages: z.array(message),
  system: systemBlock.optional(),
  tools: z.array(tool).optional(),
  stream: z.boolean().optional(),
  max_tokens: z.number().optional(),
  temperature: z.number().optional(),
  top_p: z.number().optional(),
  top_k: z.number().optional(),
  stop_sequences: z.array(z.string()).optional(),
  tool_choice: toolChoice.optional(),
  thinking: thinking.optional(),
  metadata: z.record(z.string(), z.unknown()).optional(),
});

export type ValidatedRequest = z.infer<typeof schema>;

export type ValidationResult =
  | { ok: true; data: ValidatedRequest }
  | { ok: false; error: string };

export function validate(body: unknown): ValidationResult {
  const result = schema.safeParse(body);
  if (result.success) {
    return { ok: true, data: result.data };
  }
  const issue = result.error.issues[0];
  return { ok: false, error: `${issue.path.join(".")}: ${issue.message}` };
}

const countTokensSchema = z.object({
  model: z.string(),
  messages: z.array(message),
  system: systemBlock.optional(),
  tools: z.array(tool).optional(),
  tool_choice: toolChoice.optional(),
  thinking: thinking.optional(),
});

export type CountTokensRequest = z.infer<typeof countTokensSchema>;

export function validateCountTokens(body: unknown): ValidationResult {
  const result = countTokensSchema.safeParse(body);
  if (result.success) {
    return { ok: true, data: result.data as ValidatedRequest };
  }
  const issue = result.error.issues[0];
  return { ok: false, error: `${issue.path.join(".")}: ${issue.message}` };
}
