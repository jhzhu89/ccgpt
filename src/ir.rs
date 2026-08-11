use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    Text(String),
    Image(String),
    Reasoning(Value),
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
    ToolResult {
        id: String,
        content: Vec<ToolResultContent>,
        is_error: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToolResultContent {
    Text(String),
    Image(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub role: Role,
    pub content: Vec<Content>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tool {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToolChoice {
    Auto {
        disable_parallel: bool,
    },
    Any {
        disable_parallel: bool,
    },
    Named {
        name: String,
        disable_parallel: bool,
    },
    None,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl ReasoningEffort {
    pub const ALL: [Self; 7] = [
        Self::None,
        Self::Minimal,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::Xhigh,
        Self::Max,
    ];
}

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<Tool>,
    pub stream: bool,
    pub max_output_tokens: Option<u64>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub tool_choice: Option<ToolChoice>,
    pub reasoning_effort: Option<ReasoningEffort>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Refusal,
}

impl StopReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EndTurn => "end_turn",
            Self::ToolUse => "tool_use",
            Self::MaxTokens => "max_tokens",
            Self::Refusal => "refusal",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AssistantContent {
    Text(String),
    Reasoning(Value),
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssistantResponse {
    pub content: Vec<AssistantContent>,
    pub stop_reason: StopReason,
    pub input_tokens: u64,
    pub output_tokens: u64,
}
