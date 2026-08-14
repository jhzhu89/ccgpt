use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::ir::{
    AssistantContent, AssistantResponse, Content, Message, ReasoningEffort, Request, Role, Tool,
    ToolChoice, ToolResultContent,
};

use super::reasoning::{decode_signature, encode_signature};

#[derive(Deserialize)]
struct WireRequest {
    model: String,
    messages: Vec<WireMessage>,
    #[serde(default)]
    system: Option<WireSystem>,
    #[serde(default)]
    tools: Vec<WireTool>,
    #[serde(default)]
    stream: bool,
    max_tokens: Option<u64>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    tool_choice: Option<WireToolChoice>,
    thinking: Option<WireThinking>,
    output_config: Option<WireOutputConfig>,
}

#[derive(Deserialize)]
struct WireMessage {
    role: WireRole,
    content: WireContent,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum WireRole {
    System,
    User,
    Assistant,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum WireContent {
    Text(String),
    Blocks(Vec<WireBlock>),
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireBlock {
    Text {
        text: String,
    },
    Image {
        source: WireImageSource,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: Value,
        #[serde(default)]
        is_error: bool,
    },
    Thinking {
        signature: String,
    },
    MidConvSystem {
        content: Vec<WireTextBlock>,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum WireImageSource {
    Base64 { media_type: String, data: String },
    Url { url: String },
}

#[derive(Deserialize)]
struct WireTextBlock {
    text: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum WireSystem {
    Text(String),
    Blocks(Vec<WireTextBlock>),
}

#[derive(Deserialize)]
struct WireTool {
    name: String,
    description: Option<String>,
    input_schema: Value,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum WireToolChoice {
    Auto {
        #[serde(default)]
        disable_parallel_tool_use: bool,
    },
    Any {
        #[serde(default)]
        disable_parallel_tool_use: bool,
    },
    Tool {
        name: String,
        #[serde(default)]
        disable_parallel_tool_use: bool,
    },
    None,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum WireThinking {
    Enabled { budget_tokens: u64 },
    Adaptive,
    Disabled,
}

#[derive(Deserialize)]
struct WireOutputConfig {
    effort: Option<ReasoningEffort>,
}

pub fn parse_request(body: &[u8]) -> Result<Request, String> {
    let wire: WireRequest = serde_json::from_slice(body).map_err(|error| error.to_string())?;
    if wire.model.trim().is_empty() {
        return Err("model must not be empty".into());
    }

    let mut messages = Vec::with_capacity(wire.messages.len() + usize::from(wire.system.is_some()));
    if let Some(system) = wire.system {
        let text = match system {
            WireSystem::Text(text) => text,
            WireSystem::Blocks(blocks) => blocks
                .into_iter()
                .map(|block| block.text)
                .collect::<Vec<_>>()
                .join("\n"),
        };
        messages.push(Message {
            role: Role::System,
            content: vec![Content::Text(text)],
        });
    }

    for message in wire.messages {
        messages.push(parse_message(message)?);
    }

    let reasoning_effort =
        wire.output_config
            .and_then(|config| config.effort)
            .or(match wire.thinking {
                Some(WireThinking::Disabled) => Some(ReasoningEffort::None),
                Some(WireThinking::Enabled { budget_tokens }) if budget_tokens <= 4_000 => {
                    Some(ReasoningEffort::Low)
                }
                Some(WireThinking::Enabled { budget_tokens }) if budget_tokens <= 16_000 => {
                    Some(ReasoningEffort::Medium)
                }
                Some(WireThinking::Enabled { .. }) => Some(ReasoningEffort::High),
                Some(WireThinking::Adaptive) | None => None,
            });

    Ok(Request {
        model: wire.model,
        messages,
        tools: wire
            .tools
            .into_iter()
            .map(|tool| Tool {
                name: tool.name,
                description: tool.description,
                input_schema: tool.input_schema,
            })
            .collect(),
        stream: wire.stream,
        max_output_tokens: wire.max_tokens,
        temperature: wire.temperature,
        top_p: wire.top_p,
        tool_choice: wire.tool_choice.map(|choice| match choice {
            WireToolChoice::Auto {
                disable_parallel_tool_use,
            } => ToolChoice::Auto {
                disable_parallel: disable_parallel_tool_use,
            },
            WireToolChoice::Any {
                disable_parallel_tool_use,
            } => ToolChoice::Any {
                disable_parallel: disable_parallel_tool_use,
            },
            WireToolChoice::Tool {
                name,
                disable_parallel_tool_use,
            } => ToolChoice::Named {
                name,
                disable_parallel: disable_parallel_tool_use,
            },
            WireToolChoice::None => ToolChoice::None,
        }),
        reasoning_effort,
    })
}

fn parse_message(message: WireMessage) -> Result<Message, String> {
    let role = match message.role {
        WireRole::System => Role::System,
        WireRole::User => Role::User,
        WireRole::Assistant => Role::Assistant,
    };
    let blocks = match message.content {
        WireContent::Text(text) => vec![Content::Text(text)],
        WireContent::Blocks(blocks) => blocks
            .into_iter()
            .filter_map(|block| match block {
                WireBlock::Text { text } => Some(Content::Text(text)),
                WireBlock::Image { source } => Some(Content::Image(match source {
                    WireImageSource::Base64 { media_type, data } => {
                        format!("data:{media_type};base64,{data}")
                    }
                    WireImageSource::Url { url } => url,
                })),
                WireBlock::ToolUse { id, name, input } => Some(Content::ToolCall {
                    id,
                    name,
                    arguments: input,
                }),
                WireBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => Some(Content::ToolResult {
                    id: tool_use_id,
                    content: tool_result_content(content),
                    is_error,
                }),
                WireBlock::Thinking { signature } => {
                    decode_signature(&signature).map(Content::Reasoning)
                }
                WireBlock::MidConvSystem { content } => Some(Content::Text(
                    content
                        .into_iter()
                        .map(|block| block.text)
                        .collect::<Vec<_>>()
                        .join("\n"),
                )),
            })
            .collect(),
    };
    Ok(Message {
        role,
        content: blocks,
    })
}

fn tool_result_content(value: Value) -> Vec<ToolResultContent> {
    match value {
        Value::String(text) => vec![ToolResultContent::Text(text)],
        Value::Array(items) => items.into_iter().map(tool_result_block).collect(),
        other => vec![ToolResultContent::Text(json_text(&other))],
    }
}

fn tool_result_block(value: Value) -> ToolResultContent {
    if value.get("type").and_then(Value::as_str) == Some("text")
        && let Some(text) = value.get("text").and_then(Value::as_str)
    {
        return ToolResultContent::Text(text.to_owned());
    }
    if value.get("type").and_then(Value::as_str) == Some("image")
        && let Some(source) = value.get("source")
        && let Some(url) = image_source_url(source)
    {
        return ToolResultContent::Image(url);
    }
    ToolResultContent::Text(json_text(&value))
}

fn image_source_url(source: &Value) -> Option<String> {
    match source.get("type").and_then(Value::as_str) {
        Some("base64") => Some(format!(
            "data:{};base64,{}",
            source.get("media_type")?.as_str()?,
            source.get("data")?.as_str()?
        )),
        Some("url") => source.get("url")?.as_str().map(str::to_owned),
        _ => None,
    }
}

fn json_text(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".into())
}

pub fn anthropic_response(response: AssistantResponse, requested_model: &str) -> Value {
    let content = response
        .content
        .into_iter()
        .filter_map(|item| match item {
            AssistantContent::Text(text) => Some(json!({ "type": "text", "text": text })),
            AssistantContent::Reasoning(item) => encode_signature(&item).map(
                |signature| json!({ "type": "thinking", "thinking": "", "signature": signature }),
            ),
            AssistantContent::ToolCall {
                id,
                name,
                arguments,
            } => Some(json!({
                "type": "tool_use",
                "id": id,
                "name": name,
                "input": arguments
            })),
        })
        .collect::<Vec<_>>();
    json!({
        "id": format!("msg_{}", Uuid::new_v4().simple()),
        "type": "message",
        "role": "assistant",
        "content": content,
        "model": requested_model,
        "stop_reason": response.stop_reason.as_str(),
        "stop_sequence": null,
        "usage": message_usage(response.input_tokens, response.output_tokens)
    })
}

pub(super) fn message_usage(input_tokens: u64, output_tokens: u64) -> Value {
    json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens
    })
}

pub(super) fn message_delta_usage(input_tokens: u64, output_tokens: u64) -> Value {
    json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens
    })
}

pub fn anthropic_error(error_type: &str, message: impl Into<String>) -> Value {
    json!({
        "type": "error",
        "error": { "type": error_type, "message": message.into() }
    })
}
