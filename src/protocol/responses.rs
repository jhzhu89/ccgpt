use serde_json::{Map, Value, json};

use crate::ir::{
    AssistantContent, AssistantResponse, Content, Message, ReasoningEffort, Request, Role,
    StopReason, Tool, ToolChoice, ToolResultContent,
};

pub struct BuildOptions<'a> {
    pub model: &'a str,
    pub supports_parallel_tool_calls: bool,
    pub reasoning_efforts: &'a [ReasoningEffort],
}

pub fn build_responses_request(request: &Request, options: BuildOptions<'_>) -> Value {
    let mut body = Map::from_iter([
        ("model".into(), Value::String(options.model.into())),
        ("input".into(), Value::Array(build_input(&request.messages))),
        ("store".into(), Value::Bool(false)),
    ]);

    if !request.tools.is_empty() {
        body.insert(
            "tools".into(),
            Value::Array(request.tools.iter().map(tool_json).collect()),
        );
        if options.supports_parallel_tool_calls {
            let disabled = match &request.tool_choice {
                Some(ToolChoice::Auto { disable_parallel })
                | Some(ToolChoice::Any { disable_parallel })
                | Some(ToolChoice::Named {
                    disable_parallel, ..
                }) => *disable_parallel,
                Some(ToolChoice::None) => true,
                None => false,
            };
            body.insert("parallel_tool_calls".into(), Value::Bool(!disabled));
        }
    }

    if let Some(choice) = request.tool_choice.as_ref().and_then(tool_choice_json) {
        body.insert("tool_choice".into(), choice);
    }

    let effort = select_effort(request.reasoning_effort, options.reasoning_efforts);
    if let Some(effort) = effort {
        body.insert("reasoning".into(), json!({ "effort": effort }));
    }
    if let Some(value) = request.max_output_tokens {
        body.insert("max_output_tokens".into(), value.into());
    }
    if let Some(value) = request.temperature {
        body.insert("temperature".into(), json!(value));
    }
    if let Some(value) = request.top_p {
        body.insert("top_p".into(), json!(value));
    }
    if request.stream {
        body.insert("stream".into(), Value::Bool(true));
    }
    Value::Object(body)
}

fn select_effort(
    requested: Option<ReasoningEffort>,
    supported: &[ReasoningEffort],
) -> Option<ReasoningEffort> {
    if supported.is_empty() {
        return None;
    }
    let requested = requested.or_else(|| {
        supported
            .contains(&ReasoningEffort::Medium)
            .then_some(ReasoningEffort::Medium)
    })?;
    supported.iter().copied().min_by_key(|candidate| {
        let left = ReasoningEffort::ALL
            .iter()
            .position(|effort| effort == candidate)
            .unwrap_or_default();
        let right = ReasoningEffort::ALL
            .iter()
            .position(|effort| *effort == requested)
            .unwrap_or_default();
        (left.abs_diff(right), usize::MAX - left)
    })
}

fn build_input(messages: &[Message]) -> Vec<Value> {
    let mut input = Vec::new();
    for message in messages {
        match message.role {
            Role::User => build_user_input(&mut input, &message.content),
            Role::System | Role::Assistant => build_message_input(&mut input, message),
        }
    }
    input
}

fn build_user_input(input: &mut Vec<Value>, content: &[Content]) {
    let mut parts = Vec::new();
    for item in content {
        match item {
            Content::Text(text) => parts.push(json!({ "type": "input_text", "text": text })),
            Content::Image(url) => parts.push(json!({
                "type": "input_image",
                "image_url": url,
                "detail": "auto"
            })),
            Content::ToolResult {
                id,
                content,
                is_error,
            } => {
                flush_parts(input, "user", &mut parts);
                input.push(json!({
                    "type": "function_call_output",
                    "call_id": id,
                    "output": tool_result_output(content, *is_error)
                }));
            }
            Content::Reasoning(item) => {
                flush_parts(input, "user", &mut parts);
                input.push(item.clone());
            }
            Content::ToolCall {
                id,
                name,
                arguments,
            } => {
                flush_parts(input, "user", &mut parts);
                input.push(function_call_json(id, name, arguments));
            }
        }
    }
    flush_parts(input, "user", &mut parts);
}

