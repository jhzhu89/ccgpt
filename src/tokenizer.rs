use crate::ir::{Content, Request, ToolResultContent};
use tiktoken_rs::{CoreBPE, o200k_base_singleton};

pub fn count_request(request: &Request) -> u64 {
    let bpe = o200k_base_singleton();
    let mut total = 3;
    for message in &request.messages {
        total += 3 + tokens(
            bpe,
            match message.role {
                crate::ir::Role::System => "system",
                crate::ir::Role::User => "user",
                crate::ir::Role::Assistant => "assistant",
            },
        );
        for content in &message.content {
            total += match content {
                Content::Text(text) => tokens(bpe, text),
                Content::Image(_) => 85,
                Content::Reasoning(_) => 0,
                Content::ToolCall {
                    name, arguments, ..
                } => {
                    tokens(bpe, name)
                        + tokens(
                            bpe,
                            &serde_json::to_string(arguments).unwrap_or_else(|_| "null".into()),
                        )
                }
                Content::ToolResult { content, .. } => content
                    .iter()
                    .map(|item| match item {
                        ToolResultContent::Text(text) => tokens(bpe, text),
                        ToolResultContent::Image(_) => 85,
                    })
                    .sum(),
            };
        }
    }
    for tool in &request.tools {
        total += tokens(bpe, &tool.name)
            + tokens(bpe, tool.description.as_deref().unwrap_or_default())
            + tokens(
                bpe,
                &serde_json::to_string(&tool.input_schema).unwrap_or_else(|_| "null".into()),
            )
            + 10;
    }
    total as u64
}

fn tokens(bpe: &CoreBPE, text: &str) -> usize {
    bpe.encode_with_special_tokens(text).len()
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
