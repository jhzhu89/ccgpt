use serde_json::Value;
use tiktoken_rs::o200k_base_singleton;

use crate::ir::Request;
use crate::protocol::build_token_count_payload;

const IMAGE_TOKEN_ESTIMATE: u64 = 85;

pub fn count_request(request: &Request) -> u64 {
    let bpe = o200k_base_singleton();
    let mut payload = build_token_count_payload(request);
    let images = redact_images(&mut payload);
    let json = serde_json::to_string(&payload).unwrap_or_else(|_| "null".into());
    bpe.encode_with_special_tokens(&json).len() as u64 + images * IMAGE_TOKEN_ESTIMATE + 3
}

fn redact_images(value: &mut Value) -> u64 {
    match value {
        Value::Array(items) => items.iter_mut().map(redact_images).sum(),
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("input_image") {
                object.insert("image_url".into(), Value::String(String::new()));
                1
            } else {
                object.values_mut().map(redact_images).sum()
            }
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Content, Message, Role};
    use crate::protocol::parse_request;

    #[test]
    fn tools_and_images_increase_the_estimate() {
        let plain =
            parse_request(br#"{"model":"x","messages":[{"role":"user","content":"hello"}]}"#)
                .unwrap();
        let rich = parse_request(
            br#"{
            "model":"x",
            "messages":[{"role":"user","content":[
                {"type":"text","text":"hello"},
                {"type":"image","source":{"type":"url","url":"https://example.test/a.png"}}
            ]}],
            "tools":[{"name":"lookup","input_schema":{"type":"object","properties":{}}}]
        }"#,
        )
        .unwrap();
        assert!(count_request(&plain) > 0);
        assert!(count_request(&rich) > count_request(&plain) + 80);
    }

    #[test]
    fn replayed_reasoning_is_included_in_the_estimate() {
        let plain =
            parse_request(br#"{"model":"x","messages":[{"role":"assistant","content":"done"}]}"#)
                .unwrap();
        let mut with_reasoning = plain.clone();
        with_reasoning.messages[0].content.insert(
            0,
            Content::Reasoning(serde_json::json!({
                "type": "reasoning",
                "summary": [{"type":"summary_text","text":"worked through it"}],
                "encrypted_content": "encrypted-reasoning-payload".repeat(100)
            })),
        );

        assert!(count_request(&with_reasoning) > count_request(&plain) + 100);
    }

    #[test]
    fn tool_history_uses_the_normalized_responses_shape() {
        let plain = parse_request(br#"{"model":"x","messages":[]}"#).unwrap();
        let mut with_tools = plain.clone();
        with_tools.messages.push(Message {
            role: Role::Assistant,
            content: vec![Content::ToolCall {
                id: "call_123".into(),
                name: "lookup".into(),
                arguments: serde_json::json!({"query":"current model limits"}),
            }],
        });

        assert!(count_request(&with_tools) > count_request(&plain));
    }
}