fn build_message_input(input: &mut Vec<Value>, message: &Message) {
    let role = if message.role == Role::System {
        "system"
    } else {
        "assistant"
    };
    let phase = (message.role == Role::Assistant).then(|| {
        if message
            .content
            .iter()
            .any(|item| matches!(item, Content::ToolCall { .. }))
        {
            "commentary"
        } else {
            "final_answer"
        }
    });
    let mut text = String::new();
    let flush_text = |input: &mut Vec<Value>, text: &mut String| {
        if !text.is_empty() {
            let mut item = Map::from_iter([
                ("type".into(), Value::String("message".into())),
                ("role".into(), Value::String(role.into())),
                ("content".into(), Value::String(std::mem::take(text))),
            ]);
            if let Some(phase) = phase {
                item.insert("phase".into(), Value::String(phase.into()));
            }
            input.push(Value::Object(item));
        }
    };

    for item in &message.content {
        match item {
            Content::Text(value) => text.push_str(value),
            Content::Reasoning(value) => {
                flush_text(input, &mut text);
                input.push(value.clone());
            }
            Content::ToolCall {
                id,
                name,
                arguments,
            } => {
                flush_text(input, &mut text);
                input.push(function_call_json(id, name, arguments));
            }
            Content::ToolResult {
                id,
                content,
                is_error,
            } => {
                flush_text(input, &mut text);
                input.push(json!({
                    "type": "function_call_output",
                    "call_id": id,
                    "output": tool_result_output(content, *is_error)
                }));
            }
            Content::Image(_) => {}
        }
    }
    flush_text(input, &mut text);
}

fn flush_parts(input: &mut Vec<Value>, role: &str, parts: &mut Vec<Value>) {
    if parts.is_empty() {
        return;
    }
    input.push(json!({
        "type": "message",
        "role": role,
        "content": std::mem::take(parts)
    }));
}

fn function_call_json(id: &str, name: &str, arguments: &Value) -> Value {
    json!({
        "type": "function_call",
        "call_id": id,
        "name": name,
        "arguments": serde_json::to_string(arguments).unwrap_or_else(|_| "null".into())
    })
}

