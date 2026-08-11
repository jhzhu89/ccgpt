use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};

use super::{
    anthropic::{message_delta_usage, message_usage},
    reasoning::encode_signature,
};

static MESSAGE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TextKey {
    item_id: String,
    content_index: u64,
}

struct TextBlock {
    index: usize,
    saw_delta: bool,
}

struct ToolBlock {
    index: usize,
    saw_delta: bool,
    call_id: String,
}

pub struct StreamTranslator {
    model: String,
    message_id: String,
    next_index: usize,
    started: bool,
    terminal: bool,
    saw_tool_call: bool,
    saw_refusal: bool,
    text_blocks: HashMap<TextKey, TextBlock>,
    closed_text: HashSet<TextKey>,
    tool_blocks: HashMap<String, ToolBlock>,
    reasoning_items: HashSet<String>,
    open_blocks: BTreeSet<usize>,
}

impl StreamTranslator {
    pub fn new(model: impl Into<String>) -> Self {
        Self::with_message_id(model, next_message_id())
    }

    pub fn with_message_id(model: impl Into<String>, message_id: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            message_id: message_id.into(),
            next_index: 0,
            started: false,
            terminal: false,
            saw_tool_call: false,
            saw_refusal: false,
            text_blocks: HashMap::new(),
            closed_text: HashSet::new(),
            tool_blocks: HashMap::new(),
            reasoning_items: HashSet::new(),
            open_blocks: BTreeSet::new(),
        }
    }

    pub fn push(&mut self, event: &Value) -> Vec<String> {
        if self.terminal {
            return Vec::new();
        }

        let mut frames = Vec::new();
        match event.get("type").and_then(Value::as_str) {
            Some("response.created") => self.ensure_started(&mut frames),
            Some("response.output_text.delta") => self.text_delta(event, &mut frames),
            Some("response.output_text.done") => self.text_done(event, &mut frames),
            Some("response.refusal.delta") => {
                self.saw_refusal = true;
                self.text_delta(event, &mut frames);
            }
            Some("response.refusal.done") => {
                self.saw_refusal = true;
                self.refusal_done(event, &mut frames);
            }
            Some("response.output_item.added") => self.item_added(event, &mut frames),
            Some("response.function_call_arguments.delta") => {
                self.tool_delta(event, &mut frames);
            }
            Some("response.function_call_arguments.done") => {
                self.tool_done(event, &mut frames);
            }
            Some("response.output_item.done") => self.item_done(event, &mut frames),
            Some("response.completed") => self.complete(event, false, &mut frames),
            Some("response.incomplete") => self.complete(event, true, &mut frames),
            Some("response.failed") | Some("error") => self.fail(event, &mut frames),
            _ => {}
        }
        frames
    }

    pub fn is_terminal(&self) -> bool {
        self.terminal
    }

    pub fn abort(&mut self, message: impl Into<String>) -> Vec<String> {
        if self.terminal {
            return Vec::new();
        }
        let mut frames = Vec::new();
        self.close_all(&mut frames);
        frames.push(frame(
            "error",
            json!({
                "type": "error",
                "error": { "type": "api_error", "message": message.into() }
            }),
        ));
        self.terminal = true;
        frames
    }

    fn ensure_started(&mut self, frames: &mut Vec<String>) {
        if self.started {
            return;
        }
        self.started = true;
        frames.push(frame(
            "message_start",
            json!({
                "type": "message_start",
                "message": {
                    "id": self.message_id,
                    "type": "message",
                    "role": "assistant",
                    "content": [],
                    "model": self.model,
                    "stop_reason": null,
                    "usage": message_usage(0, 0)
                }
            }),
        ));
    }

    fn allocate_index(&mut self) -> usize {
        let index = self.next_index;
        self.next_index += 1;
        self.open_blocks.insert(index);
        index
    }

    fn open_text(&mut self, event: &Value, frames: &mut Vec<String>) -> Option<TextKey> {
        let key = text_key(event)?;
        if self.closed_text.contains(&key) {
            return None;
        }
        if !self.text_blocks.contains_key(&key) {
            self.ensure_started(frames);
            let index = self.allocate_index();
            self.text_blocks.insert(
                key.clone(),
                TextBlock {
                    index,
                    saw_delta: false,
                },
            );
            frames.push(frame(
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": index,
                    "content_block": { "type": "text", "text": "" }
                }),
            ));
        }
        Some(key)
    }

    fn text_delta(&mut self, event: &Value, frames: &mut Vec<String>) {
        let Some(text) = event.get("delta").and_then(Value::as_str) else {
            return;
        };
        let Some(key) = self.open_text(event, frames) else {
            return;
        };
        let Some(block) = self.text_blocks.get_mut(&key) else {
            return;
        };
        block.saw_delta = true;
        frames.push(frame(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": block.index,
                "delta": { "type": "text_delta", "text": text }
            }),
        ));
    }

    fn text_done(&mut self, event: &Value, frames: &mut Vec<String>) {
        self.finish_text(event, event.get("text").and_then(Value::as_str), frames);
    }

    fn refusal_done(&mut self, event: &Value, frames: &mut Vec<String>) {
        self.finish_text(event, event.get("refusal").and_then(Value::as_str), frames);
    }

    fn finish_text(&mut self, event: &Value, full_text: Option<&str>, frames: &mut Vec<String>) {
        let Some(key) = text_key(event) else {
            return;
        };
        if !self.text_blocks.contains_key(&key) && full_text.is_some_and(|text| !text.is_empty()) {
            self.open_text(event, frames);
        }
        let Some(block) = self.text_blocks.get_mut(&key) else {
            return;
        };
        let index = block.index;
        if !block.saw_delta
            && let Some(text) = full_text.filter(|text| !text.is_empty())
        {
            block.saw_delta = true;
            frames.push(frame(
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": index,
                    "delta": { "type": "text_delta", "text": text }
                }),
            ));
        }
        self.close_index(index, frames);
        self.closed_text.insert(key);
    }

    fn item_added(&mut self, event: &Value, frames: &mut Vec<String>) {
        let Some(item) = event.get("item") else {
            return;
        };
        if item.get("type").and_then(Value::as_str) == Some("function_call") {
            self.open_tool(event, item, frames);
        }
    }

    fn open_tool(
        &mut self,
        event: &Value,
        item: &Value,
        frames: &mut Vec<String>,
    ) -> Option<String> {
        let call_id = item.get("call_id").and_then(Value::as_str)?;
        if let Some(item_id) = self
            .tool_blocks
            .iter()
            .find_map(|(item_id, block)| (block.call_id == call_id).then(|| item_id.clone()))
        {
            return Some(item_id);
        }
        let item_id = item
            .get("id")
            .and_then(Value::as_str)
            .or_else(|| event.get("item_id").and_then(Value::as_str))
            .or_else(|| item.get("call_id").and_then(Value::as_str))?
            .to_owned();
        if self.tool_blocks.contains_key(&item_id) {
            return Some(item_id);
        }
        let name = item.get("name").and_then(Value::as_str)?;
        self.ensure_started(frames);
        let index = self.allocate_index();
        self.tool_blocks.insert(
            item_id.clone(),
            ToolBlock {
                index,
                saw_delta: false,
                call_id: call_id.to_owned(),
            },
        );
        self.saw_tool_call = true;
        frames.push(frame(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": index,
                "content_block": {
                    "type": "tool_use",
                    "id": call_id,
                    "name": name,
                    "input": {}
                }
            }),
        ));
        Some(item_id)
    }

    fn tool_delta(&mut self, event: &Value, frames: &mut Vec<String>) {
        let Some(item_id) = event.get("item_id").and_then(Value::as_str) else {
            return;
        };
        let Some(delta) = event.get("delta").and_then(Value::as_str) else {
            return;
        };
        let Some(block) = self.tool_blocks.get_mut(item_id) else {
            return;
        };
        if !self.open_blocks.contains(&block.index) {
            return;
        }
        block.saw_delta = true;
        frames.push(frame(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": block.index,
                "delta": { "type": "input_json_delta", "partial_json": delta }
            }),
        ));
    }

    fn tool_done(&mut self, event: &Value, frames: &mut Vec<String>) {
        let Some(item_id) = event.get("item_id").and_then(Value::as_str) else {
            return;
        };
        self.finish_tool(
            item_id,
            event.get("arguments").and_then(Value::as_str),
            frames,
        );
    }

    fn finish_tool(&mut self, item_id: &str, arguments: Option<&str>, frames: &mut Vec<String>) {
        let Some(block) = self.tool_blocks.get_mut(item_id) else {
            return;
        };
        let index = block.index;
        if !self.open_blocks.contains(&index) {
            return;
        }
        if !block.saw_delta
            && let Some(arguments) = arguments.filter(|value| !value.is_empty())
        {
            block.saw_delta = true;
            frames.push(frame(
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": index,
                    "delta": {
                        "type": "input_json_delta",
                        "partial_json": arguments
                    }
                }),
            ));
        }
        self.close_index(index, frames);
    }

    fn item_done(&mut self, event: &Value, frames: &mut Vec<String>) {
        let Some(item) = event.get("item") else {
            return;
        };
        match item.get("type").and_then(Value::as_str) {
            Some("function_call") => {
                let Some(item_id) = self.open_tool(event, item, frames) else {
                    return;
                };
                self.finish_tool(
                    &item_id,
                    item.get("arguments").and_then(Value::as_str),
                    frames,
                );
            }
            Some("reasoning") => self.reasoning_done(event, item, frames),
            _ => {}
        }
    }

    fn reasoning_done(&mut self, event: &Value, item: &Value, frames: &mut Vec<String>) {
        let key = item
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| event_key(event));
        let Some(key) = key else {
            return;
        };
        if self.reasoning_items.contains(&key) {
            return;
        }
        let Some(signature) = encode_signature(item) else {
            return;
        };
        self.reasoning_items.insert(key);
        self.ensure_started(frames);
        let index = self.allocate_index();
        frames.push(frame(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": index,
                "content_block": { "type": "thinking", "thinking": "", "signature": "" }
            }),
        ));
        frames.push(frame(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": index,
                "delta": { "type": "signature_delta", "signature": signature }
            }),
        ));
        self.close_index(index, frames);
    }

    fn complete(&mut self, event: &Value, incomplete: bool, frames: &mut Vec<String>) {
        self.ensure_started(frames);
        if event
            .pointer("/response/output")
            .and_then(Value::as_array)
            .is_some_and(|items| {
                items
                    .iter()
                    .any(|item| item.get("type").and_then(Value::as_str) == Some("function_call"))
            })
        {
            self.saw_tool_call = true;
        }
        if event
            .pointer("/response/output")
            .and_then(Value::as_array)
            .is_some_and(|items| {
                items.iter().any(|item| {
                    item.get("content")
                        .and_then(Value::as_array)
                        .is_some_and(|parts| {
                            parts.iter().any(|part| {
                                part.get("type").and_then(Value::as_str) == Some("refusal")
                            })
                        })
                })
            })
        {
            self.saw_refusal = true;
        }
        self.close_all(frames);
        let usage = event.pointer("/response/usage").unwrap_or(&Value::Null);
        let output_tokens = usage
            .get("output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let stop_reason = if self.saw_tool_call {
            "tool_use"
        } else if self.saw_refusal
            || event
                .pointer("/response/incomplete_details/reason")
                .and_then(Value::as_str)
                == Some("content_filter")
        {
            "refusal"
        } else if incomplete {
            "max_tokens"
        } else {
            "end_turn"
        };
        frames.push(frame(
            "message_delta",
            json!({
                "type": "message_delta",
                "delta": {
                    "stop_reason": stop_reason
                },
                "usage": message_delta_usage(output_tokens)
            }),
        ));
        frames.push(frame("message_stop", json!({ "type": "message_stop" })));
        self.terminal = true;
    }

    fn fail(&mut self, event: &Value, frames: &mut Vec<String>) {
        let message = event
            .pointer("/response/error/message")
            .and_then(Value::as_str)
            .or_else(|| event.pointer("/error/message").and_then(Value::as_str))
            .or_else(|| event.get("message").and_then(Value::as_str))
            .unwrap_or("Upstream response failed")
            .to_owned();
        frames.extend(self.abort(message));
    }

    fn close_index(&mut self, index: usize, frames: &mut Vec<String>) {
        if self.open_blocks.remove(&index) {
            frames.push(frame(
                "content_block_stop",
                json!({ "type": "content_block_stop", "index": index }),
            ));
        }
    }

    fn close_all(&mut self, frames: &mut Vec<String>) {
        let indices = self.open_blocks.iter().copied().collect::<Vec<_>>();
        for index in indices {
            self.close_index(index, frames);
        }
    }
}