fn tool_result_output(content: &[ToolResultContent], is_error: bool) -> Value {
    if !content
        .iter()
        .any(|item| matches!(item, ToolResultContent::Image(_)))
    {
        let text = content
            .iter()
            .filter_map(|item| match item {
                ToolResultContent::Text(text) => Some(text.as_str()),
                ToolResultContent::Image(_) => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        return Value::String(if is_error {
            format!("Error: {text}")
        } else {
            text
        });
    }

    let mut output = Vec::with_capacity(content.len() + usize::from(is_error));
    if is_error {
        output.push(json!({ "type": "input_text", "text": "Error:" }));
    }
    output.extend(content.iter().map(|item| match item {
        ToolResultContent::Text(text) => json!({ "type": "input_text", "text": text }),
        ToolResultContent::Image(url) => json!({
            "type": "input_image",
            "image_url": url,
            "detail": "auto"
        }),
    }));
    Value::Array(output)
}

fn tool_json(tool: &Tool) -> Value {
    let mut value = Map::from_iter([
        ("type".into(), Value::String("function".into())),
        ("name".into(), Value::String(tool.name.clone())),
        ("parameters".into(), tool.input_schema.clone()),
        ("strict".into(), Value::Bool(false)),
    ]);
    if let Some(description) = &tool.description {
        value.insert("description".into(), Value::String(description.clone()));
    }
    Value::Object(value)
}

fn tool_choice_json(choice: &ToolChoice) -> Option<Value> {
    match choice {
        ToolChoice::Auto { .. } => None,
        ToolChoice::Any { .. } => Some(Value::String("required".into())),
        ToolChoice::Named { name, .. } => Some(json!({ "type": "function", "name": name })),
        ToolChoice::None => Some(Value::String("none".into())),
    }
}

pub fn parse_responses_response(value: &Value) -> Result<AssistantResponse, String> {
    if value.get("status").and_then(Value::as_str) == Some("failed") {
        let message = value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("Upstream response failed");
        return Err(message.to_owned());
    }
    let output = value
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| "Responses output is missing".to_string())?;
    let mut content = Vec::new();
    let mut has_tool_call = false;
    let mut has_refusal = false;

    for item in output {
        match item.get("type").and_then(Value::as_str) {
            Some("reasoning")
                if item
                    .get("encrypted_content")
                    .and_then(Value::as_str)
                    .is_some() =>
            {
                content.push(AssistantContent::Reasoning(item.clone()));
            }
            Some("message") => {
                if let Some(parts) = item.get("content").and_then(Value::as_array) {
                    for part in parts {
                        match part.get("type").and_then(Value::as_str) {
                            Some("output_text") => {
                                if let Some(text) = part.get("text").and_then(Value::as_str) {
                                    content.push(AssistantContent::Text(text.into()));
                                }
                            }
                            Some("refusal") => {
                                if let Some(text) = part.get("refusal").and_then(Value::as_str) {
                                    content.push(AssistantContent::Text(text.into()));
                                    has_refusal = true;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Some("function_call") => {
                let id = string_field(item, "call_id")?;
                let name = string_field(item, "name")?;
                let raw = string_field(item, "arguments")?;
                let arguments = serde_json::from_str(raw)
                    .map_err(|error| format!("Invalid function arguments: {error}"))?;
                content.push(AssistantContent::ToolCall {
                    id: id.into(),
                    name: name.into(),
                    arguments,
                });
                has_tool_call = true;
            }
            _ => {}
        }
    }

    let stop_reason = if has_tool_call {
        StopReason::ToolUse
    } else if has_refusal
        || value
            .pointer("/incomplete_details/reason")
            .and_then(Value::as_str)
            == Some("content_filter")
    {
        StopReason::Refusal
    } else if value.get("status").and_then(Value::as_str) == Some("incomplete") {
        StopReason::MaxTokens
    } else {
        StopReason::EndTurn
    };
    let usage = value.get("usage").and_then(Value::as_object);
    Ok(AssistantResponse {
        content,
        stop_reason,
        input_tokens: usage
            .and_then(|usage| usage.get("input_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        output_tokens: usage
            .and_then(|usage| usage.get("output_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or_default(),
    })
}

fn string_field<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Responses {field} is missing"))
}

#[cfg(test)]
mod tests {
    use super::super::anthropic::parse_request;
    use super::*;

    fn options() -> BuildOptions<'static> {
        BuildOptions {
            model: "gpt-5.6-sol",
            supports_parallel_tool_calls: true,
            reasoning_efforts: &ReasoningEffort::ALL,
        }
    }

    #[test]
    fn translates_full_request_and_preserves_order() {
        let request = parse_request(
            br#"{
                "model":"claude-opus-4-1",
                "system":[{"type":"text","text":"system"},{"type":"text","text":"rules"}],
                "messages":[
                  {"role":"user","content":[
                    {"type":"text","text":"before"},
                    {"type":"image","source":{"type":"base64","media_type":"image/png","data":"AA=="}},
                    {"type":"tool_result","tool_use_id":"call_1","content":[
                      {"type":"text","text":"done"},
                      {"type":"image","source":{"type":"base64","media_type":"image/png","data":"AQ=="}}
                    ]},
                    {"type":"text","text":"after"}
                  ]}
                ],
                "tools":[{"name":"shell","description":"Run","input_schema":{"type":"object","properties":{}}}],
                "tool_choice":{"type":"auto"},
                "thinking":{"type":"enabled","budget_tokens":20000},
                "max_tokens":4096,
                "stream":true
            }"#,
        )
        .unwrap();
        let value = build_responses_request(&request, options());

        assert_eq!(value["store"], false);
        assert!(value.get("include").is_none());
        assert_eq!(value["reasoning"]["effort"], "high");
        assert_eq!(value["parallel_tool_calls"], true);
        assert_eq!(value["input"].as_array().unwrap().len(), 4);
        assert_eq!(value["input"][1]["content"][1]["type"], "input_image");
        assert_eq!(value["input"][2]["type"], "function_call_output");
        assert_eq!(value["input"][2]["output"][0]["type"], "input_text");
        assert_eq!(value["input"][2]["output"][0]["text"], "done");
        assert_eq!(value["input"][2]["output"][1]["type"], "input_image");
        assert_eq!(
            value["input"][2]["output"][1]["image_url"],
            "data:image/png;base64,AQ=="
        );
        assert_eq!(value["input"][3]["content"][0]["text"], "after");
    }

    #[test]
    fn output_effort_wins_and_maps_to_nearest_capability() {
        let request = parse_request(
            br#"{"model":"x","messages":[],"thinking":{"type":"disabled"},"output_config":{"effort":"max"}}"#,
        )
        .unwrap();
        let value = build_responses_request(
            &request,
            BuildOptions {
                model: "gpt",
                supports_parallel_tool_calls: false,
                reasoning_efforts: &[ReasoningEffort::Low, ReasoningEffort::High],
            },
        );
        assert_eq!(value["reasoning"]["effort"], "high");
    }

    #[test]
    fn parses_non_streaming_tool_response() {
        let response = parse_responses_response(&json!({
            "status": "completed",
            "output": [
                {"type":"message","content":[{"type":"output_text","text":"checking"}]},
                {"type":"function_call","call_id":"call_1","name":"shell","arguments":"{\"cmd\":\"pwd\"}"}
            ],
            "usage":{"input_tokens":10,"output_tokens":4}
        }))
        .unwrap();
        assert_eq!(response.stop_reason, StopReason::ToolUse);
        assert_eq!(response.content.len(), 2);
        assert_eq!(response.input_tokens, 10);
    }

    #[test]
    fn phases_assistant_history_by_turn_completion() {
        let request = parse_request(
            br#"{
                "model":"x",
                "messages":[
                    {"role":"assistant","content":[
                        {"type":"text","text":"I will inspect it."},
                        {"type":"tool_use","id":"call_1","name":"read","input":{}}
                    ]},
                    {"role":"assistant","content":"The bug is fixed."}
                ]
            }"#,
        )
        .unwrap();

        let value = build_responses_request(&request, options());

        assert_eq!(value["input"][0]["phase"], "commentary");
        assert_eq!(value["input"][2]["phase"], "final_answer");
    }

    #[test]
    fn rejects_failed_non_streaming_responses() {
        let error = parse_responses_response(&json!({
            "status": "failed",
            "error": { "message": "model failed" },
            "output": []
        }))
        .unwrap_err();

        assert_eq!(error, "model failed");
    }

    #[test]
    fn preserves_refusals_and_content_filter_stops() {
        let response = parse_responses_response(&json!({
            "status": "incomplete",
            "incomplete_details": { "reason": "content_filter" },
            "output": [{
                "type": "message",
                "content": [{"type":"refusal","refusal":"I cannot help with that."}]
            }],
            "usage": {"input_tokens": 7, "output_tokens": 4}
        }))
        .unwrap();

        assert_eq!(response.stop_reason, StopReason::Refusal);
        assert_eq!(
            response.content,
            vec![AssistantContent::Text("I cannot help with that.".into())]
        );
    }
}