fn text_key(event: &Value) -> Option<TextKey> {
    let item_id = event_key(event).or_else(|| {
        event
            .get("item_id")
            .and_then(Value::as_str)
            .map(str::to_owned)
    })?;
    let content_index = event
        .get("content_index")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    Some(TextKey {
        item_id,
        content_index,
    })
}

fn event_key(event: &Value) -> Option<String> {
    event
        .get("output_index")
        .and_then(Value::as_u64)
        .map(|index| format!("output:{index}"))
}

fn next_message_id() -> String {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let sequence = MESSAGE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("msg_{time:x}{sequence:x}")
}

fn frame(event: &str, data: Value) -> String {
    format!("event: {event}\ndata: {data}\n\n")
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::super::reasoning::decode_signature;

    use super::StreamTranslator;

    fn data(frame: &str) -> Value {
        let line = frame
            .lines()
            .find(|line| line.starts_with("data: "))
            .unwrap();
        serde_json::from_str(&line[6..]).unwrap()
    }

    fn event_name(frame: &str) -> &str {
        frame
            .lines()
            .next()
            .unwrap()
            .strip_prefix("event: ")
            .unwrap()
    }

    #[test]
    fn translates_interleaved_parallel_function_calls() {
        let mut stream = StreamTranslator::with_message_id("claude-opus", "msg_test");
        let started = stream.push(&json!({ "type": "response.created" }));
        assert_eq!(started.len(), 1);
        assert_eq!(
            data(&started[0])["message"]["usage"],
            json!({
                "input_tokens": 0,
                "output_tokens": 0
            })
        );

        let first = stream.push(&json!({
            "type": "response.output_item.added",
            "item": {
                "id": "item_a",
                "type": "function_call",
                "call_id": "call_a",
                "name": "read_a"
            }
        }));
        let second = stream.push(&json!({
            "type": "response.output_item.added",
            "item": {
                "id": "item_b",
                "type": "function_call",
                "call_id": "call_b",
                "name": "read_b"
            }
        }));
        assert_eq!(data(&first[0])["index"], 0);
        assert_eq!(data(&first[0])["content_block"]["id"], "call_a");
        assert_eq!(data(&second[0])["index"], 1);
        assert_eq!(data(&second[0])["content_block"]["id"], "call_b");

        let a1 = stream.push(&json!({
            "type": "response.function_call_arguments.delta",
            "item_id": "item_a",
            "delta": "{\"path\":"
        }));
        let b1 = stream.push(&json!({
            "type": "response.function_call_arguments.delta",
            "item_id": "item_b",
            "delta": "{\"path\":"
        }));
        let a2 = stream.push(&json!({
            "type": "response.function_call_arguments.delta",
            "item_id": "item_a",
            "delta": "\"a\"}"
        }));
        assert_eq!(data(&a1[0])["index"], 0);
        assert_eq!(data(&b1[0])["index"], 1);
        assert_eq!(data(&a2[0])["index"], 0);

        let stop_a = stream.push(&json!({
            "type": "response.function_call_arguments.done",
            "item_id": "item_a",
            "arguments": "{\"path\":\"a\"}"
        }));
        let b2 = stream.push(&json!({
            "type": "response.function_call_arguments.delta",
            "item_id": "item_b",
            "delta": "\"b\"}"
        }));
        let stop_b = stream.push(&json!({
            "type": "response.function_call_arguments.done",
            "item_id": "item_b",
            "arguments": "{\"path\":\"b\"}"
        }));
        assert_eq!(data(&stop_a[0])["index"], 0);
        assert_eq!(data(&b2[0])["index"], 1);
        assert_eq!(data(&stop_b[0])["index"], 1);

        let terminal = stream.push(&json!({
            "type": "response.completed",
            "response": { "usage": { "output_tokens": 17 }, "output": [] }
        }));
        assert_eq!(event_name(&terminal[0]), "message_delta");
        assert_eq!(
            data(&terminal[0])["delta"],
            json!({ "stop_reason": "tool_use" })
        );
        assert_eq!(data(&terminal[0])["usage"], json!({ "output_tokens": 17 }));
        assert_eq!(event_name(&terminal[1]), "message_stop");
    }

    #[test]
    fn does_not_reopen_a_tool_when_done_omits_the_item_id() {
        let mut stream = StreamTranslator::with_message_id("claude-opus", "msg_test");
        let added = stream.push(&json!({
            "type": "response.output_item.added",
            "item": {
                "id": "item_a",
                "type": "function_call",
                "call_id": "call_a",
                "name": "read"
            }
        }));
        assert_eq!(
            added
                .iter()
                .filter(|frame| event_name(frame) == "content_block_start")
                .count(),
            1
        );

        let done = stream.push(&json!({
            "type": "response.output_item.done",
            "item": {
                "type": "function_call",
                "call_id": "call_a",
                "name": "read",
                "arguments": "{\"path\":\"README.md\"}"
            }
        }));
        assert_eq!(
            done.iter()
                .filter(|frame| event_name(frame) == "content_block_start")
                .count(),
            0
        );
        assert_eq!(event_name(&done[0]), "content_block_delta");
        assert_eq!(
            data(&done[0])["delta"]["partial_json"],
            "{\"path\":\"README.md\"}"
        );
        assert_eq!(event_name(&done[1]), "content_block_stop");
    }

    #[test]
    fn maps_text_by_item_and_content_index() {
        let mut stream = StreamTranslator::with_message_id("claude-opus", "msg_test");
        assert!(
            stream
                .push(&json!({
                    "type": "response.content_part.added",
                    "item_id": "message_1",
                    "content_index": 0,
                    "part": { "type": "output_text", "text": "" }
                }))
                .is_empty()
        );
        let first = stream.push(&json!({
            "type": "response.output_text.delta",
            "item_id": "message_1",
            "content_index": 0,
            "delta": "first"
        }));
        assert_eq!(event_name(&first[0]), "message_start");
        assert_eq!(data(&first[1])["index"], 0);

        let delta_second = stream.push(&json!({
            "type": "response.output_text.delta",
            "item_id": "message_1",
            "content_index": 1,
            "delta": "second"
        }));
        assert_eq!(data(&delta_second[0])["index"], 1);

        let stop_first = stream.push(&json!({
            "type": "response.output_text.done",
            "item_id": "message_1",
            "content_index": 0,
            "text": "first"
        }));
        assert!(
            stream
                .push(&json!({
                    "type": "response.content_part.done",
                    "item_id": "message_1",
                    "content_index": 1,
                    "part": { "type": "output_text", "text": "second" }
                }))
                .is_empty()
        );
        let stop_second = stream.push(&json!({
            "type": "response.output_text.done",
            "item_id": "message_1",
            "content_index": 1,
            "text": "second"
        }));
        assert_eq!(data(&stop_first[0])["index"], 0);
        assert_eq!(data(&stop_second[0])["index"], 1);
    }

    #[test]
    fn keeps_one_text_block_when_item_ids_change() {
        let mut stream = StreamTranslator::with_message_id("claude-opus", "msg_test");
        let first = stream.push(&json!({
            "type": "response.output_text.delta",
            "item_id": "unstable_1",
            "output_index": 0,
            "content_index": 0,
            "delta": "first"
        }));
        assert_eq!(event_name(&first[1]), "content_block_start");
        assert_eq!(data(&first[1])["index"], 0);

        let second = stream.push(&json!({
            "type": "response.output_text.delta",
            "item_id": "unstable_2",
            "output_index": 0,
            "content_index": 0,
            "delta": " second"
        }));
        assert_eq!(second.len(), 1);
        assert_eq!(event_name(&second[0]), "content_block_delta");
        assert_eq!(data(&second[0])["index"], 0);

        let done = stream.push(&json!({
            "type": "response.output_text.done",
            "item_id": "unstable_3",
            "output_index": 0,
            "content_index": 0,
            "text": "first second"
        }));
        assert_eq!(done.len(), 1);
        assert_eq!(event_name(&done[0]), "content_block_stop");
        assert_eq!(data(&done[0])["index"], 0);
    }

    #[test]
    fn emits_a_replayable_reasoning_signature() {
        let mut stream = StreamTranslator::with_message_id("claude-opus", "msg_test");
        let frames = stream.push(&json!({
            "type": "response.output_item.done",
            "output_index": 0,
            "item": {
                "id": "reasoning_1",
                "type": "reasoning",
                "status": "completed",
                "summary": [],
                "encrypted_content": "encrypted"
            }
        }));
        assert_eq!(event_name(&frames[0]), "message_start");
        assert_eq!(event_name(&frames[1]), "content_block_start");
        assert_eq!(event_name(&frames[2]), "content_block_delta");
        assert_eq!(event_name(&frames[3]), "content_block_stop");
        let signature = data(&frames[2])["delta"]["signature"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            decode_signature(&signature),
            Some(json!({
                "type": "reasoning",
                "summary": [],
                "encrypted_content": "encrypted"
            }))
        );
    }

    #[test]
    fn closes_open_blocks_on_incomplete_and_reports_max_tokens() {
        let mut stream = StreamTranslator::with_message_id("claude-opus", "msg_test");
        stream.push(&json!({
            "type": "response.output_text.delta",
            "item_id": "message_1",
            "content_index": 0,
            "delta": "partial"
        }));

        let frames = stream.push(&json!({
            "type": "response.incomplete",
            "response": { "usage": { "output_tokens": 9 } }
        }));
        assert_eq!(event_name(&frames[0]), "content_block_stop");
        assert_eq!(event_name(&frames[1]), "message_delta");
        assert_eq!(data(&frames[1])["delta"]["stop_reason"], "max_tokens");
        assert_eq!(data(&frames[1])["usage"]["output_tokens"], 9);
        assert_eq!(event_name(&frames[2]), "message_stop");
        assert!(
            stream
                .push(&json!({ "type": "response.completed" }))
                .is_empty()
        );
    }

    #[test]
    fn streams_refusals_and_reports_the_refusal_stop() {
        let mut stream = StreamTranslator::with_message_id("claude-opus", "msg_test");
        let delta = stream.push(&json!({
            "type": "response.refusal.delta",
            "item_id": "message_1",
            "content_index": 0,
            "delta": "I cannot help."
        }));
        assert_eq!(event_name(&delta[0]), "message_start");
        assert_eq!(event_name(&delta[1]), "content_block_start");
        assert_eq!(data(&delta[2])["delta"]["text"], "I cannot help.");

        let done = stream.push(&json!({
            "type": "response.refusal.done",
            "item_id": "message_1",
            "content_index": 0,
            "refusal": "I cannot help."
        }));
        assert_eq!(event_name(&done[0]), "content_block_stop");

        let terminal = stream.push(&json!({
            "type": "response.incomplete",
            "response": {
                "incomplete_details": { "reason": "content_filter" },
                "usage": { "output_tokens": 4 },
                "output": []
            }
        }));
        assert_eq!(data(&terminal[0])["delta"]["stop_reason"], "refusal");
        assert_eq!(event_name(&terminal[1]), "message_stop");
    }

    #[test]
    fn closes_open_blocks_on_failure_and_tolerates_unknown_events() {
        let mut stream = StreamTranslator::with_message_id("claude-opus", "msg_test");
        assert!(
            stream
                .push(&json!({ "type": "response.unknown" }))
                .is_empty()
        );
        stream.push(&json!({
            "type": "response.output_text.delta",
            "item_id": "message_1",
            "content_index": 0,
            "delta": "partial"
        }));

        let frames = stream.push(&json!({
            "type": "response.failed",
            "response": { "error": { "message": "model failed" } }
        }));
        assert_eq!(event_name(&frames[0]), "content_block_stop");
        assert_eq!(event_name(&frames[1]), "error");
        assert_eq!(data(&frames[1])["error"]["message"], "model failed");
        assert!(
            stream
                .push(&json!({ "type": "error", "message": "late" }))
                .is_empty()
        );
    }

    #[test]
    fn emits_standalone_stream_errors() {
        let mut stream = StreamTranslator::with_message_id("claude-opus", "msg_test");
        let frames = stream.push(&json!({
            "type": "error",
            "message": "connection lost"
        }));
        assert_eq!(frames.len(), 1);
        assert_eq!(event_name(&frames[0]), "error");
        assert_eq!(data(&frames[0])["error"]["message"], "connection lost");
    }
}
